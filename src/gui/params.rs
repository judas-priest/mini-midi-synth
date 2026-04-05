/// Synth parameter editor UI.

use eframe::egui;

use super::App;
use super::{
    OSC_NAMES, SIMPLE_OSC_NAMES, FILTER_NAMES, FILTER_ROUTING_NAMES,
    PD_SHAPE_NAMES, FOLD_SOURCE_NAMES, MODAL_MATERIAL_NAMES, SYNC_SHAPE_NAMES,
    BASS_STYLE_NAMES, BASS_PICKUP_NAMES, BOWED_BODY_NAMES, BRASS_BELL_NAMES,
    ACCORDION_REGISTER_NAMES, SAX_TYPE_NAMES, ALIAS_WAVE_NAMES, WINDOW_TYPE_NAMES,
    ENV_SHAPE_NAMES, FORMANT_VOICE_NAMES, FORMANT_VOWEL_NAMES, LFO_WAVEFORM_NAMES,
    VELOCITY_CURVE_NAMES, PORTAMENTO_MODE_NAMES, REVERB_TYPE_NAMES, RING_MOD_SHAPE_NAMES,
    LAYER_NAMES,
};

impl App {
    pub(super) fn draw_params_editable(&mut self, ui: &mut egui::Ui) {
        let layer = self.active_layer;
        let preset_idx = self.layers[layer].preset_idx;
        let preset_name = self
            .presets
            .get(preset_idx)
            .map(|p| p.name.as_str())
            .unwrap_or("(none)");

        let layer_label = LAYER_NAMES.get(layer).unwrap_or(&"?");
        ui.strong(format!("{preset_name} (Layer {layer_label})"));
        ui.add_space(6.0);

        let mut changed = false;

        // Oscillator 1
        ui.strong("Oscillator 1");
        ui.horizontal(|ui| {
            let mut osc = self.layers[layer].edited_params.get("osc_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("osc_type_{layer}"))
                .selected_text(*OSC_NAMES.get(osc).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in OSC_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut osc, i, *name).changed() {
                            self.layers[layer].edited_params.insert("osc_type".into(), osc as f32);
                            changed = true;
                        }
                    }
                });

            let mut detune = self.layers[layer].edited_params.get("osc_detune").copied().unwrap_or(0.0);
            ui.label("Detune:");
            if ui.add(egui::Slider::new(&mut detune, 0.0..=0.05).step_by(0.001)).changed() {
                self.layers[layer].edited_params.insert("osc_detune".into(), detune);
                changed = true;
            }
        });

        let osc_type = self.layers[layer].edited_params.get("osc_type").copied().unwrap_or(0.0) as u32;
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
            ui.add_space(4.0);
            ui.strong("Drawbars");
            let drawbar_names = ["16'", "5⅓'", "8'", "4'", "2⅔'", "2'", "1⅗'", "1⅓'", "1'"];
            for (i, name) in drawbar_names.iter().enumerate() {
                let key = format!("drawbar_{}", i + 1);
                let mut val = self.layers[layer].edited_params.get(&key).copied().unwrap_or(0.0);
                if ui.add(egui::Slider::new(&mut val, 0.0..=8.0).step_by(1.0).text(*name)).changed() {
                    self.layers[layer].edited_params.insert(key, val);
                    changed = true;
                }
            }
        }

        // Drum synth params
        if osc_type == 12 {
            ui.add_space(4.0);
            ui.strong("Drum Synth");
            changed |= self.param_slider(ui, "drum_pitch_amount", "Pitch Sweep (st)", 0.0, 72.0, false);
            changed |= self.param_slider(ui, "drum_pitch_decay", "Pitch Decay", 5.0, 200.0, false);
            changed |= self.param_slider(ui, "drum_noise_level", "Noise Level", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "drum_noise_decay", "Noise Decay", 5.0, 200.0, false);
            changed |= self.param_slider(ui, "drum_noise_color", "Noise Color", 0.0, 1.0, false);
        }

        // Bass guitar params
        if osc_type == 13 {
            ui.add_space(4.0);
            ui.strong("Bass Guitar");
            ui.horizontal(|ui| {
                let mut style = self.layers[layer].edited_params.get("bass_style").copied().unwrap_or(0.0) as usize;
                ui.label("Style:");
                egui::ComboBox::from_id_salt(format!("bass_style_{layer}"))
                    .selected_text(*BASS_STYLE_NAMES.get(style).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BASS_STYLE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut style, i, *name).changed() {
                                self.layers[layer].edited_params.insert("bass_style".into(), style as f32);
                                changed = true;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                let mut pu = self.layers[layer].edited_params.get("bass_pickup").copied().unwrap_or(0.0) as usize;
                ui.label("Pickup:");
                egui::ComboBox::from_id_salt(format!("bass_pickup_{layer}"))
                    .selected_text(*BASS_PICKUP_NAMES.get(pu).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BASS_PICKUP_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut pu, i, *name).changed() {
                                self.layers[layer].edited_params.insert("bass_pickup".into(), pu as f32);
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
            ui.add_space(4.0);
            ui.strong("Bowed String");
            ui.horizontal(|ui| {
                let mut bt = self.layers[layer].edited_params.get("body_type").copied().unwrap_or(0.0) as usize;
                ui.label("Body:");
                egui::ComboBox::from_id_salt(format!("body_type_{layer}"))
                    .selected_text(*BOWED_BODY_NAMES.get(bt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BOWED_BODY_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut bt, i, *name).changed() {
                                self.layers[layer].edited_params.insert("body_type".into(), bt as f32);
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
            ui.add_space(4.0);
            ui.strong("Brass");
            ui.horizontal(|ui| {
                let mut bt = self.layers[layer].edited_params.get("bell_type").copied().unwrap_or(0.0) as usize;
                ui.label("Type:");
                egui::ComboBox::from_id_salt(format!("bell_type_{layer}"))
                    .selected_text(*BRASS_BELL_NAMES.get(bt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in BRASS_BELL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut bt, i, *name).changed() {
                                self.layers[layer].edited_params.insert("bell_type".into(), bt as f32);
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
            ui.add_space(4.0);
            ui.strong("Accordion");
            ui.horizontal(|ui| {
                let mut reg = self.layers[layer].edited_params.get("accordion_register").copied().unwrap_or(0.0) as usize;
                ui.label("Register:");
                egui::ComboBox::from_id_salt(format!("accordion_reg_{layer}"))
                    .selected_text(*ACCORDION_REGISTER_NAMES.get(reg).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ACCORDION_REGISTER_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut reg, i, *name).changed() {
                                self.layers[layer].edited_params.insert("accordion_register".into(), reg as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "accordion_bellows", "Bellows Pressure", 0.0, 1.0, false);
        }

        // Saxophone params
        if osc_type == 23 {
            ui.add_space(4.0);
            ui.strong("Saxophone");
            ui.horizontal(|ui| {
                let mut st = self.layers[layer].edited_params.get("sax_type").copied().unwrap_or(1.0) as usize;
                ui.label("Type:");
                egui::ComboBox::from_id_salt(format!("sax_type_{layer}"))
                    .selected_text(*SAX_TYPE_NAMES.get(st).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in SAX_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut st, i, *name).changed() {
                                self.layers[layer].edited_params.insert("sax_type".into(), st as f32);
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
            ui.add_space(4.0);
            ui.strong("Electric Piano");
            ui.horizontal(|ui| {
                let mut ep_t = self.layers[layer].edited_params.get("epiano_type").copied().unwrap_or(0.0) as usize;
                ui.label("Model:");
                egui::ComboBox::from_id_salt(format!("epiano_type_{layer}"))
                    .selected_text(*["Rhodes MkII", "Wurlitzer 200A", "Stage 73"].get(ep_t).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ["Rhodes MkII", "Wurlitzer 200A", "Stage 73"].iter().enumerate() {
                            if ui.selectable_value(&mut ep_t, i, *name).changed() {
                                self.layers[layer].edited_params.insert("epiano_type".into(), ep_t as f32);
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
            ui.add_space(4.0);
            ui.strong("Alias (8-bit)");
            ui.horizontal(|ui| {
                let mut wt = self.layers[layer].edited_params.get("alias_wave_type").copied().unwrap_or(0.0) as usize;
                ui.label("Wave:");
                egui::ComboBox::from_id_salt(format!("alias_wave_{layer}"))
                    .selected_text(*ALIAS_WAVE_NAMES.get(wt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in ALIAS_WAVE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut wt, i, *name).changed() {
                                self.layers[layer].edited_params.insert("alias_wave_type".into(), wt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "alias_crush", "Bit Depth", 1.0, 8.0, false);
        }

        // Window oscillator params
        if osc_type == 26 {
            ui.add_space(4.0);
            ui.strong("Window Oscillator");
            ui.horizontal(|ui| {
                let mut wt = self.layers[layer].edited_params.get("window_type").copied().unwrap_or(0.0) as usize;
                ui.label("Window:");
                egui::ComboBox::from_id_salt(format!("window_type_{layer}"))
                    .selected_text(*WINDOW_TYPE_NAMES.get(wt).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in WINDOW_TYPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut wt, i, *name).changed() {
                                self.layers[layer].edited_params.insert("window_type".into(), wt as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "window_morph", "Morph", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "window_formant", "Formant", -24.0, 24.0, false);
        }

        // Square wave — pulse width control
        if osc_type == 2 {
            let pw = self.layers[layer].edited_params.get("pulse_width").copied().unwrap_or(0.5);
            let mut pw_val = pw;
            ui.horizontal(|ui| {
                ui.label("Pulse Width");
                if ui.add(egui::Slider::new(&mut pw_val, 0.05..=0.95)).changed() {
                    self.layers[layer].edited_params.insert("pulse_width".into(), pw_val);
                    self.layers[layer].params_dirty = true;
                }
            });
        }

        // Phase Distortion params
        if osc_type == 16 {
            ui.add_space(4.0);
            ui.strong("Phase Distortion");
            ui.horizontal(|ui| {
                let mut shape = self.layers[layer].edited_params.get("pd_shape").copied().unwrap_or(0.0) as usize;
                ui.label("Shape:");
                egui::ComboBox::from_id_salt(format!("pd_shape_{layer}"))
                    .selected_text(*PD_SHAPE_NAMES.get(shape).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in PD_SHAPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut shape, i, *name).changed() {
                                self.layers[layer].edited_params.insert("pd_shape".into(), shape as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "pd_depth", "Depth (DCW)", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "pd_env_amount", "Env → Depth", 0.0, 1.0, false);
        }

        // Wavefolder params
        if osc_type == 17 {
            ui.add_space(4.0);
            ui.strong("Wavefolder");
            ui.horizontal(|ui| {
                let mut src = self.layers[layer].edited_params.get("fold_source").copied().unwrap_or(0.0) as usize;
                ui.label("Source:");
                egui::ComboBox::from_id_salt(format!("fold_source_{layer}"))
                    .selected_text(*FOLD_SOURCE_NAMES.get(src).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FOLD_SOURCE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut src, i, *name).changed() {
                                self.layers[layer].edited_params.insert("fold_source".into(), src as f32);
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
            ui.add_space(4.0);
            ui.strong("Modal Resonator");
            ui.horizontal(|ui| {
                let mut mat = self.layers[layer].edited_params.get("modal_material").copied().unwrap_or(0.0) as usize;
                ui.label("Material:");
                egui::ComboBox::from_id_salt(format!("modal_material_{layer}"))
                    .selected_text(*MODAL_MATERIAL_NAMES.get(mat).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in MODAL_MATERIAL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut mat, i, *name).changed() {
                                self.layers[layer].edited_params.insert("modal_material".into(), mat as f32);
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
            ui.add_space(4.0);
            ui.label("Hard Sync");
            ui.horizontal(|ui| {
                let mut shape = self.layers[layer].edited_params.get("sync_shape").copied().unwrap_or(0.0) as usize;
                ui.label("Slave Wave:");
                egui::ComboBox::from_id_salt(format!("sync_shape_{layer}"))
                    .selected_text(*SYNC_SHAPE_NAMES.get(shape).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in SYNC_SHAPE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut shape, i, *name).changed() {
                                self.layers[layer].edited_params.insert("sync_shape".into(), shape as f32);
                                changed = true;
                            }
                        }
                    });
            });
            changed |= self.param_slider(ui, "sync_ratio", "Sync Ratio", 1.0, 16.0, false);
        }

        // Supersaw params
        if osc_type == 20 {
            ui.add_space(4.0);
            ui.label("Supersaw");
            changed |= self.param_slider(ui, "supersaw_detune", "Detune", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "supersaw_mix", "Mix", 0.0, 1.0, false);
        }

        // Multi-osc controls (only for simple osc types)
        if osc_is_simple {
            ui.add_space(4.0);
            let mut osc_count = self.layers[layer].edited_params.get("osc_count").copied().unwrap_or(1.0) as u32;
            ui.horizontal(|ui| {
                ui.label("Osc Count:");
                if ui.add(egui::Slider::new(&mut osc_count, 1..=3)).changed() {
                    self.layers[layer].edited_params.insert("osc_count".into(), osc_count as f32);
                    changed = true;
                }
            });

            if osc_count >= 1 {
                changed |= self.param_slider(ui, "osc1_level", "Osc 1 Level", 0.0, 1.0, false);
            }

            if osc_count >= 2 {
                ui.add_space(4.0);
                ui.strong("Oscillator 2");
                ui.horizontal(|ui| {
                    let mut osc2 = self.layers[layer].edited_params.get("osc2_type").copied().unwrap_or(1.0) as usize;
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt(format!("osc2_type_{layer}"))
                        .selected_text(*SIMPLE_OSC_NAMES.get(osc2).unwrap_or(&"?"))
                        .show_ui(ui, |ui| {
                            for (i, name) in SIMPLE_OSC_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut osc2, i, *name).changed() {
                                    self.layers[layer].edited_params.insert("osc2_type".into(), osc2 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune2 = self.layers[layer].edited_params.get("osc2_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    if ui.add(egui::Slider::new(&mut detune2, -0.05..=0.05).step_by(0.001)).changed() {
                        self.layers[layer].edited_params.insert("osc2_detune".into(), detune2);
                        changed = true;
                    }
                });
                changed |= self.param_slider(ui, "osc2_level", "Osc 2 Level", 0.0, 1.0, false);
            }

            if osc_count >= 3 {
                ui.add_space(4.0);
                ui.strong("Oscillator 3");
                ui.horizontal(|ui| {
                    let mut osc3 = self.layers[layer].edited_params.get("osc3_type").copied().unwrap_or(1.0) as usize;
                    ui.label("Type:");
                    egui::ComboBox::from_id_salt(format!("osc3_type_{layer}"))
                        .selected_text(*SIMPLE_OSC_NAMES.get(osc3).unwrap_or(&"?"))
                        .show_ui(ui, |ui| {
                            for (i, name) in SIMPLE_OSC_NAMES.iter().enumerate() {
                                if ui.selectable_value(&mut osc3, i, *name).changed() {
                                    self.layers[layer].edited_params.insert("osc3_type".into(), osc3 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune3 = self.layers[layer].edited_params.get("osc3_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    if ui.add(egui::Slider::new(&mut detune3, -0.05..=0.05).step_by(0.001)).changed() {
                        self.layers[layer].edited_params.insert("osc3_detune".into(), detune3);
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

        ui.add_space(6.0);

        // Filter 1
        ui.strong("Filter 1");
        ui.horizontal(|ui| {
            let mut ft = self.layers[layer].edited_params.get("filter_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("filter_type_{layer}"))
                .selected_text(*FILTER_NAMES.get(ft).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in FILTER_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut ft, i, *name).changed() {
                            self.layers[layer].edited_params.insert("filter_type".into(), ft as f32);
                            changed = true;
                        }
                    }
                });
        });

        let filter_type = self.layers[layer].edited_params.get("filter_type").copied().unwrap_or(0.0) as u32;
        let is_formant = filter_type == 3;

        if is_formant {
            // Formant filter controls
            ui.horizontal(|ui| {
                let mut fv = self.layers[layer].edited_params.get("formant_voice").copied().unwrap_or(0.0) as usize;
                ui.label("Voice:");
                egui::ComboBox::from_id_salt(format!("formant_voice_{layer}"))
                    .selected_text(*FORMANT_VOICE_NAMES.get(fv).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FORMANT_VOICE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut fv, i, *name).changed() {
                                self.layers[layer].edited_params.insert("formant_voice".into(), fv as f32);
                                changed = true;
                            }
                        }
                    });

                let mut vw = self.layers[layer].edited_params.get("formant_vowel").copied().unwrap_or(0.0) as usize;
                ui.label("Vowel:");
                egui::ComboBox::from_id_salt(format!("formant_vowel_{layer}"))
                    .selected_text(*FORMANT_VOWEL_NAMES.get(vw).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FORMANT_VOWEL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut vw, i, *name).changed() {
                                self.layers[layer].edited_params.insert("formant_vowel".into(), vw as f32);
                                changed = true;
                            }
                        }
                    });
            });
        } else {
            changed |= self.param_slider(ui, "filter_cutoff", "Cutoff", 20.0, 20000.0, true);
            changed |= self.param_slider(ui, "filter_resonance", "Resonance", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "filter_env_amount", "Env Amount", 0.0, 15000.0, false);
            changed |= self.param_slider(ui, "filter_key_track", "Key Track", 0.0, 1.0, false);
        }

        // Filter routing (not available with formant filter)
        if !is_formant {
            ui.add_space(4.0);
            let mut routing = self.layers[layer].edited_params.get("filter_routing").copied().unwrap_or(0.0) as usize;
            ui.horizontal(|ui| {
                ui.label("Routing:");
                egui::ComboBox::from_id_salt(format!("filter_routing_{layer}"))
                    .selected_text(*FILTER_ROUTING_NAMES.get(routing).unwrap_or(&"Single"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FILTER_ROUTING_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut routing, i, *name).changed() {
                                self.layers[layer].edited_params.insert("filter_routing".into(), routing as f32);
                                changed = true;
                            }
                        }
                    });
            });

            // Filter 2 controls (only when routing != Single)
            if routing >= 1 {
                ui.add_space(4.0);
                ui.strong("Filter 2");
                ui.horizontal(|ui| {
                    let ft2 = self.layers[layer].edited_params.get("filter2_type").copied().unwrap_or(0.0) as usize;
                    ui.label("Type:");
                    // Filter 2: LP/HP/BP + Moog (skip Formant index 3)
                    let f2_names: &[&str] = &["LowPass", "HighPass", "BandPass", "Moog 24dB", "Moog 12dB", "Diode 18dB", "Comb", "Allpass", "Comb+", "Comb-"];
                    let f2_values: &[usize] = &[0, 1, 2, 4, 5, 6, 7, 8, 9, 10]; // maps to FilterType param values
                    let f2_display = f2_values.iter().position(|&v| v == ft2).unwrap_or(0);
                    let mut f2_sel = f2_display;
                    egui::ComboBox::from_id_salt(format!("filter2_type_{layer}"))
                        .selected_text(*f2_names.get(f2_sel).unwrap_or(&"LowPass"))
                        .show_ui(ui, |ui| {
                            for (i, name) in f2_names.iter().enumerate() {
                                if ui.selectable_value(&mut f2_sel, i, *name).changed() {
                                    let param_val = f2_values[f2_sel];
                                    self.layers[layer].edited_params.insert("filter2_type".into(), param_val as f32);
                                    changed = true;
                                }
                            }
                        });
                });
                changed |= self.param_slider(ui, "filter2_cutoff", "Cutoff 2", 20.0, 20000.0, true);
                changed |= self.param_slider(ui, "filter2_resonance", "Resonance 2", 0.0, 1.0, false);
            }
        }

        ui.add_space(6.0);

        // Amp Envelope
        ui.strong("Amp Envelope");
        changed |= self.param_slider(ui, "amp_attack", "Attack", 0.001, 5.0, true);
        changed |= self.param_slider(ui, "amp_decay", "Decay", 0.0, 5.0, false);
        changed |= self.param_slider(ui, "amp_sustain", "Sustain", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "amp_release", "Release", 0.001, 5.0, true);

        ui.add_space(6.0);

        // Filter Envelope
        ui.strong("Filter Envelope");
        changed |= self.param_slider(ui, "filter_attack", "Attack", 0.001, 5.0, true);
        changed |= self.param_slider(ui, "filter_decay", "Decay", 0.0, 5.0, false);
        changed |= self.param_slider(ui, "filter_sustain", "Sustain", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "filter_release", "Release", 0.001, 5.0, true);

        // Envelope shapes
        ui.horizontal(|ui| {
            let mut as_ = self.layers[layer].edited_params.get("env_attack_shape").copied().unwrap_or(0.0) as usize;
            ui.label("Atk Shape:");
            egui::ComboBox::from_id_salt(format!("env_atk_shape_{layer}"))
                .selected_text(*ENV_SHAPE_NAMES.get(as_).unwrap_or(&"Sqrt"))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for (i, name) in ENV_SHAPE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut as_, i, *name).changed() {
                            self.layers[layer].edited_params.insert("env_attack_shape".into(), as_ as f32);
                            changed = true;
                        }
                    }
                });
            let mut ds = self.layers[layer].edited_params.get("env_decay_shape").copied().unwrap_or(0.0) as usize;
            ui.label("Dec/Rel:");
            egui::ComboBox::from_id_salt(format!("env_dec_shape_{layer}"))
                .selected_text(*ENV_SHAPE_NAMES.get(ds).unwrap_or(&"Sqrt"))
                .width(90.0)
                .show_ui(ui, |ui| {
                    for (i, name) in ENV_SHAPE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut ds, i, *name).changed() {
                            self.layers[layer].edited_params.insert("env_decay_shape".into(), ds as f32);
                            changed = true;
                        }
                    }
                });
        });

        ui.add_space(6.0);

        // Dynamics
        ui.strong("Dynamics");
        ui.horizontal(|ui| {
            let mut vc = self.layers[layer].edited_params.get("velocity_curve").copied().unwrap_or(0.0) as usize;
            ui.label("Vel Curve:");
            egui::ComboBox::from_id_salt(format!("vel_curve_{layer}"))
                .selected_text(*VELOCITY_CURVE_NAMES.get(vc).unwrap_or(&"Linear"))
                .show_ui(ui, |ui| {
                    for (i, name) in VELOCITY_CURVE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut vc, i, *name).changed() {
                            self.layers[layer].edited_params.insert("velocity_curve".into(), vc as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "vel_to_filter", "Vel->Filter", 0.0, 1.0, false);

        ui.add_space(6.0);

        // LFO
        ui.strong("LFO");
        ui.horizontal(|ui| {
            let mut lw = self.layers[layer].edited_params.get("lfo_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo_wf_{layer}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw, i, *name).changed() {
                            self.layers[layer].edited_params.insert("lfo_waveform".into(), lw as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "lfo_rate", "Rate", 0.1, 20.0, true);
        changed |= self.param_slider(ui, "lfo_pitch_depth", "Pitch Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_filter_depth", "Filter Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_amp_depth", "Amp Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo_deform", "Deform", -1.0, 1.0, false);

        ui.add_space(6.0);

        // LFO 2
        ui.strong("LFO 2");
        ui.horizontal(|ui| {
            let mut lw2 = self.layers[layer].edited_params.get("lfo2_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo2_wf_{layer}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw2).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw2, i, *name).changed() {
                            self.layers[layer].edited_params.insert("lfo2_waveform".into(), lw2 as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "lfo2_rate", "Rate", 0.1, 20.0, true);
        changed |= self.param_slider(ui, "lfo2_pitch_depth", "Pitch Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo2_filter_depth", "Filter Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo2_amp_depth", "Amp Depth", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "lfo2_deform", "Deform", -1.0, 1.0, false);

        ui.add_space(6.0);

        // LFO 3 & 4 (mod matrix sources)
        ui.strong("LFO 3 (Mod Matrix)");
        ui.horizontal(|ui| {
            let mut lw3 = self.layers[layer].edited_params.get("lfo3_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo3_wf_{layer}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw3).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw3, i, *name).changed() {
                            self.layers[layer].edited_params.insert("lfo3_waveform".into(), lw3 as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "lfo3_rate", "Rate", 0.1, 20.0, true);
        changed |= self.param_slider(ui, "lfo3_deform", "Deform", -1.0, 1.0, false);

        ui.strong("LFO 4 (Mod Matrix)");
        ui.horizontal(|ui| {
            let mut lw4 = self.layers[layer].edited_params.get("lfo4_waveform").copied().unwrap_or(0.0) as usize;
            ui.label("Waveform:");
            egui::ComboBox::from_id_salt(format!("lfo4_wf_{layer}"))
                .selected_text(*LFO_WAVEFORM_NAMES.get(lw4).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in LFO_WAVEFORM_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut lw4, i, *name).changed() {
                            self.layers[layer].edited_params.insert("lfo4_waveform".into(), lw4 as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "lfo4_rate", "Rate", 0.1, 20.0, true);
        changed |= self.param_slider(ui, "lfo4_deform", "Deform", -1.0, 1.0, false);

        ui.add_space(6.0);

        // Modulation Matrix
        ui.strong("Mod Matrix");
        {
            use crate::synth::mod_matrix::{ModSource, ModDest, MOD_SLOTS};
            let ep = &mut self.layers[layer].edited_params;
            for slot_idx in 0..MOD_SLOTS {
                let prefix = format!("mod_{slot_idx}_");
                let src_val = ep.get(&format!("{prefix}source")).copied().unwrap_or(0.0);
                let dst_val = ep.get(&format!("{prefix}dest")).copied().unwrap_or(0.0);
                let depth_val = ep.get(&format!("{prefix}depth")).copied().unwrap_or(0.0);
                let src = ModSource::from_param(src_val);
                let dst = ModDest::from_param(dst_val);

                // Only show if this slot or any prior slot is active, plus one empty row
                if src == ModSource::None && dst == ModDest::None && slot_idx > 0 {
                    // Check if previous slot was also empty — stop showing
                    let prev_src = ep.get(&format!("mod_{}_source", slot_idx - 1)).copied().unwrap_or(0.0);
                    let prev_dst = ep.get(&format!("mod_{}_dest", slot_idx - 1)).copied().unwrap_or(0.0);
                    if prev_src == 0.0 && prev_dst == 0.0 && slot_idx > 1 {
                        break;
                    }
                }

                ui.horizontal(|ui| {
                    ui.label(format!("{}:", slot_idx + 1));

                    // Source dropdown
                    let mut src_sel = ModSource::ALL.iter().position(|&s| s == src).unwrap_or(0);
                    egui::ComboBox::from_id_salt(format!("mod_src_{layer}_{slot_idx}"))
                        .width(80.0)
                        .selected_text(src.name())
                        .show_ui(ui, |ui| {
                            for (i, s) in ModSource::ALL.iter().enumerate() {
                                if ui.selectable_value(&mut src_sel, i, s.name()).changed() {
                                    ep.insert(format!("{prefix}source"), ModSource::ALL[src_sel] as u8 as f32);
                                    changed = true;
                                }
                            }
                        });

                    // Dest dropdown
                    let mut dst_sel = ModDest::ALL.iter().position(|&d| d == dst).unwrap_or(0);
                    egui::ComboBox::from_id_salt(format!("mod_dst_{layer}_{slot_idx}"))
                        .width(90.0)
                        .selected_text(dst.name())
                        .show_ui(ui, |ui| {
                            for (i, d) in ModDest::ALL.iter().enumerate() {
                                if ui.selectable_value(&mut dst_sel, i, d.name()).changed() {
                                    ep.insert(format!("{prefix}dest"), ModDest::ALL[dst_sel] as u8 as f32);
                                    changed = true;
                                }
                            }
                        });

                    // Depth slider
                    let mut depth = depth_val;
                    if ui.add(egui::Slider::new(&mut depth, -1.0..=1.0).text("Depth").step_by(0.01)).changed() {
                        ep.insert(format!("{prefix}depth"), depth);
                        changed = true;
                    }
                });
            }
        }

        ui.add_space(6.0);

        // Portamento
        ui.strong("Portamento");
        ui.horizontal(|ui| {
            let mut pm = self.layers[layer].edited_params.get("portamento_mode").copied().unwrap_or(0.0) as usize;
            ui.label("Mode:");
            egui::ComboBox::from_id_salt(format!("porta_mode_{layer}"))
                .selected_text(*PORTAMENTO_MODE_NAMES.get(pm).unwrap_or(&"Off"))
                .show_ui(ui, |ui| {
                    for (i, name) in PORTAMENTO_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut pm, i, *name).changed() {
                            self.layers[layer].edited_params.insert("portamento_mode".into(), pm as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "portamento_time", "Time", 0.0, 2.0, false);

        ui.add_space(6.0);

        // Unison
        ui.strong("Unison");
        {
            let mut uv = self.layers[layer].edited_params.get("unison_voices").copied().unwrap_or(1.0) as u32;
            if ui.add(egui::Slider::new(&mut uv, 1..=8).text("Voices")).changed() {
                self.layers[layer].edited_params.insert("unison_voices".into(), uv as f32);
                changed = true;
            }
        }
        changed |= self.param_slider(ui, "unison_detune", "Detune (cents)", 0.0, 50.0, false);
        changed |= self.param_slider(ui, "unison_spread", "Spread", 0.0, 1.0, false);

        ui.add_space(6.0);

        // Effects
        ui.strong("Effects");
        changed |= self.param_slider(ui, "chorus_mix", "Chorus", 0.0, 1.0, false);

        ui.add_space(4.0);
        ui.strong("Ring Modulator");
        changed |= self.param_slider(ui, "ring_mod_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "ring_mod_freq", "Carrier Freq", 20.0, 8000.0, true);
        ui.horizontal(|ui| {
            let mut rs = self.layers[layer].edited_params.get("ring_mod_shape").copied().unwrap_or(0.0) as usize;
            ui.label("Shape:");
            egui::ComboBox::from_id_salt(format!("rm_shape_{layer}"))
                .selected_text(*RING_MOD_SHAPE_NAMES.get(rs).unwrap_or(&"Sine"))
                .show_ui(ui, |ui| {
                    for (i, name) in RING_MOD_SHAPE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut rs, i, *name).changed() {
                            self.layers[layer].edited_params.insert("ring_mod_shape".into(), rs as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "ring_mod_bias", "Diode Bias", 0.0, 2.0, false);
        changed |= self.param_slider(ui, "ring_mod_linear", "Diode Linear", 0.01, 2.0, false);

        ui.add_space(4.0);
        ui.strong("Freq Shifter");
        changed |= self.param_slider(ui, "freq_shift_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "freq_shift_hz", "Shift (Hz)", -1000.0, 1000.0, false);
        changed |= self.param_slider(ui, "freq_shift_feedback", "Feedback", 0.0, 0.9, false);
        changed |= self.param_slider(ui, "freq_shift_delay", "Delay", 0.0, 1.0, false);

        ui.add_space(4.0);
        ui.strong("Tape Saturation");
        changed |= self.param_slider(ui, "tape_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_saturation", "Saturation", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_bias", "Bias", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_tone", "Tone", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "tape_speed", "Speed", 0.0, 1.0, false);

        ui.add_space(4.0);
        ui.strong("Neuron Distortion");
        changed |= self.param_slider(ui, "neuron_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_squash", "Squash", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_stab", "Stab", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_asym", "Asymmetry", -1.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_bias", "Bias", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "neuron_comb_freq", "Comb Freq", 20.0, 4000.0, true);
        changed |= self.param_slider(ui, "neuron_comb_sep", "Comb Sep", 0.0, 1.0, false);

        ui.add_space(4.0);
        ui.strong("Delay");
        changed |= self.param_slider(ui, "delay_mix", "Mix", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "delay_time_l", "Time L", 0.01, 2.0, false);
        changed |= self.param_slider(ui, "delay_time_r", "Time R", 0.01, 2.0, false);
        changed |= self.param_slider(ui, "delay_feedback", "Feedback", 0.0, 0.95, false);
        changed |= self.param_slider(ui, "delay_filter", "Filter", 0.0, 0.95, false);
        {
            let mut pp = self.layers[layer].edited_params.get("delay_ping_pong").copied().unwrap_or(0.0) > 0.5;
            if ui.checkbox(&mut pp, "Ping-Pong").changed() {
                self.layers[layer].edited_params.insert("delay_ping_pong".into(), if pp { 1.0 } else { 0.0 });
                changed = true;
            }
        }

        ui.add_space(4.0);
        ui.strong("Reverb");
        ui.horizontal(|ui| {
            let mut rt = self.layers[layer].edited_params.get("reverb_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("reverb_type_{layer}"))
                .selected_text(*REVERB_TYPE_NAMES.get(rt).unwrap_or(&"Plate"))
                .show_ui(ui, |ui| {
                    for (i, name) in REVERB_TYPE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut rt, i, *name).changed() {
                            self.layers[layer].edited_params.insert("reverb_type".into(), rt as f32);
                            changed = true;
                        }
                    }
                });
        });
        let reverb_type = self.layers[layer].edited_params.get("reverb_type").copied().unwrap_or(0.0) as usize;
        if reverb_type == 0 {
            // Plate reverb params
            changed |= self.param_slider(ui, "reverb_mix", "Mix", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_room_size", "Room Size", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_damping", "Damping", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_width", "Width", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "reverb_pre_delay", "Pre-Delay", 0.0, 0.1, false);
        } else {
            // Spring reverb params
            changed |= self.param_slider(ui, "spring_mix", "Mix", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_size", "Size", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_decay", "Decay", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_reflections", "Reflections", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_damping", "Damping", 0.0, 1.0, false);
            changed |= self.param_slider(ui, "spring_spin", "Spin", 0.0, 1.0, false);
        }

        if changed {
            self.layers[layer].params_dirty = true;
            self.send_edited_params(layer);
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if self.layers[layer].params_dirty {
                ui.colored_label(egui::Color32::YELLOW, "Modified");
                ui.separator();
            }
            if ui.button("Save as new preset...").clicked() {
                self.save_as_user_preset();
            }
            if self.layers[layer].params_dirty {
                if ui.button("Reset").clicked() {
                    let l = self.active_layer;
                    self.load_edited_params(l);
                    self.send_edited_params(l);
                }
            }
        });
    }
}
