//! Synth parameter editor UI.

use eframe::egui;

use super::App;
use super::theme;
use super::{
    VELOCITY_CURVE_NAMES, PORTAMENTO_MODE_NAMES, PLAY_MODE_NAMES, REVERB_TYPE_NAMES,
    RING_MOD_SHAPE_NAMES, WAVE_SHAPER_MODE_NAMES,
};

impl App {
    /// Полный редактор параметров для КОНКРЕТНОГО парта (браузер — единственный
    /// вызов; секции ниже — `draw_sec_*` методы). undo-снимки идут в `part`,
    /// не в active_part (правки из браузера другого активного парта пишутся
    /// в правильный undo).
    pub(super) fn draw_params_editable_for(&mut self, ui: &mut egui::Ui, part: usize) {
        let patch_idx = self.parts[part].patch_idx;
        let preset_name = self
            .patches
            .get(patch_idx)
            .map(|p| p.name.as_str())
            .unwrap_or("(none)");

        let layer_label = if part < super::PARTS_PER_ZONE {
            format!("A{}", part + 1)
        } else {
            format!("B{}", part - super::PARTS_PER_ZONE + 1)
        };
        ui.strong(format!("{preset_name} ({layer_label})"));
        ui.add_space(theme::SP_MD);

        let mut changed = false;

        // Macro knobs (always visible at top — key performance controls)
        self.draw_macros_for(ui, part);
        ui.add_space(theme::SP_SM);

        // Oscillator section
        changed |= self.draw_sec_osc(ui, part);
        ui.add_space(theme::SP_SM);

        // Filter section
        changed |= self.draw_sec_filter(ui, part);
        ui.add_space(theme::SP_SM);

        // Envelopes section
        changed |= self.draw_sec_env(ui, part);
        ui.add_space(theme::SP_SM);

        // Dynamics section
        changed |= self.draw_sec_dynamics(ui, part);
        ui.add_space(theme::SP_SM);

        // LFO section
        changed |= self.draw_sec_lfo(ui, part);
        ui.add_space(theme::SP_SM);

        // Mod Matrix section
        changed |= self.draw_sec_matrix(ui, part);
        ui.add_space(theme::SP_SM);

        // Voice section (Play Mode + Portamento + Unison)
        changed |= self.draw_sec_voice(ui, part);
        ui.add_space(theme::SP_SM);

        // Arpeggiator section
        changed |= self.draw_sec_arp(ui, part);
        ui.add_space(theme::SP_SM);

        // Effects section
        changed |= self.draw_sec_effects(ui, part);
        ui.add_space(theme::SP_SM);

        if changed {
            self.parts[part].params_dirty = true;
            self.send_edited_params(part);
        }

        ui.add_space(theme::SP_MD);
        ui.horizontal(|ui| {
            if self.parts[part].params_dirty {
                ui.colored_label(egui::Color32::YELLOW, "Modified");
                ui.separator();
            }
            if ui.button("Save as new preset...").clicked() {
                self.save_as_user_preset();
            }
            if self.parts[part].params_dirty && ui.button("Reset").clicked() {
                self.load_edited_params(part);
                self.send_edited_params(part);
            }
        });
    }
    /// Verbatim frame section (dynamics) extracted from draw_params_editable_for.
    fn draw_sec_dynamics(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Secondary width for dynamics, LFOs, modulators
        ui.style_mut().spacing.slider_width = theme::SLIDER_SECONDARY;

        // Dynamics
        ui.strong("Dynamics");
        ui.horizontal(|ui| {
            let mut vc = self.parts[part].edited_params.get("velocity_curve").copied().unwrap_or(0.0) as usize;
            ui.label("Vel Curve:");
            egui::ComboBox::from_id_salt(format!("vel_curve_{part}"))
                .selected_text(*VELOCITY_CURVE_NAMES.get(vc).unwrap_or(&"Linear"))
                .show_ui(ui, |ui| {
                    for (i, name) in VELOCITY_CURVE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut vc, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("velocity_curve".into(), vc as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "vel_to_filter", "Vel->Filter", 0.0, 1.0, false);
        }); // end Dynamics frame
        changed
    }
    /// Verbatim frame section (voice) extracted from draw_params_editable_for.
    fn draw_sec_voice(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Compact width for utility params
        ui.style_mut().spacing.slider_width = theme::SLIDER_COMPACT;

        // Play Mode
        ui.strong("Play Mode");
        ui.horizontal(|ui| {
            let mut pmode = self.parts[part].edited_params.get("play_mode").copied().unwrap_or(0.0) as usize;
            egui::ComboBox::from_id_salt(format!("play_mode_{part}"))
                .selected_text(*PLAY_MODE_NAMES.get(pmode).unwrap_or(&"Poly"))
                .show_ui(ui, |ui| {
                    for (i, name) in PLAY_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut pmode, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("play_mode".into(), pmode as f32);
                            changed = true;
                        }
                    }
                });
        });

        // Sustain Mode
        ui.horizontal(|ui| {
            ui.label("Sustain:");
            let mut smode = self.parts[part].edited_params.get("sustain_mode").copied().unwrap_or(0.0) as usize;
            let sustain_names = ["Hold All", "Release Others"];
            egui::ComboBox::from_id_salt(format!("sustain_mode_{part}"))
                .selected_text(*sustain_names.get(smode).unwrap_or(&"Hold All"))
                .show_ui(ui, |ui| {
                    for (i, name) in sustain_names.iter().enumerate() {
                        if ui.selectable_value(&mut smode, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("sustain_mode".into(), smode as f32);
                            changed = true;
                        }
                    }
                });
        });

        ui.add_space(theme::SP_SM);

        // Portamento
        ui.strong("Portamento");
        ui.horizontal(|ui| {
            let mut pm = self.parts[part].edited_params.get("portamento_mode").copied().unwrap_or(0.0) as usize;
            ui.label("Mode:");
            egui::ComboBox::from_id_salt(format!("porta_mode_{part}"))
                .selected_text(*PORTAMENTO_MODE_NAMES.get(pm).unwrap_or(&"Off"))
                .show_ui(ui, |ui| {
                    for (i, name) in PORTAMENTO_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut pm, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("portamento_mode".into(), pm as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "portamento_time", "Time", 0.0, 2.0, false, " s");

        ui.add_space(theme::SP_SM);

        // Unison
        ui.strong("Unison");
        {
            let mut uv = self.parts[part].edited_params.get("unison_voices").copied().unwrap_or(1.0) as u32;
            let uv_resp = ui.add(egui::Slider::new(&mut uv, 1..=8).text("Voices")).on_hover_text("Unison voice count");
            if uv_resp.drag_started() { self.push_undo_snapshot_for(part); }
            if uv_resp.changed() {
                self.parts[part].edited_params.insert("unison_voices".into(), uv as f32);
                changed = true;
            }
        }
        changed |= self.param_slider_ex(ui, "unison_detune", "Detune", 0.0, 50.0, false, " ct");
        changed |= self.param_slider(ui, "unison_spread", "Spread", 0.0, 1.0, false);
        }); // end Voice frame
        changed
    }
    /// Verbatim frame section (arp) extracted from draw_params_editable_for.
    fn draw_sec_arp(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        ui.style_mut().spacing.slider_width = theme::SLIDER_COMPACT;

        // Arpeggiator
        ui.strong("Arpeggiator");
        {
            let arp_mode_names = ["Up", "Down", "UpDown", "Random", "Order"];
            let arp_rate_names = ["1/4", "1/8", "1/16", "1/32", "1/4T", "1/8T"];

            ui.horizontal(|ui| {
                let mut enabled = self.parts[part].edited_params.get("arp_enabled").copied().unwrap_or(0.0) > 0.5;
                if ui.checkbox(&mut enabled, "Enabled").changed() {
                    self.push_undo_snapshot_for(part);
                    self.parts[part].edited_params.insert("arp_enabled".into(), if enabled { 1.0 } else { 0.0 });
                    changed = true;
                    // Send immediate arp param update
                    let mode = self.parts[part].edited_params.get("arp_mode").copied().unwrap_or(0.0) as u8;
                    let rate = self.parts[part].edited_params.get("arp_rate").copied().unwrap_or(1.0) as u8;
                    let octaves = self.parts[part].edited_params.get("arp_octaves").copied().unwrap_or(1.0) as u8;
                    let gate = self.parts[part].edited_params.get("arp_gate").copied().unwrap_or(0.5);
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::SetArpParams {
                        enabled, mode, rate, octaves, gate,
                    });
                }

                let mut latch = self.parts[part].edited_params.get("arp_latch").copied().unwrap_or(0.0) > 0.5;
                if ui.checkbox(&mut latch, "Latch").on_hover_text("Keep the arp playing after keys are released").changed() {
                    self.push_undo_snapshot_for(part);
                    self.parts[part].edited_params.insert("arp_latch".into(), if latch { 1.0 } else { 0.0 });
                    changed = true;
                    // Immediate live update (patch reload follows via send_edited_params)
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::SetArpLatch { latch });
                }

                let mut amode = self.parts[part].edited_params.get("arp_mode").copied().unwrap_or(0.0) as usize;
                ui.label("Mode:");
                egui::ComboBox::from_id_salt(format!("arp_mode_{part}"))
                    .selected_text(*arp_mode_names.get(amode).unwrap_or(&"Up"))
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in arp_mode_names.iter().enumerate() {
                            if ui.selectable_value(&mut amode, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("arp_mode".into(), amode as f32);
                                changed = true;
                            }
                        }
                    });

                let mut arate = self.parts[part].edited_params.get("arp_rate").copied().unwrap_or(1.0) as usize;
                ui.label("Rate:");
                egui::ComboBox::from_id_salt(format!("arp_rate_{part}"))
                    .selected_text(*arp_rate_names.get(arate).unwrap_or(&"1/8"))
                    .width(50.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in arp_rate_names.iter().enumerate() {
                            if ui.selectable_value(&mut arate, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("arp_rate".into(), arate as f32);
                                changed = true;
                            }
                        }
                    });
            });

            ui.horizontal(|ui| {
                let mut oct = self.parts[part].edited_params.get("arp_octaves").copied().unwrap_or(1.0) as u32;
                let oct_resp = ui.add(egui::Slider::new(&mut oct, 1..=4).text("Octaves")).on_hover_text("Arpeggiator octave range");
                if oct_resp.drag_started() { self.push_undo_snapshot_for(part); }
                if oct_resp.changed() {
                    self.parts[part].edited_params.insert("arp_octaves".into(), oct as f32);
                    changed = true;
                }
            });
            changed |= self.param_slider(ui, "arp_gate", "Gate", 0.1, 1.0, false);
        }
        }); // end Arpeggiator frame
        changed
    }
    /// Verbatim frame section (effects) extracted from draw_params_editable_for.
    fn draw_sec_effects(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Secondary width for effects
        ui.style_mut().spacing.slider_width = theme::SLIDER_SECONDARY;

        // Effects
        ui.strong("Effects");
        changed |= self.param_slider(ui, "chorus_mix", "Chorus", 0.0, 1.0, false);

        ui.add_space(theme::SP_SM);
        ui.strong("Ring Modulator");
        changed |= self.param_slider(ui, "ring_mod_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider_ex(ui, "ring_mod_freq", "Carrier Freq", 20.0, 8000.0, true, " Hz");
        ui.horizontal(|ui| {
            let mut rs = self.parts[part].edited_params.get("ring_mod_shape").copied().unwrap_or(0.0) as usize;
            ui.label("Shape:");
            egui::ComboBox::from_id_salt(format!("rm_shape_{part}"))
                .selected_text(*RING_MOD_SHAPE_NAMES.get(rs).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in RING_MOD_SHAPE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut rs, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("ring_mod_shape".into(), rs as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "ring_mod_bias", "Diode Bias", 0.0, 2.0, false);
        changed |= self.param_slider(ui, "ring_mod_linear", "Diode Linear", 0.01, 2.0, false);

        ui.add_space(theme::SP_SM);
        ui.strong("Freq Shifter");
        changed |= self.param_slider(ui, "freq_shift_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider_ex(ui, "freq_shift_hz", "Shift", -1000.0, 1000.0, false, " Hz");
        changed |= self.param_slider(ui, "freq_shift_feedback", "Feedback", 0.0, 0.9, false);
        changed |= self.param_slider_ex(ui, "freq_shift_delay", "Delay", 0.0, 1.0, false, " s");

        ui.add_space(theme::SP_SM);
        ui.strong("Tape Saturation");
        changed |= self.param_slider(ui, "tape_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_saturation", "Saturation", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_bias", "Bias", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_tone", "Tone", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_speed", "Speed", 0.0, 1.0, false);

        ui.add_space(theme::SP_SM);
        ui.strong("Neuron Distortion");
        changed |= self.param_slider(ui, "neuron_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_squash", "Squash", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_stab", "Stab", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_asym", "Asymmetry", -1.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_bias", "Bias", 0.0, 1.0, false);
        changed |= self.param_slider_ex(ui, "neuron_comb_freq", "Comb Freq", 20.0, 4000.0, true, " Hz");
        changed |= self.param_slider(ui, "neuron_comb_sep", "Comb Sep", 0.0, 1.0, false);

        ui.add_space(theme::SP_SM);
        ui.strong("Delay");
        changed |= self.param_slider(ui, "delay_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider_ex(ui, "delay_time_l", "Time L", 0.01, 2.0, false, " s");
        changed |= self.param_slider_ex(ui, "delay_time_r", "Time R", 0.01, 2.0, false, " s");
        changed |= self.param_slider(ui, "delay_feedback", "Feedback", 0.0, 0.95, false);
        changed |= self.param_slider(ui, "delay_filter", "Filter", 0.0, 0.95, false);
        {
            let mut pp = self.parts[part].edited_params.get("delay_ping_pong").copied().unwrap_or(0.0) > 0.5;
            if ui.checkbox(&mut pp, "Ping-Pong").changed() {
                self.push_undo_snapshot_for(part);
                self.parts[part].edited_params.insert("delay_ping_pong".into(), if pp { 1.0 } else { 0.0 });
                changed = true;
            }
        }

        ui.add_space(theme::SP_SM);
        ui.strong("Reverb");
        ui.horizontal(|ui| {
            let mut rt = self.parts[part].edited_params.get("reverb_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("reverb_type_{part}"))
                .selected_text(*REVERB_TYPE_NAMES.get(rt).unwrap_or(&"Plate"))
                .show_ui(ui, |ui| {
                    for (i, name) in REVERB_TYPE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut rt, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("reverb_type".into(), rt as f32);
                            changed = true;
                        }
                    }
                });
        });
        let reverb_type = self.parts[part].edited_params.get("reverb_type").copied().unwrap_or(0.0) as usize;
        if reverb_type == 0 {
            // Plate reverb params
            changed |= self.param_slider(ui, "reverb_mix", "Mix", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_room_size", "Room Size", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_damping", "Damping", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_width", "Width", 0.0, 1.0, false);
            changed |= self.param_slider_ex(ui, "reverb_pre_delay", "Pre-Delay", 0.0, 0.1, false, " s");
        } else {
            // Spring reverb params
            changed |= self.param_slider(ui, "spring_mix", "Mix", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_size", "Size", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_decay", "Decay", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_reflections", "Reflections", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_damping", "Damping", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_spin", "Spin", 0.0, 1.0, false);
        }

        ui.add_space(theme::SP_SM);
        ui.strong("Wave Shaper");
        changed |= self.param_slider(ui, "wave_shaper_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "wave_shaper_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "wave_shaper_bias", "Bias", -1.0, 1.0, false);
        ui.horizontal(|ui| {
            let mut wsm = self.parts[part].edited_params.get("wave_shaper_mode").copied().unwrap_or(0.0) as usize;
            ui.label("Mode:");
            egui::ComboBox::from_id_salt(format!("ws_mode_{part}"))
                .selected_text(*WAVE_SHAPER_MODE_NAMES.get(wsm).unwrap_or(&"Tanh"))
                .show_ui(ui, |ui| {
                    for (i, name) in WAVE_SHAPER_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut wsm, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("wave_shaper_mode".into(), wsm as f32);
                            changed = true;
                        }
                    }
                });
        });

        ui.add_space(theme::SP_SM);
        ui.strong("Airwindows");
        changed |= self.param_slider(ui, "airwindows_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "airwindows_drive", "Drive", 0.0, 1.0, false);
        ui.horizontal(|ui| {
            let airwindows_mode_names = &[
                // 0-5: original
                "Tape2", "Density", "Console", "ToVinyl4", "Atmosphere", "Pressure5",
                // 6-12: saturation/drive
                "Drive", "HardVacuum", "Spiral2", "Fracture", "Mojo", "ADClip7", "Loud",
                // 13-15: tape
                "IronOxide5", "ToTape6", "ChromeOxide",
                // 16-19: compressors
                "Pressure4", "ButterComp2", "VariMu", "PowerSag",
                // 20-21: reverbs
                "Galactic", "Verbity",
                // 22-24: filters
                "Capacitor", "Focus", "YLowpass",
                // 25-32: special
                "DubSub", "Melt", "Pop", "BitGlitter", "DeRez2", "BussColors4", "Hombre", "Slew2",
            ];
            let num_modes = airwindows_mode_names.len();
            let mut awm = self.parts[part].edited_params.get("airwindows_mode").copied().unwrap_or(0.0) as usize;
            ui.label("Mode:");
            egui::ComboBox::from_id_salt(format!("aw_mode_{part}"))
                .selected_text(*airwindows_mode_names.get(awm.min(num_modes-1)).unwrap_or(&"Tape2"))
                .show_ui(ui, |ui| {
                    for (i, name) in airwindows_mode_names.iter().enumerate() {
                        if ui.selectable_value(&mut awm, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("airwindows_mode".into(), i as f32);
                            changed = true;
                        }
                    }
                });
        });
        }); // end Effects frame
        changed
    }
}


#[cfg(test)]
mod tests {
    use super::tests_support::*;

    /// Layout-equivalence lock: the ordered sequence of `ui.strong` headers and
    /// ComboBox `id_salt` strings across the params editor, extracted from the
    /// source of the section fns in parent call order. Any reorder/renumber of
    /// UI elements fails this test (goldens are blind to params rendering).
    /// ui.label sub-headers are deliberately NOT pinned (plan: strong/id_salt only).
    #[test]
    fn params_sections_pin() {
        let found = section_markers();
        let expected: Vec<String> = [
            "strong:{preset_name} ({layer_label})",
            "strong:Oscillator 1",
            "salt:osc_type_{part}",
            "strong:Drawbars",
            "strong:Drum Synth",
            "strong:Bass Guitar",
            "salt:bass_style_{part}",
            "salt:bass_pickup_{part}",
            "strong:Bowed String",
            "salt:body_type_{part}",
            "strong:Brass",
            "salt:bell_type_{part}",
            "strong:Accordion",
            "salt:accordion_reg_{part}",
            "strong:Saxophone",
            "salt:sax_type_{part}",
            "strong:Electric Piano",
            "salt:epiano_type_{part}",
            "strong:Alias (8-bit)",
            "salt:alias_wave_{part}",
            "strong:Window Oscillator",
            "salt:window_type_{part}",
            "strong:Twist (Plaits)",
            "salt:twist_eng_{part}",
            "strong:Phase Distortion",
            "salt:pd_shape_{part}",
            "strong:Wavefolder",
            "salt:fold_source_{part}",
            "strong:Modal Resonator",
            "salt:modal_material_{part}",
            "salt:sync_shape_{part}",
            "strong:Oscillator 2",
            "salt:osc2_type_{part}",
            "strong:Oscillator 3",
            "salt:osc3_type_{part}",
            "strong:Osc Shaper",
            "salt:osc_ws_mode_{part}",
            "strong:Filter 1",
            "salt:filter_type_{part}",
            "salt:formant_voice_{part}",
            "salt:formant_vowel_{part}",
            "salt:filter_routing_{part}",
            "strong:Filter 2",
            "salt:filter2_type_{part}",
            "strong:Inter-Filter Shaper",
            "salt:inter_ws_mode_{part}",
            "strong:Amp Envelope",
            "strong:Filter Envelope",
            "strong:Dynamics",
            "salt:vel_curve_{part}",
            "strong:LFO",
            "salt:lfo_wf_{part}",
            "strong:LFO 2",
            "salt:lfo2_wf_{part}",
            "strong:LFO 3 (Mod Matrix)",
            "salt:lfo3_wf_{part}",
            "strong:LFO 4 (Mod Matrix)",
            "salt:lfo4_wf_{part}",
            "strong:Scene LFO 1",
            "salt:slfo1_wf_{part}",
            "strong:Scene LFO 2",
            "salt:slfo2_wf_{part}",
            "strong:Mod Matrix",
            "salt:mod_src_{part}_{slot_idx}",
            "salt:mod_dst_{part}_{slot_idx}",
            "strong:Play Mode",
            "salt:play_mode_{part}",
            "salt:sustain_mode_{part}",
            "strong:Portamento",
            "salt:porta_mode_{part}",
            "strong:Unison",
            "strong:Arpeggiator",
            "salt:arp_mode_{part}",
            "salt:arp_rate_{part}",
            "strong:Effects",
            "strong:Ring Modulator",
            "salt:rm_shape_{part}",
            "strong:Freq Shifter",
            "strong:Tape Saturation",
            "strong:Neuron Distortion",
            "strong:Delay",
            "strong:Reverb",
            "salt:reverb_type_{part}",
            "strong:Wave Shaper",
            "salt:ws_mode_{part}",
            "strong:Airwindows",
            "salt:aw_mode_{part}",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(found.len(), expected.len(), "marker count changed");
        for (i, (f, e)) in found.iter().zip(expected.iter()).enumerate() {
            assert_eq!(f, e, "layout marker #{i} changed");
        }
    }
}

/// Support code for the `params_sections_pin` source-parse test.
#[cfg(test)]
mod tests_support {
    /// Source text of a pin file (paths relative to src/gui/params.rs).
    fn file_src(file: &str) -> &'static str {
        match file {
            "params.rs" => include_str!("params.rs"),
            "params_osc.rs" => include_str!("params_osc.rs"),
            "params_filter.rs" => include_str!("params_filter.rs"),
            "params_env.rs" => include_str!("params_env.rs"),
            "params_lfo.rs" => include_str!("params_lfo.rs"),
            "params_matrix.rs" => include_str!("params_matrix.rs"),
            other => panic!("unknown pin source file: {other}"),
        }
    }

    /// Sections in the exact order `draw_params_editable_for` draws them:
    /// the head fn (preset strong-header) first, then the 9 frame sections.
    /// (file, fn_name) — file paths updated when sections move (phase B);
    /// semantics of the pin itself never change.
    pub(super) const SECTION_FNS: &[(&str, &str)] = &[
        ("params.rs", "draw_params_editable_for"),
        ("params_osc.rs", "draw_sec_osc"),
        ("params_filter.rs", "draw_sec_filter"),
        ("params_env.rs", "draw_sec_env"),
        ("params.rs", "draw_sec_dynamics"),
        ("params_lfo.rs", "draw_sec_lfo"),
        ("params_matrix.rs", "draw_sec_matrix"),
        ("params.rs", "draw_sec_voice"),
        ("params.rs", "draw_sec_arp"),
        ("params.rs", "draw_sec_effects"),
    ];

    /// First string literal in `rest`, e.g. `format!("x_{part}"))` → `x_{part}`.
    fn first_string_literal(rest: &str) -> String {
        let start = rest.find('"').expect("string literal after marker keyword");
        let rest = &rest[start + 1..];
        let end = rest.find('"').expect("closing quote");
        rest[..end].to_string()
    }

    /// Extract the ordered `ui.strong(...)` / `from_id_salt(...)` string
    /// literals from one fn body (mechanical layout fingerprint).
    fn extract_markers(src: &str, fn_name: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut in_fn = false;
        for line in src.lines() {
            if !in_fn {
                let t = line.trim_start();
                if line.starts_with("    ")
                    && (t.starts_with(&format!("fn {fn_name}("))
                        || t.starts_with(&format!("pub(super) fn {fn_name}(")))
                {
                    in_fn = true;
                }
                continue;
            }
            // fn body: ends at the impl-level closing brace (indent 4).
            if line == "    }" {
                break;
            }
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("ui.strong(") {
                out.push(format!("strong:{}", first_string_literal(rest)));
            } else if let Some(pos) = t.find("from_id_salt(") {
                let rest = &t[pos + "from_id_salt(".len()..];
                out.push(format!("salt:{}", first_string_literal(rest)));
            }
        }
        assert!(in_fn, "fn {fn_name} not found in source");
        out
    }

    pub(super) fn section_markers() -> Vec<String> {
        let mut found = Vec::new();
        for (file, fn_name) in SECTION_FNS {
            let src = file_src(file);
            found.extend(extract_markers(src, fn_name));
        }
        found
    }
}
