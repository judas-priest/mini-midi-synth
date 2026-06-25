//! Drum sequencer, pitch sequencer, and looper UI.

use std::sync::atomic::Ordering;
use eframe::egui;

use crate::preset::{self, DrumKit};
use crate::synth::{ControlEvent, DrumParam};
use crate::synth::drum::{NUM_DRUM_SLOTS, DRUM_NAMES, DrumSlotParams, DrumPattern};
use super::App;

impl App {
    pub(super) fn draw_drum_sequencer(&mut self, ui: &mut egui::Ui) {
        let current_step = self.drum_step_atom.load(Ordering::Relaxed);
        let pat_idx = self.drum_current_pattern as usize;



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
            if ui.button("Undo").on_hover_text("Undo last edit").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqUndo);
            }
            if ui.button("Clear").on_hover_text("Clear all steps").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqClear);
            }

            ui.separator();

            // BPM
            ui.label("BPM:");
            if ui.add(egui::DragValue::new(&mut self.drum_bpm).range(40.0..=300.0).speed(0.5)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqBpm { bpm: self.drum_bpm });
                if self.looper_sync_bpm {
                    self.looper_bpm = self.drum_bpm;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetBpm { bpm: self.drum_bpm });
                }
            }

            ui.separator();

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
                        self.drum_kit_status = format!("Loaded: {}", kit.name);
                        let _ = self.ctrl_tx.push(ControlEvent::DrumLoadKit {
                            patterns: Box::new(patterns),
                            params: Box::new(params),
                            bpm: kit.bpm,
                            swing: kit.swing,
                            volume: kit.volume,
                        });
                        if self.looper_sync_bpm {
                            self.looper_bpm = kit.bpm;
                            let _ = self.ctrl_tx.push(ControlEvent::LooperSetBpm { bpm: kit.bpm });
                        }
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
            if ui.button("Import").on_hover_text("Import from MIDI file").clicked() && !self.drum_midi_import_path.trim().is_empty() {
                let path = std::path::PathBuf::from(self.drum_midi_import_path.trim());
                match preset::import_midi_drums(&path) {
                    Ok((patterns_vec, bpm)) => {
                        let mut patterns = [DrumPattern::default(); 8];
                        let pvec_len = patterns_vec.len().min(8);
                        patterns[..pvec_len].copy_from_slice(&patterns_vec[..pvec_len]);
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
                        if self.looper_sync_bpm {
                            self.looper_bpm = bpm;
                            let _ = self.ctrl_tx.push(ControlEvent::LooperSetBpm { bpm });
                        }
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
                    if ui.add(egui::Slider::new(&mut level, 0.0..=1.0).show_value(false).text("L")).on_hover_text("Level").changed() {
                        self.drum_params[slot].level = level;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Level(level) });
                    }
                    if ui.add(egui::Slider::new(&mut tune, -24.0..=24.0).show_value(false).text("T")).on_hover_text("Tune (semitones)").changed() {
                        self.drum_params[slot].tune = tune;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Tune(tune) });
                    }
                    if ui.add(egui::Slider::new(&mut decay, 0.1..=4.0).show_value(false).text("D")).on_hover_text("Decay time").changed() {
                        self.drum_params[slot].decay = decay;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Decay(decay) });
                    }
                    let mut pan = self.drum_params[slot].pan;
                    if ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false).text("P")).on_hover_text("Pan L/R").changed() {
                        self.drum_params[slot].pan = pan;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetParam { slot: slot as u8, param: DrumParam::Pan(pan) });
                    }
                });
            });
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
                painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));

                // Center line (pitch = 0)
                let center_y = rect.top() + bar_h * 0.5;
                painter.line_segment(
                    [egui::pos2(rect.left(), center_y), egui::pos2(rect.left() + step_w * length as f32, center_y)],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
                );

                for i in 0..length {
                    let x = rect.left() + i as f32 * step_w;
                    let step = &self.pitch_seq_steps[part][i];
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

        let part_colors = [
            egui::Color32::from_rgb(70, 130, 200),
            egui::Color32::from_rgb(70, 190, 100),
            egui::Color32::from_rgb(220, 150, 50),
            egui::Color32::from_rgb(200, 70, 70),
            egui::Color32::from_rgb(160, 80, 200),
            egui::Color32::from_rgb(50, 190, 190),
            egui::Color32::from_rgb(220, 200, 50),
            egui::Color32::from_rgb(190, 100, 150),
        ];

        // --- Row 1: Transport + Status ---
        ui.horizontal(|ui| {
            let play_label = match state {
                LooperState::Playing | LooperState::Overdubbing => "\u{23F9} Stop",
                _ => "\u{25B6} Play",
            };
            if ui.button(play_label).clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::LooperTogglePlay);
            }

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

            let undo_label = if layer_count > 1 { format!("Undo L{layer_count}") } else { "Undo".into() };
            if ui.add_enabled(layer_count > 1 && event_count > 0, egui::Button::new(undo_label)).clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::LooperUndo);
            }

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
            if state != LooperState::Idle {
                ui.add(egui::ProgressBar::new(position as f32 / 255.0).desired_width(80.0));
            }
        });

        // --- Row 2: Bars + Quantize + BPM + Recording target ---
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Bars").weak().small());
            for &b in &[1u8, 2, 4, 8] {
                if ui.selectable_label(self.looper_bars == b, format!("{b}")).clicked() {
                    self.looper_bars = b;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetBars { bars: b });
                }
            }
            ui.separator();

            ui.label(egui::RichText::new("Q").weak().small());
            for (idx, &label) in ["Off", "1/4", "1/8", "1/16"].iter().enumerate() {
                let idx = idx as u8;
                if ui.selectable_label(self.looper_quantize == idx, label).clicked() {
                    self.looper_quantize = idx;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetQuantize { quantize: idx });
                }
            }
            ui.separator();

            if ui.selectable_label(self.looper_sync_bpm, "Sync").on_hover_text("Sync BPM with drum sequencer").clicked() {
                self.looper_sync_bpm = !self.looper_sync_bpm;
                if self.looper_sync_bpm {
                    self.looper_bpm = self.drum_bpm;
                    let _ = self.ctrl_tx.push(ControlEvent::LooperSetBpm { bpm: self.drum_bpm });
                }
            }
            if !self.looper_sync_bpm
                && ui.add(egui::DragValue::new(&mut self.looper_bpm).range(40.0..=300.0).speed(0.5).prefix("BPM ")).changed()
            {
                let _ = self.ctrl_tx.push(ControlEvent::LooperSetBpm { bpm: self.looper_bpm });
            }

            ui.separator();

            // "Recording to" — shows active part, allows quick switch
            let ap = self.active_part;
            let zone = if ap < 4 { "A" } else { "B" };
            let num = (ap % 4) + 1;
            let part_name = self.patches.get(self.parts[ap].patch_idx)
                .map(|p| p.name.as_str()).unwrap_or("---");
            let is_recording = matches!(state, LooperState::Recording | LooperState::Overdubbing);
            let rec_indicator = if is_recording { "Rec" } else { "Part" };
            let rec_text_color = if is_recording { egui::Color32::RED } else { part_colors[ap % 8] };
            let selected = format!("{rec_indicator}: {zone}{num} {part_name}");
            egui::ComboBox::from_id_salt("looper_rec_part")
                .selected_text(egui::RichText::new(&selected).color(rec_text_color))
                .width(160.0)
                .show_ui(ui, |ui| {
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled { continue; }
                        let z = if i < 4 { "A" } else { "B" };
                        let n = (i % 4) + 1;
                        let pname = self.patches.get(self.parts[i].patch_idx)
                            .map(|p| p.name.as_str()).unwrap_or("---");
                        let label = format!("{z}{n}: {pname}");
                        if ui.selectable_label(i == ap, egui::RichText::new(label).color(part_colors[i % 8])).clicked() {
                            self.set_active_part(i);
                        }
                    }
                });
        });

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
                    egui::Color32::from_gray(80)
                } else {
                    part_colors[primary_part]
                };

                // Part/preset names for this layer
                let part_names: Vec<String> = part_ids.iter().map(|&pid| {
                    let z = if (pid as usize) < 4 { "A" } else { "B" };
                    let n = (pid as usize % 4) + 1;
                    let pname = self.patches.get(self.parts[pid as usize].patch_idx)
                        .map(|p| p.name.as_str()).unwrap_or("---");
                    format!("{z}{n}:{pname}")
                }).collect();

                ui.colored_label(color, egui::RichText::new(format!("L{l}")).strong());
                ui.colored_label(color, egui::RichText::new(part_names.join(" ")).small());

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
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(25));

        // Beat grid lines
        let beats = self.looper_bars as u32 * 4;
        for beat in 1..beats {
            let x = rect.left() + (beat as f32 / beats as f32) * total_w;
            let color = if beat % 4 == 0 {
                egui::Color32::from_gray(70)
            } else {
                egui::Color32::from_gray(40)
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
        let dot_w = (total_w / (self.looper_bars as f32 * 16.0)).clamp(2.0, 6.0);
        let dot_h = (height / (note_span / 2.0)).clamp(2.0, 5.0);
        for ev in &events {
            if ev.velocity == 0 { continue; } // skip note-offs
            let x = rect.left() + ev.position * total_w;
            let y = rect.bottom() - ((ev.note as f32 - note_lo) / note_span) * height;
            let color = part_colors[ev.part_id as usize % 8];
            // Dim muted/non-solo layers, older layers slightly dimmer
            let layer_muted = self.looper_layer_mute[ev.layer_id as usize];
            let layer_hidden = if let Some(solo) = self.looper_solo_layer {
                ev.layer_id != solo
            } else { layer_muted };
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
