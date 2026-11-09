//! Oscillator section of the params editor (split from params.rs, verbatim).

use eframe::egui;

use super::App;
use super::theme;
use super::{
    OSC_NAMES, SIMPLE_OSC_NAMES, PD_SHAPE_NAMES, FOLD_SOURCE_NAMES, MODAL_MATERIAL_NAMES,
    SYNC_SHAPE_NAMES, BASS_STYLE_NAMES, BASS_PICKUP_NAMES, BOWED_BODY_NAMES, BRASS_BELL_NAMES,
    ACCORDION_REGISTER_NAMES, SAX_TYPE_NAMES, ALIAS_WAVE_NAMES, WINDOW_TYPE_NAMES,
    WAVE_SHAPER_MODE_NAMES,
};

impl App {
    /// Verbatim frame section (osc) extracted from draw_params_editable_for.
    pub(super) fn draw_sec_osc(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Set secondary width for oscillator params
        ui.style_mut().spacing.slider_width = theme::SLIDER_SECONDARY;

        // Oscillator 1
        ui.strong("Oscillator 1");
        ui.horizontal(|ui| {
            let mut osc = self.parts[part].edited_params.get("osc_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("osc_type_{part}"))
                .selected_text(*OSC_NAMES.get(osc).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in OSC_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut osc, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("osc_type".into(), osc as f32);
                            changed = true;
                        }
                    }
                });

            let mut detune = self.parts[part].edited_params.get("osc_detune").copied().unwrap_or(0.0);
            ui.label("Detune:");
            let detune_resp = ui.add(egui::Slider::new(&mut detune, 0.0..=0.05).step_by(0.001)).on_hover_text("Oscillator pitch detune");
            if detune_resp.drag_started() { self.push_undo_snapshot_for(part); }
            if detune_resp.changed() {
                self.parts[part].edited_params.insert("osc_detune".into(), detune);
                changed = true;
            }
        });

        let osc_type = self.parts[part].edited_params.get("osc_type").copied().unwrap_or(0.0) as u32;
        let osc_is_simple = osc_type <= 4;

        // Noise level (for all osc types except pure Noise)
        if osc_type != 5 {
            changed |= self.param_slider(ui, "noise_level", "Noise Mix", 0.0, 1.0, false);
        }

        // FM params (FM and FM Piano)
        if osc_type == 4 || osc_type == 8 {
            changed |= self.param_slider(ui, "fm_ratio", "FM Ratio", 0.5, 8.0, false);
            changed |= self.param_slider(ui, "fm_index", "FM Index", 0.1, 10.0, false);
            if osc_type == 4 {
                changed |= self.param_slider(ui, "fm_env_amount", "FM Env Amount", 0.0, 1.0, false);
            }
        }

        // Physical model params (KS, Commuted Piano, Banded WG)
        if matches!(osc_type, 6 | 9 | 10) {
            changed |= self.param_slider(ui, "ks_brightness", "Brightness", 0.05, 1.0, false);
            changed |= self.param_slider(ui, "ks_feedback", "Feedback", 0.9, 0.9999, false);
        }

        // Organ drawbar params
        if osc_type == 7 {
            ui.add_space(theme::SP_SM);
            ui.strong("Drawbars");
            let drawbar_names = ["16'", "5⅓'", "8'", "4'", "2⅔'", "2'", "1⅗'", "1⅓'", "1'"];
            for (i, name) in drawbar_names.iter().enumerate() {
                let key = format!("drawbar_{}", i + 1);
                let mut val = self.parts[part].edited_params.get(&key).copied().unwrap_or(0.0);
                let db_resp = ui.add(egui::Slider::new(&mut val, 0.0..=8.0).step_by(1.0).text(*name)).on_hover_text("Organ drawbar level");
                if db_resp.drag_started() { self.push_undo_snapshot_for(part); }
                if db_resp.changed() {
                    self.parts[part].edited_params.insert(key, val);
                    changed = true;
                }
            }
        }

        // Drum synth params
        if osc_type == 12 {
            ui.add_space(theme::SP_SM);
            ui.strong("Drum Synth");
            changed |= self.param_slider_ex(ui, "drum_pitch_amount", "Pitch Sweep", 0.0, 72.0, false, " st");
            changed |= self.param_slider_ex(ui, "drum_pitch_decay", "Pitch Decay", 5.0, 200.0, false, " ms");
            changed |= self.param_slider(ui, "drum_noise_level", "Noise Level", 0.0, 1.0, false);
            changed |= self.param_slider_ex(ui, "drum_noise_decay", "Noise Decay", 5.0, 200.0, false, " ms");
            changed |= self.param_slider(ui, "drum_noise_color", "Noise Color", 0.0, 1.0, false);
        }

        // Bass guitar params
        if osc_type == 13 {
            ui.add_space(theme::SP_SM);
            ui.strong("Bass Guitar");
            ui.horizontal(|ui| {
                let mut style = self.parts[part].edited_params.get("bass_style").copied().unwrap_or(0.0) as usize;
                ui.label("Style:");
                egui::ComboBox::from_id_salt(format!("bass_style_{part}"))
                    .selected_text(*BASS_STYLE_NAMES.get(style).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BASS_STYLE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut style, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("bass_style".into(), style as f32);
                                changed = true;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                let mut pu = self.parts[part].edited_params.get("bass_pickup").copied().unwrap_or(0.0) as usize;
                ui.label("Pickup:");
                egui::ComboBox::from_id_salt(format!("bass_pickup_{part}"))
                    .selected_text(*BASS_PICKUP_NAMES.get(pu).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BASS_PICKUP_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut pu, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("bass_pickup".into(), pu as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "bass_tone", "Tone", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "bass_body", "Body", 0.0, 1.0, false);
        }

        // Bowed string params
        if osc_type == 14 {
            ui.add_space(theme::SP_SM);
            ui.strong("Bowed String");
            ui.horizontal(|ui| {
                let mut bt = self.parts[part].edited_params.get("body_type").copied().unwrap_or(0.0) as usize;
                ui.label("Body:");
                egui::ComboBox::from_id_salt(format!("body_type_{part}"))
                    .selected_text(*BOWED_BODY_NAMES.get(bt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BOWED_BODY_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut bt, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("body_type".into(), bt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "bow_pressure", "Bow Pressure", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "bow_position", "Bow Position", 0.0, 1.0, false);
        }

        // Brass params
        if osc_type == 15 {
            ui.add_space(theme::SP_SM);
            ui.strong("Brass");
            ui.horizontal(|ui| {
                let mut bt = self.parts[part].edited_params.get("bell_type").copied().unwrap_or(0.0) as usize;
                ui.label("Type:");
                egui::ComboBox::from_id_salt(format!("bell_type_{part}"))
                    .selected_text(*BRASS_BELL_NAMES.get(bt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BRASS_BELL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut bt, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("bell_type".into(), bt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "lip_tension", "Lip Tension", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "blowing_pressure", "Blowing Pressure", 0.0, 1.0, false);
        }

        // Accordion params
        if osc_type == 22 {
            ui.add_space(theme::SP_SM);
            ui.strong("Accordion");
            ui.horizontal(|ui| {
                let mut reg = self.parts[part].edited_params.get("accordion_register").copied().unwrap_or(0.0) as usize;
                ui.label("Register:");
                egui::ComboBox::from_id_salt(format!("accordion_reg_{part}"))
                    .selected_text(*ACCORDION_REGISTER_NAMES.get(reg).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ACCORDION_REGISTER_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut reg, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("accordion_register".into(), reg as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "accordion_bellows", "Bellows Pressure", 0.0, 1.0, false);
        }

        // Saxophone params
        if osc_type == 23 {
            ui.add_space(theme::SP_SM);
            ui.strong("Saxophone");
            ui.horizontal(|ui| {
                let mut st = self.parts[part].edited_params.get("sax_type").copied().unwrap_or(1.0) as usize;
                ui.label("Type:");
                egui::ComboBox::from_id_salt(format!("sax_type_{part}"))
                    .selected_text(*SAX_TYPE_NAMES.get(st).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in SAX_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut st, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("sax_type".into(), st as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "sax_reed_stiffness", "Reed Stiffness", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "sax_embouchure", "Embouchure", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "sax_blow_pressure", "Blow Pressure", 0.0, 1.0, false);
        }

        // Electric Piano params
        if osc_type == 24 {
            ui.add_space(theme::SP_SM);
            ui.strong("Electric Piano");
            ui.horizontal(|ui| {
                let mut ep_t = self.parts[part].edited_params.get("epiano_type").copied().unwrap_or(0.0) as usize;
                ui.label("Model:");
                egui::ComboBox::from_id_salt(format!("epiano_type_{part}"))
                    .selected_text(*["Rhodes MkII", "Wurlitzer 200A", "Stage 73"].get(ep_t).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ["Rhodes MkII", "Wurlitzer 200A", "Stage 73"].iter().enumerate() {
                            if ui.selectable_value(&mut ep_t, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("epiano_type".into(), ep_t as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "ks_brightness", "Brightness (Strike)", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "ks_feedback", "Sustain", 0.0, 1.0, false);
        }

        // Alias oscillator params
        if osc_type == 25 {
            ui.add_space(theme::SP_SM);
            ui.strong("Alias (8-bit)");
            ui.horizontal(|ui| {
                let mut wt = self.parts[part].edited_params.get("alias_wave_type").copied().unwrap_or(0.0) as usize;
                ui.label("Wave:");
                egui::ComboBox::from_id_salt(format!("alias_wave_{part}"))
                    .selected_text(*ALIAS_WAVE_NAMES.get(wt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ALIAS_WAVE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut wt, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("alias_wave_type".into(), wt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "alias_crush", "Bit Depth", 1.0, 8.0, false);
        }

        // Window oscillator params
        if osc_type == 26 {
            ui.add_space(theme::SP_SM);
            ui.strong("Window Oscillator");
            ui.horizontal(|ui| {
                let mut wt = self.parts[part].edited_params.get("window_type").copied().unwrap_or(0.0) as usize;
                ui.label("Window:");
                egui::ComboBox::from_id_salt(format!("window_type_{part}"))
                    .selected_text(*WINDOW_TYPE_NAMES.get(wt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in WINDOW_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut wt, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("window_type".into(), wt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "window_morph", "Morph", 0.0, 1.0, false);
            changed |= self.param_slider_ex(ui, "window_formant", "Formant", -24.0, 24.0, false, " st");
        }

        // Twist / Plaits (engine 29)
        if osc_type == 29 {
            ui.add_space(theme::SP_SM);
            ui.strong("Twist (Plaits)");
            // Engine selector
            const TWIST_ENGINES: &[&str] = &[
                // engine2 engines (0-7)
                "VA+VCF", "Phase Dist", "6-Op FM A", "6-Op FM B", "6-Op FM C",
                "Wave Terrain", "String Machine", "Chiptune",
                // classic Plaits engines (8-23)
                "VA (Waveforms)", "Waveshaper", "2-Op FM", "Grain",
                "Additive", "Wavetable", "Chords", "Vowels/Speech",
                "Swarm", "Noise", "Particle", "String (KS)",
                "Modal", "Bass Drum", "Snare Drum", "Hi-Hat",
            ];
            ui.horizontal(|ui| {
                let mut eng = self.parts[part].edited_params.get("twist_engine").copied().unwrap_or(8.0) as usize;
                ui.label("Engine:");
                egui::ComboBox::from_id_salt(format!("twist_eng_{part}"))
                    .width(160.0)
                    .selected_text(*TWIST_ENGINES.get(eng).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in TWIST_ENGINES.iter().enumerate() {
                            if ui.selectable_value(&mut eng, i, *name).clicked() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("twist_engine".into(), eng as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "twist_harmonics",  "Harmonics", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "twist_timbre",     "Timbre",    0.0, 1.0, false);
            changed |= self.param_slider(ui, "twist_morph",      "Morph",     0.0, 1.0, false);
            changed |= self.param_slider(ui, "twist_lpg_decay",  "LPG Decay", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "twist_lpg_colour", "LPG Colour",0.0, 1.0, false);
            changed |= self.param_slider(ui, "twist_aux_mix",    "Aux Mix",   0.0, 1.0, false);
        }

        // Square wave — pulse width control
        if osc_type == 2 {
            let pw = self.parts[part].edited_params.get("pulse_width").copied().unwrap_or(0.5);
            let mut pw_val = pw;
            ui.horizontal(|ui| {
                ui.label("Pulse Width");
                let pw_resp = ui.add(egui::Slider::new(&mut pw_val, 0.05..=0.95)).on_hover_text("Square wave duty cycle");
                if pw_resp.drag_started() { self.push_undo_snapshot_for(part); }
                if pw_resp.changed() {
                    self.parts[part].edited_params.insert("pulse_width".into(), pw_val);
                    self.parts[part].params_dirty = true;
                }
            });
        }

        // Phase Distortion params
        if osc_type == 16 {
            ui.add_space(theme::SP_SM);
            ui.strong("Phase Distortion");
            ui.horizontal(|ui| {
                let mut shape = self.parts[part].edited_params.get("pd_shape").copied().unwrap_or(0.0) as usize;
                ui.label("Shape:");
                egui::ComboBox::from_id_salt(format!("pd_shape_{part}"))
                    .selected_text(*PD_SHAPE_NAMES.get(shape).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in PD_SHAPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut shape, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("pd_shape".into(), shape as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "pd_depth", "Depth (DCW)", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "pd_env_amount", "Env \u{27A1} Depth", 0.0, 1.0, false);
        }

        // Wavefolder params
        if osc_type == 17 {
            ui.add_space(theme::SP_SM);
            ui.strong("Wavefolder");
            ui.horizontal(|ui| {
                let mut src = self.parts[part].edited_params.get("fold_source").copied().unwrap_or(0.0) as usize;
                ui.label("Source:");
                egui::ComboBox::from_id_salt(format!("fold_source_{part}"))
                    .selected_text(*FOLD_SOURCE_NAMES.get(src).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FOLD_SOURCE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut src, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("fold_source".into(), src as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "fold_amount", "Fold Amount", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "fold_symmetry", "Symmetry", 0.0, 1.0, false);
        }

        // Modal Resonator params
        if osc_type == 18 {
            ui.add_space(theme::SP_SM);
            ui.strong("Modal Resonator");
            ui.horizontal(|ui| {
                let mut mat = self.parts[part].edited_params.get("modal_material").copied().unwrap_or(0.0) as usize;
                ui.label("Material:");
                egui::ComboBox::from_id_salt(format!("modal_material_{part}"))
                    .selected_text(*MODAL_MATERIAL_NAMES.get(mat).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in MODAL_MATERIAL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut mat, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("modal_material".into(), mat as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "modal_brightness", "Brightness", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "modal_damping", "Damping", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "modal_strike_pos", "Strike Position", 0.0, 1.0, false);
        }

        // Hard Sync params
        if osc_type == 19 {
            ui.add_space(theme::SP_SM);
            ui.label("Hard Sync");
            ui.horizontal(|ui| {
                let mut shape = self.parts[part].edited_params.get("sync_shape").copied().unwrap_or(0.0) as usize;
                ui.label("Slave Wave:");
                egui::ComboBox::from_id_salt(format!("sync_shape_{part}"))
                    .selected_text(*SYNC_SHAPE_NAMES.get(shape).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in SYNC_SHAPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut shape, i, *name).changed() {
                                self.push_undo_snapshot_for(part);
                                self.parts[part].edited_params.insert("sync_shape".into(), shape as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "sync_ratio", "Sync Ratio", 1.0, 16.0, false);
        }

        // Supersaw params
        if osc_type == 20 {
            ui.add_space(theme::SP_SM);
            ui.label("Supersaw");
            changed |= self.param_slider(ui, "supersaw_detune", "Detune", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "supersaw_mix", "Mix", 0.0, 1.0, false);
        }

        // Multi-osc controls (only for simple osc types)
        if osc_is_simple {
            ui.add_space(theme::SP_SM);
            let mut osc_count = self.parts[part].edited_params.get("osc_count").copied().unwrap_or(1.0) as u32;
            ui.horizontal(|ui| {
                ui.label("Osc Count:");
                let oc_resp = ui.add(egui::Slider::new(&mut osc_count, 1..=3)).on_hover_text("Number of oscillators");
                if oc_resp.drag_started() { self.push_undo_snapshot_for(part); }
                if oc_resp.changed() {
                    self.parts[part].edited_params.insert("osc_count".into(), osc_count as f32);
                    changed = true;
                }
            });

            if osc_count >= 1 {
                changed |= self.param_slider(ui, "osc1_level", "Osc 1 Level", 0.0, 1.0, false);
            }

            if osc_count >= 2 {
                ui.add_space(theme::SP_SM);
                ui.strong("Oscillator 2");
                ui.horizontal(|ui| {
                    let mut osc2 = self.parts[part].edited_params.get("osc2_type").copied().unwrap_or(1.0) as usize;
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt(format!("osc2_type_{part}"))
                        .selected_text(*SIMPLE_OSC_NAMES.get(osc2).unwrap_or(&"?"))
                        .show_ui(ui, |ui| {
                            for (i, name) in SIMPLE_OSC_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut osc2, i, *name).changed() {
                                    self.push_undo_snapshot_for(part);
                                    self.parts[part].edited_params.insert("osc2_type".into(), osc2 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune2 = self.parts[part].edited_params.get("osc2_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    let d2_resp = ui.add(egui::Slider::new(&mut detune2, -0.05..=0.05).step_by(0.001)).on_hover_text("Osc 2 pitch detune");
                    if d2_resp.drag_started() { self.push_undo_snapshot_for(part); }
                    if d2_resp.changed() {
                        self.parts[part].edited_params.insert("osc2_detune".into(), detune2);
                        changed = true;
                    }
                });
                changed |= self.param_slider(ui, "osc2_level", "Osc 2 Level", 0.0, 1.0, false);
            }

            if osc_count >= 3 {
                ui.add_space(theme::SP_SM);
                ui.strong("Oscillator 3");
                ui.horizontal(|ui| {
                    let mut osc3 = self.parts[part].edited_params.get("osc3_type").copied().unwrap_or(1.0) as usize;
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt(format!("osc3_type_{part}"))
                        .selected_text(*SIMPLE_OSC_NAMES.get(osc3).unwrap_or(&"?"))
                        .show_ui(ui, |ui| {
                            for (i, name) in SIMPLE_OSC_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut osc3, i, *name).changed() {
                                    self.push_undo_snapshot_for(part);
                                    self.parts[part].edited_params.insert("osc3_type".into(), osc3 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune3 = self.parts[part].edited_params.get("osc3_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    let d3_resp = ui.add(egui::Slider::new(&mut detune3, -0.05..=0.05).step_by(0.001)).on_hover_text("Osc 3 pitch detune");
                    if d3_resp.drag_started() { self.push_undo_snapshot_for(part); }
                    if d3_resp.changed() {
                        self.parts[part].edited_params.insert("osc3_detune".into(), detune3);
                        changed = true;
                    }
                });
                changed |= self.param_slider(ui, "osc3_level", "Osc 3 Level", 0.0, 1.0, false);
            }

            // FM cross-routing (osc1 → osc2/3 freq mod) — only show when multi-osc
            if osc_count >= 2 {
                changed |= self.param_slider(ui, "fm_cross_depth", "FM Cross Depth", 0.0, 4.0, false);
            }
        }

        // Osc Waveshaper (pre-filter, per-voice)
        ui.add_space(theme::SP_SM);
        ui.strong("Osc Shaper");
        ui.horizontal(|ui| {
            let mut wsm = self.parts[part].edited_params.get("osc_ws_mode").copied().unwrap_or(0.0) as usize;
            ui.label("Mode:");
            egui::ComboBox::from_id_salt(format!("osc_ws_mode_{part}"))
                .selected_text(*WAVE_SHAPER_MODE_NAMES.get(wsm).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in WAVE_SHAPER_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut wsm, i, *name).changed() {
                            self.push_undo_snapshot_for(part);
                            self.parts[part].edited_params.insert("osc_ws_mode".into(), wsm as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "osc_ws_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "osc_ws_mix", "Mix", 0.0, 1.0, false);
        }); // end Oscillator frame
        changed
    }
}
