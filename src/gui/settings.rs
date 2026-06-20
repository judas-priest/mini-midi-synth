//! Settings, help, feedback, and utility UI methods.

use eframe::egui;

use crate::preset::{self, Patch};
use crate::synth::{ControlEvent, ParamFeedback};
use super::{App, BUFFER_SIZES, buffer_label, scan_sf2_files, sf2_dir};

impl App {
    pub(super) fn draw_pad_perf_window(&mut self, ctx: &egui::Context) {
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

    pub(super) fn draw_help(&mut self, ctx: &egui::Context) {
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
                ui.strong("Preset (reset on patch change, pickup mode)");
                egui::Grid::new("cc_help_preset")
                    .num_columns(3)
                    .spacing([16.0, 2.0])
                    .striped(true)
                    .show(ui, |ui| {
                        ui.strong("Control"); ui.strong("CC"); ui.strong("Parameter"); ui.end_row();
                        for (cc, binding) in self.cc_map.bindings.iter().enumerate().filter_map(|(i, b)| b.map(|b| (i as u8, b))) {
                            if binding.scope == crate::cc_map::ParamScope::Patch {
                                let ctrl = crate::cc_map::cc_to_control_name(cc);
                                let label = crate::cc_map::find_param_meta(binding.param_key)
                                    .map(|m| m.label).unwrap_or("?");
                                ui.label(format!("{ctrl} (CC{cc})")); ui.label(""); ui.label(label); ui.end_row();
                            }
                        }
                    });

                ui.add_space(8.0);

                // Global bindings
                ui.strong("Global (persist across patch changes)");
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

    pub(super) fn drain_feedback(&mut self) {
        let mut param_changes: Vec<(&str, f32)> = Vec::new();
        let mut learned_cc: Option<(u8, &'static str)> = None;
        let mut program: Option<u8> = None;
        let nav_delta: Option<i32> = None;

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
                // Global params (volume, tone) — don't put in patch
                self.global_params.insert(key.to_string(), value);
                self.global_dirty = true;
            } else {
                // Patch params — update all parts
                for layer_state in &mut self.parts {
                    layer_state.edited_params.insert(key.to_string(), value);
                    layer_state.params_dirty = true;
                }
            }
        }

        if let Some((cc, target)) = learned_cc {
            if let Some(meta) = crate::cc_map::find_param_meta(target) {
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
            if self.patches.get(prog as usize).is_some() {
                self.parts[0].patch_idx = prog as usize;
                self.load_edited_params(0);
            }
        }

        let _ = nav_delta; // no longer used (setlist removed)
    }

    pub(super) fn save_cc_map(&mut self) {
        self.config.cc_map = Some(crate::cc_map::CcMapRaw::from_cc_map(&self.cc_map));
        let _ = self.config.save();
        let _ = self.ctrl_tx.push(ControlEvent::SetCcMap { map: Box::new(self.cc_map) });
    }

    pub(super) fn param_slider(&mut self, ui: &mut egui::Ui, key: &str, label: &str, min: f32, max: f32, logarithmic: bool) -> bool {
        let part = self.active_part;
        let mut val = self.parts[part].edited_params.get(key).copied().unwrap_or(min);

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
        resp.context_menu(|ui| {
            // Show current CC assignment
            if let Some(cc) = cc_info {
                ui.label(format!("Assigned: CC {cc}"));
                ui.separator();
            }
            if ui.button("MIDI Learn").clicked() {
                if let Some(static_key) = crate::cc_map::resolve_key(key) {
                    self.midi_learn_target = Some(static_key);
                }
                ui.close_menu();
            }
            if let Some(cc) = cc_info {
                if ui.button("Clear MIDI").clicked() {
                    self.cc_map.bindings[cc] = None;
                    self.save_cc_map();
                    ui.close_menu();
                }
            }
        });
        if resp.changed() {
            self.parts[part].edited_params.insert(key.into(), val);
            return true;
        }
        false
    }

    pub(super) fn save_as_user_preset(&mut self) {
        let part = self.active_part;
        let base_name = self.patches.get(self.parts[part].patch_idx)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Custom".to_string());

        let new_name = format!("{base_name} (user)");
        let new_patch = Patch {
            name: new_name.clone(),
            category: "User".to_string(),
            params: self.parts[part].edited_params.clone(),
            wavetable_file: None,
            wavetable_data: None,
            wavetable_frames: 0,
            wavetable_frame_size: 0,
        };

        if let Ok(path) = preset::save_patch(&new_patch) {
            self.settings_status = format!("Saved: {}", path.display());
            self.patches.push(new_patch);
            self.parts[part].patch_idx = self.patches.len() - 1;
            self.parts[part].params_dirty = false;
            self.save_config();
        }
    }

    pub(super) fn draw_settings(&mut self, ctx: &egui::Context) {
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
                    ui.horizontal(|ui| {
                        ui.label("Sample offset:");
                        let old = self.sf2_sample_offset_ms;
                        ui.add(egui::Slider::new(&mut self.sf2_sample_offset_ms, 0.0..=50.0)
                            .suffix(" ms")
                            .fixed_decimals(1));
                        if self.sf2_sample_offset_ms != old {
                            let _ = self.ctrl_tx.push(ControlEvent::SetSf2SampleOffset { ms: self.sf2_sample_offset_ms });
                            self.config.sf2.sample_offset_ms = self.sf2_sample_offset_ms;
                            self.global_dirty = true;
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

    pub(super) fn apply_midi(&mut self) {
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
}
