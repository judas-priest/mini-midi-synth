//! Drum sequencer, pitch sequencer, and looper UI.

use std::sync::atomic::Ordering;
use eframe::egui;

use crate::preset::{self, DrumKit};
use crate::synth::{ControlEvent, DrumParam};
use crate::synth::drum::{NUM_DRUM_SLOTS, DRUM_NAMES, DrumSlotParams, DrumPattern};
use super::{App, ToastKind};
use super::keyboard::{PAD_BOT_NOTES, PAD_TOP_NOTES};
use super::theme;

impl App {
    /// Drum step-sequencer strip, docked to the window bottom on every tab
    /// (TECHDEBT #25): transport + pattern selector + step grid.
    /// Mix (BPM/Swing/Vol/V127/Metro), Remap and kit I/O stay on the Drums
    /// tab — draw_drum_kit_controls. Remap modals are global (TECHDEBT #11).
    pub(super) fn draw_drum_seq_docked(&mut self, ui: &mut egui::Ui) {
        let current_step = self.drum_step_atom.load(Ordering::Relaxed);
        let pat_idx = self.drum_current_pattern as usize;



        // ── Single master transport (#32 фаза 4) ─────────────────────────
        let looper_undo_avail = {
            let a = &self.looper_atoms;
            a.layer_count.load(std::sync::atomic::Ordering::Relaxed) > 1
                && a.event_count.load(std::sync::atomic::Ordering::Relaxed) > 0
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Master").strong().small());
            let lstate = crate::synth::looper::LooperState::from_u8(
                self.looper_atoms.state.load(std::sync::atomic::Ordering::Relaxed));
            let looper_active = matches!(lstate,
                crate::synth::looper::LooperState::Playing
                | crate::synth::looper::LooperState::Overdubbing
                | crate::synth::looper::LooperState::Recording);
            let any_playing = self.drum_playing || looper_active;
            let play_label = if any_playing { "\u{23F9} Stop" } else { "\u{25B6} Play" };
            if ui.button(play_label).on_hover_text("Master: drums + looper together").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::MasterPlay);
            }
            let rec = self.drum_recording
                || matches!(lstate,
                    crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing);
            let rec_label = if rec { "\u{23FA} Rec" } else { "\u{26AB} Rec" };
            let rec_color = if rec { egui::Color32::RED } else { egui::Color32::GRAY };
            if ui.button(egui::RichText::new(rec_label).color(rec_color))
                .on_hover_text("Master record: drums → grid, notes → looper (loop = 1 pattern cycle)").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::MasterRecord);
            }
            // Overdub (слой поверх готовой петли): старт/закрытие на границе
            let ovr_enabled = lstate == crate::synth::looper::LooperState::Playing;
            if ui.add_enabled(ovr_enabled, egui::Button::new("Слой"))
                .on_hover_text("Слой: дописать поверх петли — старт на границе, ещё раз = закрыть").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::MasterOverdub);
            }

            // Downbeat flash indicator (lights ~150 ms after beat 1)
            let flashing = self.drum_beat_flash
                .map(|t| t.elapsed() < std::time::Duration::from_millis(150))
                .unwrap_or(false);
            let (flash_rect, _) =
                ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(
                flash_rect.center(), 5.0,
                if flashing { egui::Color32::from_rgb(255, 190, 60) }
                else { egui::Color32::from_gray(45) },
            );

            if ui.button("Undo").on_hover_text("Undo last sequencer edit").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqUndo);
            }
            if ui.button("Clear").on_hover_text("Clear all steps").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqClear);
            }
            if ui.add_enabled(looper_undo_avail, egui::Button::new("L-Undo"))
                .on_hover_text("Undo last looper layer").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::LooperUndo);
            }

            ui.separator();
            ui.label("BPM:");
            if ui.add(egui::DragValue::new(&mut self.drum_bpm).range(40.0..=300.0).speed(0.5)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqBpm { bpm: self.drum_bpm });
            }
            // Паттерн мутации поля — как drum_bpm выше (capture &mut self).
            // Персист обязателен: старый чекбокс звал save_global_to_config().
            if ui.checkbox(&mut self.drum_metronome, "Metro").on_hover_text("Metronome click on beats").changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSetMetronome { on: self.drum_metronome });
                self.save_global_to_config();
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
            // Time-signature quick buttons (TECHDEBT #21). The sequencer bar IS
            // `length` steps of 16ths, so a signature sets length = beats * 4.
            // Capped at 4/4: MAX_STEPS = 16 (drum.rs). "Free" = drag the Steps
            // DragValue to any 1..=16 — no button matches then (length !=
            // beats_per_bar * 4).
            const SIGS: [(u8, &str); 3] = [(2, "2/4"), (3, "3/4"), (4, "4/4")];
            let active_sig = {
                let p = &self.drum_patterns[pat_idx];
                if p.length == p.beats_per_bar.saturating_mul(4) {
                    Some(p.beats_per_bar)
                } else {
                    None
                }
            };
            for (bpb, label) in SIGS {
                let selected = active_sig == Some(bpb);
                if ui.selectable_label(selected, label)
                    .on_hover_text(format!(
                        "{label}: {} steps ({} beats of 16ths)", bpb * 4, bpb
                    ))
                    .clicked() && !selected
                {
                    let pat = &mut self.drum_patterns[pat_idx];
                    pat.beats_per_bar = bpb;
                    pat.length = bpb * 4;
                    let _ = self.ctrl_tx.push(ControlEvent::DrumSeqTimeSig { beats_per_bar: bpb });
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

        ui.add_space(theme::SP_SM);

        // Step grid
        let cell_w = 28.0;
        let cell_h = 22.0;
        let label_width = 80.0;
        let pattern_length = self.drum_patterns[pat_idx].length as usize;

        // Colors per instrument type
        let colors = theme::DRUM_COLORS;

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
                        theme::BG_BEAT_PRIMARY
                    } else if step % 2 == 0 {
                        theme::BG_BEAT_SECONDARY
                    } else {
                        theme::BG_BEAT_OFF
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

                    // Current step playhead (amber + thicker on the downbeat)
                    if is_current {
                        let downbeat = step == 0;
                        let color = if downbeat {
                            egui::Color32::from_rgb(255, 190, 60)
                        } else {
                            egui::Color32::WHITE
                        };
                        let width = if downbeat { 3.0 } else { 2.0 };
                        painter.rect_stroke(
                            rect.shrink(0.5),
                            2.0,
                            egui::Stroke::new(width, color),
                            egui::StrokeKind::Inside,
                        );
                    }

                    // Border (subtle)
                    if !is_current {
                        painter.rect_stroke(rect, 2.0, egui::Stroke::new(0.5, theme::STROKE_CELL_BORDER), egui::StrokeKind::Outside);
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
                ui.add_space(theme::SP_MD);

                // Per-instrument controls. L/T/D/P are DSP-only by design: in
                // SF2 mode sequencer triggers bypass trigger_slot entirely and
                // go straight to the sampler as MIDI note-ons (drum.rs tick →
                // synth/mod.rs drum_triggers loop), so these params have no
                // effect on SF2 kits (TECHDEBT #33). Grayed out with a tooltip
                // instead of silently doing nothing.
                let dsp_kit = !(self.sf2_drums_enabled && self.sf2_drums_soundfont.is_some());
                let dsp_hint = if dsp_kit { "" } else { " — DSP only, no effect in SF2 mode" };
                ui.horizontal(|ui| {
                    let mut level = self.drum_params[slot].level;
                    let mut tune = self.drum_params[slot].tune;
                    let mut decay = self.drum_params[slot].decay;

                    ui.style_mut().spacing.slider_width = theme::SLIDER_DRUM;
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut level, 0.0..=1.0).show_value(false).text("L")).on_hover_text(format!("Level{dsp_hint}")).changed() {
                        self.drum_params[slot].level = level;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Level(level) });
                    }
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut tune, -24.0..=24.0).show_value(false).text("T")).on_hover_text(format!("Tune (semitones){dsp_hint}")).changed() {
                        self.drum_params[slot].tune = tune;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Tune(tune) });
                    }
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut decay, 0.1..=4.0).show_value(false).text("D")).on_hover_text(format!("Decay time{dsp_hint}")).changed() {
                        self.drum_params[slot].decay = decay;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Decay(decay) });
                    }
                    let mut pan = self.drum_params[slot].pan;
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false).text("P")).on_hover_text(format!("Pan L/R{dsp_hint}")).changed() {
                        self.drum_params[slot].pan = pan;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Pan(pan) });
                    }
                });
            });
        }
    }

    /// Drums-tab-only controls (TECHDEBT #25): mix row (Swing/Vol/V127,
    /// BPM/Metro live in the docked transport row), pad-remap toggle, kit save/load,
    /// MIDI import. The grid and transport live in the docked bottom panel
    /// (draw_drum_seq_docked).
    pub(super) fn draw_drum_kit_controls(&mut self, ui: &mut egui::Ui) {
        // Mix row
        ui.horizontal(|ui| {
            // Swing + Vol
            ui.label("Swing:");
            if ui.add(egui::Slider::new(&mut self.drum_swing, 0.0..=0.66).fixed_decimals(2)).on_hover_text("Swing amount").changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqSwing { swing: self.drum_swing });
            }

            ui.separator();
            ui.label("Vol:");
            if ui.add(egui::Slider::new(&mut self.drum_volume, 0.0..=2.0).fixed_decimals(2)).on_hover_text("Master drum volume").changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSetVolume { volume: self.drum_volume });
            }

            ui.separator();
            if ui.checkbox(&mut self.drum_fixed_velocity, "V127")
                .on_hover_text("Fixed velocity: every hit triggers at max (127). For controllers without velocity.").changed()
            {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSetFixedVelocity { fixed: self.drum_fixed_velocity });
                self.save_global_to_config();
            }
        });

        // Pad remap: incoming pad note -> internal slot (modal, off the main screen)
        if ui.selectable_label(self.show_drum_remap, "Remap")
            .on_hover_text("Pad note -> drum sound mapping").clicked()
        {
            self.show_drum_remap = !self.show_drum_remap;
        }

        ui.add_space(theme::SP_SM);

        // Save / Load drum kit
        ui.horizontal(|ui| {
            ui.label("Kit:");
            ui.add(egui::TextEdit::singleline(&mut self.drum_kit_name).desired_width(120.0).hint_text("name"));
            if ui.button("Save").on_hover_text("Save drum kit").clicked() && !self.drum_kit_name.trim().is_empty() {
                let kit = DrumKit {
                    name: self.drum_kit_name.trim().to_string(),
                    patterns: self.drum_patterns.to_vec(),
                    params: self.drum_params.to_vec(),
                    bpm: self.drum_bpm,
                    swing: self.drum_swing,
                    volume: self.drum_volume,
                };
                match preset::save_drum_kit(&kit) {
                    Ok(_path) => {
                        self.show_toast("Kit saved", ToastKind::Success);
                        self.drum_kit_list = preset::list_drum_kits();
                    }
                    Err(e) => { self.show_toast(format!("Error: {e}"), ToastKind::Error); }
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
                        let pat_len = kit.patterns.len().min(8);
                        patterns[..pat_len].copy_from_slice(&kit.patterns[..pat_len]);
                        let mut params = [DrumSlotParams::default(); NUM_DRUM_SLOTS];
                        let par_len = kit.params.len().min(NUM_DRUM_SLOTS);
                        params[..par_len].copy_from_slice(&kit.params[..par_len]);
                        self.drum_patterns = patterns;
                        self.drum_params = params;
                        self.drum_bpm = kit.bpm;
                        self.drum_swing = kit.swing;
                        self.drum_volume = kit.volume;
                        self.show_toast(format!("Kit loaded: {}", kit.name), ToastKind::Success);
                        let _ = self.ctrl_tx.push(ControlEvent::DrumLoadKit {
                            patterns: Box::new(patterns),
                            params: Box::new(params),
                            bpm: kit.bpm,
                            swing: kit.swing,
                            volume: kit.volume,
                        });
                    }
                    Err(e) => { self.show_toast(format!("Error: {e}"), ToastKind::Error); }
                }
            }
        });

        // Import MIDI drums
        ui.horizontal(|ui| {
            ui.label("MIDI:");
            ui.add(egui::TextEdit::singleline(&mut self.drum_midi_import_path)
                .desired_width(240.0).hint_text("path to .mid file"));
            if ui.button("Import").on_hover_text("Import from MIDI file").clicked() && !self.drum_midi_import_path.trim().is_empty() {
                let path = std::path::PathBuf::from(self.drum_midi_import_path.trim());
                match preset::import_midi_drums(&path) {
                    Ok((patterns_vec, bpm)) => {
                        let mut patterns = [DrumPattern::default(); 8];
                        let pvec_len = patterns_vec.len().min(8);
                        patterns[..pvec_len].copy_from_slice(&patterns_vec[..pvec_len]);
                        self.drum_patterns = patterns;
                        self.drum_bpm = bpm;
                        self.show_toast(format!(
                            "Imported {} bar(s) @ {:.0} BPM",
                            patterns_vec.len().min(8), bpm,
                        ), ToastKind::Success);
                        let _ = self.ctrl_tx.push(ControlEvent::DrumLoadKit {
                            patterns: Box::new(patterns),
                            params: Box::new(self.drum_params),
                            bpm,
                            swing: self.drum_swing,
                            volume: self.drum_volume,
                        });
                    }
                    Err(e) => { self.show_toast(format!("Import error: {e}"), ToastKind::Error); }
                }
            }
        });
    }

    /// Pad-remap modals, drawn from the main update loop so pad clicks work
    /// on every tab (TECHDEBT #11). The Remap button lives on the drums panel.
    pub(super) fn draw_drum_remap_modals(&mut self, ctx: &egui::Context) {
        if self.show_drum_remap {
            let kit_names = self.drum_kit_names.clone();
            // selectable kit notes 30..=55 that have a sample
            let kit_notes: Vec<(u8, &str)> = (0..26)
                .filter(|&i| !kit_names[i].is_empty())
                .map(|i| (30 + i as u8, kit_names[i].as_str()))
                .collect();
            let name_of = |v: u8| -> String {
                if (30..=55).contains(&v) && !kit_names[(v - 30) as usize].is_empty() {
                    kit_names[(v - 30) as usize].clone()
                } else { format!("note {}", v) }
            };
            let dialog = egui::Modal::new(egui::Id::new("drum_remap_dialog")).show(ctx, |ui| {
                ui.set_width(900.0);
                ui.strong("Pad Remap");
                ui.label("Which sound each physical pad plays. Top row = pads 1-8, bottom = 9-16.");
                ui.add_space(theme::SP_MD);
                let mut new_remap = self.drum_remap;
                let mut new_vels = self.drum_pad_vel;
                let mut pad_row = |ui: &mut egui::Ui, pads: std::ops::Range<usize>, id: &str| {
                    egui::Grid::new(id).num_columns(8).spacing([8.0, 6.0]).show(ui, |ui| {
                        for p in pads {
                            let note = if p < 8 { PAD_TOP_NOTES[p] } else { PAD_BOT_NOTES[p - 8] };
                            let slot = &mut new_remap[(note - 36) as usize];
                            let sel = name_of(*slot);
                            ui.vertical_centered(|ui| {
                                ui.label(format!("{}", p + 1));
                                egui::ComboBox::from_id_salt(p)
                                    .width(120.0)
                                    .selected_text(sel)
                                    .show_ui(ui, |ui| {
                                        for (n, name) in kit_notes.iter() {
                                            ui.selectable_value(slot, *n, *name);
                                        }
                                    });
                                let mut vel = new_vels[(note - 36) as usize];
                                ui.add_sized([60.0, 0.0], egui::DragValue::new(&mut vel)
                                    .range(0..=127)
                                    .custom_formatter(|v, _| if v <= 0.0 { "Off".into() } else { format!("{}", v as i32) }));
                                new_vels[(note - 36) as usize] = vel;
                            });
                        }
                    });
                };
                pad_row(ui, 0..8, "remap_top");
                pad_row(ui, 8..16, "remap_bot");
                ui.add_space(theme::SP_MD);
                let mut reset_clicked = false;
                ui.horizontal(|ui| {
                    if ui.button("Copy top to bottom").on_hover_text("Copy pads 1-8 mapping to pads 9-16").clicked() {
                        for i in 0..8 { new_remap[(PAD_BOT_NOTES[i] - 36) as usize] = new_remap[(PAD_TOP_NOTES[i] - 36) as usize]; }
                    }
                    if ui.button("Copy bottom to top").on_hover_text("Copy pads 9-16 mapping to pads 1-8").clicked() {
                        for i in 0..8 { new_remap[(PAD_TOP_NOTES[i] - 36) as usize] = new_remap[(PAD_BOT_NOTES[i] - 36) as usize]; }
                    }
                    if ui.button("Reset").clicked() {
                        reset_clicked = true;
                    }
                    if ui.button("Close").clicked() {
                        self.show_drum_remap = false;
                    }
                });
                if reset_clicked {
                    for (i, v) in new_remap.iter_mut().enumerate() { *v = 36 + i as u8; }
                    new_vels = [0u8; 16];
                    self.drum_remap = new_remap;
                    self.drum_pad_vel = new_vels;
                    self.push_drum_remap();
                    self.push_drum_pad_vel();
                    self.save_drum_remap();
                    self.save_drum_pad_vel();
                }
                if new_remap != self.drum_remap {
                    self.drum_remap = new_remap;
                    self.push_drum_remap();
                    self.save_drum_remap();
                }
                if new_vels != self.drum_pad_vel {
                    self.drum_pad_vel = new_vels;
                    self.push_drum_pad_vel();
                    self.save_drum_pad_vel();
                }
            });
            if dialog.should_close() {
                self.show_drum_remap = false;
            }
        }

        // Single-pad remap: click a drawn pad on the main screen
        if let Some(p) = self.remap_pad {
            // hardware pad p sends this factory note
            let note = if p < 8 { PAD_TOP_NOTES[p] } else { PAD_BOT_NOTES[p - 8] };
            let idx = (note - 36) as usize;
            let mut sel = self.drum_remap[idx];
            let kit_names = self.drum_kit_names.clone();
            let kit_notes: Vec<(u8, &str)> = (0..26)
                .filter(|&i| !kit_names[i].is_empty())
                .map(|i| (30 + i as u8, kit_names[i].as_str()))
                .collect();
            let sel_name = if (30..=55).contains(&sel) && !kit_names[(sel - 30) as usize].is_empty() {
                kit_names[(sel - 30) as usize].clone()
            } else { format!("note {}", sel) };
            let dialog = egui::Modal::new(egui::Id::new("remap_pad_dialog")).show(ctx, |ui| {
                ui.set_width(300.0);
                ui.strong(format!("Pad {}", p + 1));
                ui.label("Instrument:");
                ui.add_space(theme::SP_MD);
                egui::ComboBox::from_id_salt("remap_instrument")
                    .width(200.0)
                    .selected_text(sel_name)
                    .show_ui(ui, |ui| {
                        for (n, name) in kit_notes.iter() {
                            ui.selectable_value(&mut sel, *n, *name);
                        }
                    });
                ui.add_space(theme::SP_MD);
                ui.label("Velocity (Off = dynamic):");
                let mut vel = self.drum_pad_vel[idx];
                ui.add_sized([80.0, 0.0], egui::DragValue::new(&mut vel)
                    .range(0..=127)
                    .custom_formatter(|v, _| if v <= 0.0 { "Off".into() } else { format!("{}", v as i32) }));
                if vel != self.drum_pad_vel[idx] {
                    self.drum_pad_vel[idx] = vel;
                    self.push_drum_pad_vel();
                    self.save_drum_pad_vel();
                }
                ui.add_space(theme::SP_MD);
                if ui.button("Close").clicked() {
                    self.remap_pad = None;
                }
            });
            if sel != self.drum_remap[idx] {
                // detach any other pad mapped to this instrument, then attach this pad
                for (i, v) in self.drum_remap.iter_mut().enumerate() {
                    if *v == sel && i != idx { *v = (36 + i) as u8; }
                }
                self.drum_remap[idx] = sel;
                self.push_drum_remap();
                self.save_drum_remap();
            }
            if dialog.should_close() {
                self.remap_pad = None;
            }
        }
    }

    pub(super) fn draw_pitch_sequencer(&mut self, ui: &mut egui::Ui) {
        use crate::synth::step_seq::{StepRate, ScaleType};
        let part = self.active_part;
        if part >= 2 { return; }

        let enabled = self.pitch_seq_enabled[part];

        egui::CollapsingHeader::new("Step Sequencer")
            .default_open(enabled)
            .show(ui, |ui| {
                // Controls row
                ui.horizontal(|ui| {
                    let mut en = enabled;
                    if ui.checkbox(&mut en, "Enable").changed() {
                        self.pitch_seq_enabled[part] = en;
                        let _ = self.ctrl_tx.push(ControlEvent::SeqSetEnabled { part, enabled: en });
                    }

                    ui.separator();

                    // Rate
                    let rate = StepRate::from_index(self.pitch_seq_rate[part]);
                    let rate_name = rate.name();
                    egui::ComboBox::from_id_salt(format!("seq_rate_{part}"))
                        .selected_text(rate_name)
                        .width(50.0)
                        .show_ui(ui, |ui| {
                            for i in 0..6u8 {
                                let r = StepRate::from_index(i);
                                if ui.selectable_label(i == self.pitch_seq_rate[part], r.name()).clicked() {
                                    self.pitch_seq_rate[part] = i;
                                    let _ = self.ctrl_tx.push(ControlEvent::SeqSetRate { part, rate: i });
                                }
                            }
                        });

                    // Scale
                    let scale = ScaleType::from_index(self.pitch_seq_scale[part]);
                    egui::ComboBox::from_id_salt(format!("seq_scale_{part}"))
                        .selected_text(scale.name())
                        .width(80.0)
                        .show_ui(ui, |ui| {
                            for i in 0..7u8 {
                                let s = ScaleType::from_index(i);
                                if ui.selectable_label(i == self.pitch_seq_scale[part], s.name()).clicked() {
                                    self.pitch_seq_scale[part] = i;
                                    let _ = self.ctrl_tx.push(ControlEvent::SeqSetScale { part, scale: i });
                                }
                            }
                        });

                    // Length
                    ui.label("Len:");
                    let mut len = self.pitch_seq_length[part] as i32;
                    let len_resp = ui.add(egui::DragValue::new(&mut len).range(1..=16).speed(0.1));
                    if len_resp.changed() {
                        self.pitch_seq_length[part] = len as u8;
                        let _ = self.ctrl_tx.push(ControlEvent::SeqSetLength { part, length: len as u8 });
                    }

                    // Swing
                    ui.label("Swing:");
                    let mut sw = self.pitch_seq_swing[part];
                    if ui.add(egui::DragValue::new(&mut sw).range(0.0..=0.66).speed(0.005).fixed_decimals(2)).changed() {
                        self.pitch_seq_swing[part] = sw;
                        let _ = self.ctrl_tx.push(ControlEvent::SeqSetSwing { part, swing: sw });
                    }
                });

                if !enabled { return; }

                // Current step from audio thread
                let current_step = if part < self.pitch_seq_step_atoms.len() {
                    self.pitch_seq_step_atoms[part].load(std::sync::atomic::Ordering::Relaxed)
                } else { 0 };

                let length = self.pitch_seq_length[part] as usize;

                // Step grid: pitch bars + gate toggles
                let avail_w = ui.available_width();
                let step_w = (avail_w / length as f32).clamp(20.0, 40.0);
                let bar_h = 80.0;

                // Pitch bars
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(step_w * length as f32, bar_h + 20.0),
                    egui::Sense::click_and_drag(),
                );

                let painter = ui.painter_at(rect);

                // Background
                painter.rect_filled(rect, 2.0, theme::BG_PANEL);

                // Center line (pitch = 0)
                let center_y = rect.top() + bar_h * 0.5;
                painter.line_segment(
                    [egui::pos2(rect.left(), center_y), egui::pos2(rect.left() + step_w * length as f32, center_y)],
                    egui::Stroke::new(1.0, theme::STROKE_GUIDE),
                );

                for i in 0..length {
                    let x = rect.left() + i as f32 * step_w;
                    let step = &self.pitch_seq_steps[part][i];
                    let is_current = i == current_step as usize && enabled;

                    // Step separator
                    if i > 0 {
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.top() + bar_h)],
                            egui::Stroke::new(0.5, theme::STROKE_GRID),
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
                        theme::SEQ_BAR_MUTED
                    } else if is_current {
                        theme::SEQ_BAR_CURRENT
                    } else {
                        theme::SEQ_BAR_IDLE
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
                            theme::TEXT_OVERLAY,
                        );
                    }

                    // Current step highlight
                    if is_current {
                        painter.rect_stroke(
                            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(step_w, bar_h)),
                            0.0,
                            egui::Stroke::new(2.0, theme::SEQ_STEP_HIGHLIGHT),
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
                        theme::SEQ_GATE_ON
                    } else {
                        theme::SEQ_GATE_OFF
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
                                self.pitch_seq_steps[part][step_idx].pitch = pitch;
                                let step = &self.pitch_seq_steps[part][step_idx];
                                let _ = self.ctrl_tx.push(ControlEvent::SeqSetStep {
                                    part, step: step_idx as u8,
                                    pitch, gate: step.gate, velocity: step.velocity,
                                });
                            } else if response.clicked() {
                                // Clicking gate area
                                let gate = !self.pitch_seq_steps[part][step_idx].gate;
                                self.pitch_seq_steps[part][step_idx].gate = gate;
                                let step = &self.pitch_seq_steps[part][step_idx];
                                let _ = self.ctrl_tx.push(ControlEvent::SeqSetStep {
                                    part, step: step_idx as u8,
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

    pub(super) fn draw_looper(&mut self, ui: &mut egui::Ui) {
        use crate::synth::looper::LooperState;

        let state_u8 = self.looper_atoms.state.load(Ordering::Relaxed);
        let position = self.looper_atoms.position.load(Ordering::Relaxed);
        let event_count = self.looper_atoms.event_count.load(Ordering::Relaxed);
        let layer_count = self.looper_atoms.layer_count.load(Ordering::Relaxed);
        let state = LooperState::from_u8(state_u8);

        let part_colors = theme::PART_COLORS;

        // --- Row 1: Transport + Status ---
        ui.horizontal(|ui| {
            let clear_pending = self.looper_clear_confirm
                .map(|t| t.elapsed().as_secs_f32() < 2.0).unwrap_or(false);
            let clear_label = if clear_pending { "Sure?" } else { "Clear" };
            let clear_color = if clear_pending { egui::Color32::RED } else { ui.visuals().text_color() };
            if ui.add_enabled(
                event_count > 0 || state != LooperState::Idle,
                egui::Button::new(egui::RichText::new(clear_label).color(clear_color)),
            ).clicked() {
                if clear_pending {
                    let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
                    self.looper_clear_confirm = None;
                } else {
                    self.looper_clear_confirm = Some(std::time::Instant::now());
                }
            }
            if self.looper_clear_confirm.map(|t| t.elapsed().as_secs_f32() >= 2.0).unwrap_or(false) {
                self.looper_clear_confirm = None;
            }

            ui.separator();

            let status_color = match state {
                LooperState::Recording => egui::Color32::RED,
                LooperState::Overdubbing => theme::LOOPER_OVERDUB,
                LooperState::Playing => egui::Color32::GREEN,
                LooperState::Idle => egui::Color32::GRAY,
            };
            let state_label = match state {
                LooperState::Idle => "Idle",
                LooperState::Recording => "REC",
                LooperState::Playing => "Play",
                LooperState::Overdubbing => "OVR",
            };
            // Quantized master capture (#30): индикация, на каком цикле стоим
            let rec_state = self.master_rec_state.load(std::sync::atomic::Ordering::Relaxed);
            let rec_cycle = self.master_rec_cycle.load(std::sync::atomic::Ordering::Relaxed);
            let (label, label_color) = match rec_state {
                1 => ("ARMED".to_string(), egui::Color32::YELLOW),
                2 => (format!("REC {}/{}", rec_cycle, self.looper_bars), egui::Color32::RED),
                3 => (format!("END {}/{}", rec_cycle, self.looper_bars), egui::Color32::ORANGE),
                _ => (state_label.to_string(), status_color),
            };
            ui.colored_label(label_color, label);
            if event_count > 0 {
                ui.label(egui::RichText::new(format!("{event_count}ev L{layer_count}")).small().weak());
            }
            if state != LooperState::Idle {
                let frac = position as f32 / 255.0;
                let bar_frac = if rec_state >= 2 && self.looper_bars > 0 {
                    // Квантованный захват: прогресс ВНУТРИ текущего цикла
                    (frac * self.looper_bars as f32) % 1.0
                } else {
                    frac
                };
                ui.add(egui::ProgressBar::new(bar_frac).desired_width(80.0))
                    .on_hover_text(if rec_state >= 2 {
                        format!("Цикл {} ({} шагов в петле)", rec_cycle, loop_steps)
                    } else {
                        "Loop position".to_string()
                    });
            }
            // ＋такт / −такт — длина петли прямо во время записи
            let capturing = rec_state == 2;
            let loop_steps = self.looper_atoms.loop_steps.load(std::sync::atomic::Ordering::Relaxed);
            if ui.add_enabled(capturing, egui::Button::new("+1 такт"))
                .on_hover_text("Добавить такт к петле (запись продолжается)").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::CaptureCycleAdd);
            }
            if ui.add_enabled(capturing, egui::Button::new("−1 такт"))
                .on_hover_text("Убрать последний такт (курсор записи откатывается — переиграй его)").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::CaptureCycleRemove);
            }
        });

        // --- Row 2: Bars + Quantize + BPM + Recording target ---
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Bars").weak().small())
                .on_hover_text("Циклов паттерна в петле: запись сама закроется после них; досрочный Rec = сколько сыграл");
            for &b in &[1u8, 2, 4, 8] {
                if ui.selectable_label(self.looper_bars == b, format!("{b}")).clicked() {
                    self.looper_bars = b;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetBars { bars: b });
                }
            }
            // Честная длина петли: после квантованного захвата = цикл паттерна
            let loop_steps = self.looper_atoms.loop_steps.load(std::sync::atomic::Ordering::Relaxed);
            if loop_steps > 0 {
                ui.label(egui::RichText::new(format!("Loop: {loop_steps} st")).small().weak());
            }
            ui.separator();

            ui.label(egui::RichText::new("Q").weak().small())
                .on_hover_text("Квантование нот по сетке при записи (Boss Loop Quantize)");
            for (idx, &label) in ["Off", "1/4", "1/8", "1/16"].iter().enumerate() {
                let idx = idx as u8;
                if ui.selectable_label(self.looper_quantize == idx, label).clicked() {
                    self.looper_quantize = idx;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetQuantize { quantize: idx });
                }
            }
            ui.separator();

            // Source toggle (TECHDEBT #33/#2 partial): Mix records every
            // sounding enabled part (default); Part records only the active
            // part. Live transport state — not persisted (like seq_target).
            let ap = self.active_part;
            let zone = if ap < super::PARTS_PER_ZONE { "A" } else { "B" };
            let num = (ap % super::PARTS_PER_ZONE) + 1;
            let part_name = self.patches.get(self.parts[ap].patch_idx)
                .map(|p| p.name.as_str()).unwrap_or("---");
            let is_recording = matches!(state, LooperState::Recording | LooperState::Overdubbing);
            ui.label(egui::RichText::new("Source").weak().small());
            let mix_color = if self.looper_source_mix { egui::Color32::GREEN } else { egui::Color32::GRAY };
            if ui.selectable_label(self.looper_source_mix, egui::RichText::new("Mix").color(mix_color))
                .on_hover_text("Record every sounding enabled part")
                .clicked()
                && !self.looper_source_mix
            {
                self.looper_source_mix = true;
                let _ = self.ctrl_tx.push(ControlEvent::LooperSetSource { mix: true });
            }
            if self.looper_source_mix {
                // Collapsed: a single "Part" switch to leave Mix mode.
                if ui.selectable_label(false, "Part")
                    .on_hover_text("Record only the active part")
                    .clicked()
                {
                    self.looper_source_mix = false;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetSource { mix: false });
                }
            } else {
                // Part source: records the active part (red "Rec" prefix while
                // armed); combobox switches which part is active.
                let rec_indicator = if is_recording { "Rec" } else { "Part" };
                let rec_text_color = if is_recording { egui::Color32::RED } else { part_colors[ap % 8] };
                let selected = format!("{rec_indicator}: {zone}{num} {part_name}");
                egui::ComboBox::from_id_salt("looper_rec_part")
                    .selected_text(egui::RichText::new(&selected).color(rec_text_color))
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        for i in 0..self.parts.len() {
                            if !self.parts[i].enabled { continue; }
                            let z = if i < super::PARTS_PER_ZONE { "A" } else { "B" };
                            let n = (i % super::PARTS_PER_ZONE) + 1;
                            let pname = self.patches.get(self.parts[i].patch_idx)
                                .map(|p| p.name.as_str()).unwrap_or("---");
                            let label = format!("{z}{n}: {pname}");
                            if ui.selectable_label(i == ap, egui::RichText::new(label).color(part_colors[i % 8])).clicked() {
                                self.set_active_part(i);
                            }
                        }
                    });
            }
        });

        // --- Part mutes: filter looper replay per part (TECHDEBT #30/#34),
        // same "M" button pattern as the layer mutes below ---
        let any_part_enabled = self.parts.iter().any(|p| p.enabled);
        if any_part_enabled {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Mute parts").weak().small());
                for i in 0..self.parts.len().min(self.looper_part_mutes.len()) {
                    if !self.parts[i].enabled { continue; }
                    let muted = self.looper_part_mutes[i];
                    let color = if muted { egui::Color32::RED } else { part_colors[i % 8] };
                    let z = if i < super::PARTS_PER_ZONE { "A" } else { "B" };
                    let n = (i % super::PARTS_PER_ZONE) + 1;
                    if ui.button(egui::RichText::new(format!("{z}{n}")).small().color(color))
                        .on_hover_text(if muted {
                            "Unmute part in looper replay"
                        } else {
                            "Mute part in looper replay"
                        })
                        .clicked()
                    {
                        self.looper_part_mutes[i] = !muted;
                        let _ = self.ctrl_tx.push(ControlEvent::LooperSetPartMute { part: i as u8, mute: !muted });
                    }
                }
            });
        }

        // --- Timeline ---
        if event_count > 0 {
            self.draw_looper_timeline(ui, state, position, &part_colors);
        }

        // --- Layer list with Mute/Solo ---
        if event_count > 0 {
            self.draw_looper_layers(ui, layer_count, &part_colors);
        }
    }

    fn draw_looper_layers(&mut self, ui: &mut egui::Ui, layer_count: u8, part_colors: &[egui::Color32; 8]) {
        // Collect layer→part info from display snapshot
        let layer_info: Vec<(u8, Vec<u8>)> = {
            let display = match self.looper_display.try_lock() {
                Ok(d) => d,
                Err(_) => return,
            };
            let mut info: Vec<(u8, Vec<u8>)> = Vec::new();
            for ev in &display.events {
                if ev.velocity == 0 { continue; }
                if let Some((_, parts)) = info.iter_mut().find(|(l, _)| *l == ev.layer_id) {
                    if !parts.contains(&ev.part_id) { parts.push(ev.part_id); }
                } else {
                    info.push((ev.layer_id, vec![ev.part_id]));
                }
            }
            info.sort_by_key(|(l, _)| *l);
            info
        };

        if layer_info.is_empty() { return; }

        for (layer_id, part_ids) in &layer_info {
            ui.horizontal(|ui| {
                let l = layer_id + 1;
                let muted = self.looper_layer_mute[*layer_id as usize];
                let soloed = self.looper_solo_layer == Some(*layer_id);

                // Layer label with part color
                let primary_part = part_ids[0] as usize % 8;
                let color = if muted {
                    theme::LOOPER_LAYER_MUTED
                } else {
                    part_colors[primary_part]
                };

                // Part/preset names for this layer — компактно: имя патча
                // режется до 12 символов, полный список в hover (узкая колонка
                // docked-юнита: длинные подписи переносились и наезжали на S/M).
                // pid 254 = тег драмы (bank-1 удары) — не часть, лейбл "Drums".
                let part_label = |pid: u8, full_name: bool| -> String {
                    if pid == 254 { return "Drums".to_string(); }
                    let z = if (pid as usize) < super::PARTS_PER_ZONE { "A" } else { "B" };
                    let n = (pid as usize % super::PARTS_PER_ZONE) + 1;
                    let pname = self.patches.get(self.parts[pid as usize].patch_idx)
                        .map(|p| p.name.as_str()).unwrap_or("---");
                    if full_name {
                        format!("{z}{n}:{pname}")
                    } else {
                        let cut: String = pname.chars().take(12).collect();
                        let dots = if pname.chars().count() > 12 { "…" } else { "" };
                        format!("{z}{n}:{cut}{dots}")
                    }
                };
                let full = part_ids.iter().map(|&pid| part_label(pid, true))
                    .collect::<Vec<_>>().join(" ");
                let short = part_ids.iter().map(|&pid| part_label(pid, false))
                    .collect::<Vec<_>>().join(" ");

                ui.colored_label(color, egui::RichText::new(format!("L{l}")).strong());
                ui.add(egui::Label::new(egui::RichText::new(short).small()).truncate())
                    .on_hover_text(full);

                // Solo button
                let solo_color = if soloed { egui::Color32::YELLOW } else { egui::Color32::GRAY };
                if ui.button(egui::RichText::new("S").color(solo_color)).on_hover_text("Solo layer").clicked() {
                    if soloed {
                        self.looper_solo_layer = None;
                        let _ = self.ctrl_tx.push(ControlEvent::LooperSetSolo { layer: None });
                    } else {
                        self.looper_solo_layer = Some(*layer_id);
                        let _ = self.ctrl_tx.push(ControlEvent::LooperSetSolo { layer: Some(*layer_id) });
                    }
                }

                // Mute button
                let mute_color = if muted { egui::Color32::RED } else { egui::Color32::GRAY };
                if ui.button(egui::RichText::new("M").color(mute_color)).on_hover_text("Mute layer").clicked() {
                    let new_mute = !muted;
                    self.looper_layer_mute[*layer_id as usize] = new_mute;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetLayerMute { layer: *layer_id, mute: new_mute });
                }

                // Current layer indicator
                if *layer_id == layer_count.saturating_sub(1) {
                    ui.label(egui::RichText::new("<").small().weak());
                }
            });
        }
    }

    fn draw_looper_timeline(
        &self,
        ui: &mut egui::Ui,
        state: crate::synth::looper::LooperState,
        position: u8,
        part_colors: &[egui::Color32; 8],
    ) {
        use crate::synth::looper::LooperState;

        // Snapshot display data under lock, then release immediately
        let (events, current_layer) = {
            let display = match self.looper_display.try_lock() {
                Ok(d) => d,
                Err(_) => return,
            };
            if display.events.is_empty() { return; }
            (display.events.clone(), display.current_layer)
        };

        let total_w = ui.available_width().min(600.0);
        let height = 40.0;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(total_w, height), egui::Sense::hover());
        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 2.0, theme::BG_TIMELINE);

        // Beat grid lines — по РЕАЛЬНОЙ длине петли (loop_steps из движка):
        // после квантованного захвата длина = цикл паттерна, Bars ≠ длина.
        let loop_steps = self.looper_atoms.loop_steps.load(std::sync::atomic::Ordering::Relaxed);
        let loop_steps = if loop_steps == 0 { self.looper_bars as u32 * 16 } else { loop_steps as u32 };
        let beats = loop_steps / 4;
        for beat in 1..beats {
            let x = rect.left() + (beat as f32 / beats as f32) * total_w;
            let color = if beat % 4 == 0 {
                theme::STROKE_BAR_LINE
            } else {
                theme::STROKE_GRID_DARK
            };
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, color),
            );
        }

        // Find note range for vertical mapping
        let (mut min_note, mut max_note) = (127u8, 0u8);
        for ev in &events {
            if ev.velocity > 0 {
                min_note = min_note.min(ev.note);
                max_note = max_note.max(ev.note);
            }
        }
        if min_note > max_note { return; }
        // Add padding
        let range = (max_note - min_note).max(12) as f32;
        let mid = (min_note + max_note) as f32 / 2.0;
        let note_lo = (mid - range / 2.0 - 2.0).max(0.0);
        let note_hi = (mid + range / 2.0 + 2.0).min(127.0);
        let note_span = (note_hi - note_lo).max(1.0);

        // Draw note-on events as small rectangles
        let dot_w = (total_w / loop_steps as f32).clamp(2.0, 6.0);
        let dot_h = (height / (note_span / 2.0)).clamp(2.0, 5.0);
        for ev in &events {
            if ev.velocity == 0 { continue; } // skip note-offs
            let x = rect.left() + ev.position * total_w;
            let y = rect.bottom() - ((ev.note as f32 - note_lo) / note_span) * height;
            let color = part_colors[ev.part_id as usize % 8];
            // Dim muted/non-solo layers, muted parts, older layers slightly dimmer
            let layer_muted = self.looper_layer_mute[ev.layer_id as usize];
            let part_muted = self.looper_part_mutes.get(ev.part_id as usize).copied().unwrap_or(false);
            let layer_hidden = if let Some(solo) = self.looper_solo_layer {
                ev.layer_id != solo
            } else { layer_muted || part_muted };
            let alpha = if layer_hidden { 40 }
            else if ev.layer_id == current_layer { 255 }
            else { 180u8.saturating_sub(current_layer.saturating_sub(ev.layer_id) * 30) };
            let c = egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
            painter.rect_filled(
                egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(dot_w, dot_h)),
                1.0, c,
            );
        }

        // Playhead
        if state != LooperState::Idle {
            let px = rect.left() + (position as f32 / 255.0) * total_w;
            painter.line_segment(
                [egui::pos2(px, rect.top()), egui::pos2(px, rect.bottom())],
                egui::Stroke::new(1.5, egui::Color32::WHITE),
            );
        }

    }
}
