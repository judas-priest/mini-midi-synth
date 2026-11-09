//! LFO / Scene-LFO section of the params editor (split from params.rs, verbatim).

use eframe::egui;

use super::App;
use super::theme;
use super::LFO_WAVEFORM_NAMES;

const LFO_TRIGGER_MODE_NAMES: &[&str] = &["Free Run", "Key Trigger", "Random Start", "Random Unipolar"];

fn draw_lfo_trigger_mode(
    ui: &mut egui::Ui,
    part: usize,
    lfo_num: u8,
    params: &mut std::collections::BTreeMap<String, f32>,
) -> bool {
    let key = format!("lfo{lfo_num}_trigger_mode");
    let default = if lfo_num <= 2 { 1.0 } else { 0.0 };
    let mut mode = params.get(key.as_str()).copied().unwrap_or(default) as usize;
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Trigger:");
        egui::ComboBox::from_id_salt(format!("lfo_trig_{part}_{lfo_num}"))
            .selected_text(*LFO_TRIGGER_MODE_NAMES.get(mode).unwrap_or(&"Key Trigger"))
            .show_ui(ui, |ui| {
                for (i, name) in LFO_TRIGGER_MODE_NAMES.iter().enumerate() {
                    if ui.selectable_value(&mut mode, i, *name).clicked() {
                        params.insert(key.clone(), mode as f32);
                        changed = true;
                    }
                }
            });
    });
    changed
}

impl App {
    /// Verbatim frame section (lfo) extracted from draw_params_editable_for.
    pub(super) fn draw_sec_lfo(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        ui.style_mut().spacing.slider_width = theme::SLIDER_SECONDARY;

        // LFO
        ui.strong("LFO");
        ui.horizontal(|ui| {
            let mut lw = self.parts[part].edited_params.get("lfo_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo_wf_{part}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("lfo_waveform".into(), lw as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "lfo_rate", "Rate", 0.1, 20.0, true, " Hz");
        changed |= self.param_slider(ui, "lfo_pitch_depth", "Pitch Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_filter_depth", "Filter Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_amp_depth", "Amp Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_deform", "Deform", -1.0, 1.0, false);
        changed |= draw_lfo_trigger_mode(ui, part, 1, &mut self.parts[part].edited_params);

        ui.add_space(theme::SP_MD);

        // LFO 2
        ui.strong("LFO 2");
        ui.horizontal(|ui| {
            let mut lw2 = self.parts[part].edited_params.get("lfo2_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo2_wf_{part}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw2).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw2, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("lfo2_waveform".into(), lw2 as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "lfo2_rate", "Rate", 0.1, 20.0, true, " Hz");
        changed |= self.param_slider(ui, "lfo2_pitch_depth", "Pitch Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo2_filter_depth", "Filter Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo2_amp_depth", "Amp Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo2_deform", "Deform", -1.0, 1.0, false);
        changed |= draw_lfo_trigger_mode(ui, part, 2, &mut self.parts[part].edited_params);

        ui.add_space(theme::SP_MD);

        // LFO 3 & 4 (mod matrix sources)
        ui.strong("LFO 3 (Mod Matrix)");
        ui.horizontal(|ui| {
            let mut lw3 = self.parts[part].edited_params.get("lfo3_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo3_wf_{part}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw3).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw3, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("lfo3_waveform".into(), lw3 as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "lfo3_rate", "Rate", 0.1, 20.0, true, " Hz");
        changed |= self.param_slider(ui, "lfo3_deform", "Deform", -1.0, 1.0, false);
        changed |= draw_lfo_trigger_mode(ui, part, 3, &mut self.parts[part].edited_params);

        ui.strong("LFO 4 (Mod Matrix)");
        ui.horizontal(|ui| {
            let mut lw4 = self.parts[part].edited_params.get("lfo4_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo4_wf_{part}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw4).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw4, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("lfo4_waveform".into(), lw4 as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "lfo4_rate", "Rate", 0.1, 20.0, true, " Hz");
        changed |= self.param_slider(ui, "lfo4_deform", "Deform", -1.0, 1.0, false);
        changed |= draw_lfo_trigger_mode(ui, part, 4, &mut self.parts[part].edited_params);
        {
        }

        ui.add_space(theme::SP_MD);

        // Scene LFOs (free-running — never reset on note-on)
        ui.strong("Scene LFO 1");
        ui.label(egui::RichText::new("Free-running: phase never resets on note-on").small().weak());
        ui.horizontal(|ui| {
            let mut wf = self.parts[part].edited_params.get("slfo1_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Wave:");
            egui::ComboBox::from_id_salt(format!("slfo1_wf_{part}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(wf).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut wf, i, *name).clicked() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("slfo1_waveform".into(), wf as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "slfo1_rate", "Rate", 0.01, 20.0, true, " Hz");
        changed |= self.param_slider(ui, "slfo1_deform", "Deform", -1.0, 1.0, false);
        {
            let mut uni = self.parts[part].edited_params.get("slfo1_unipolar").copied().unwrap_or(0.0) > 0.5;
            let mut tsync = self.parts[part].edited_params.get("slfo1_tempo_sync").copied().unwrap_or(0.0) > 0.5;
            ui.horizontal(|ui| {
                if ui.checkbox(&mut uni, "Unipolar").changed() {
                    self.push_undo_snapshot_for(part);
                    self.parts[part].edited_params.insert("slfo1_unipolar".into(), if uni { 1.0 } else { 0.0 });
                    changed = true;
                }
                if ui.checkbox(&mut tsync, "Tempo Sync").changed() {
                    self.push_undo_snapshot_for(part);
                    self.parts[part].edited_params.insert("slfo1_tempo_sync".into(), if tsync { 1.0 } else { 0.0 });
                    changed = true;
                }
            });
        }

        ui.add_space(theme::SP_SM);
        ui.strong("Scene LFO 2");
        ui.horizontal(|ui| {
            let mut wf = self.parts[part].edited_params.get("slfo2_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Wave:");
            egui::ComboBox::from_id_salt(format!("slfo2_wf_{part}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(wf).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut wf, i, *name).clicked() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("slfo2_waveform".into(), wf as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider_ex(ui, "slfo2_rate", "Rate", 0.01, 20.0, true, " Hz");
        changed |= self.param_slider(ui, "slfo2_deform", "Deform", -1.0, 1.0, false);
        {
            let mut uni = self.parts[part].edited_params.get("slfo2_unipolar").copied().unwrap_or(0.0) > 0.5;
            let mut tsync = self.parts[part].edited_params.get("slfo2_tempo_sync").copied().unwrap_or(0.0) > 0.5;
            ui.horizontal(|ui| {
                if ui.checkbox(&mut uni, "Unipolar").changed() {
                    self.push_undo_snapshot_for(part);
                    self.parts[part].edited_params.insert("slfo2_unipolar".into(), if uni { 1.0 } else { 0.0 });
                    changed = true;
                }
                if ui.checkbox(&mut tsync, "Tempo Sync").changed() {
                    self.push_undo_snapshot_for(part);
                    self.parts[part].edited_params.insert("slfo2_tempo_sync".into(), if tsync { 1.0 } else { 0.0 });
                    changed = true;
                }
            });
        }
        }); // end LFO frame
        changed
    }
}
