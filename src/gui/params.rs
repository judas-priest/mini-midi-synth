//! Synth parameter editor UI.

use eframe::egui;

use super::App;
use super::theme;
use super::{
    OSC_NAMES, SIMPLE_OSC_NAMES, FILTER_NAMES, FILTER_ROUTING_NAMES,
    PD_SHAPE_NAMES, FOLD_SOURCE_NAMES, MODAL_MATERIAL_NAMES, SYNC_SHAPE_NAMES,
    BASS_STYLE_NAMES, BASS_PICKUP_NAMES, BOWED_BODY_NAMES, BRASS_BELL_NAMES,
    ACCORDION_REGISTER_NAMES, SAX_TYPE_NAMES, ALIAS_WAVE_NAMES, WINDOW_TYPE_NAMES,
    ENV_SHAPE_NAMES, FORMANT_VOICE_NAMES, FORMANT_VOWEL_NAMES, LFO_WAVEFORM_NAMES,
    VELOCITY_CURVE_NAMES, PORTAMENTO_MODE_NAMES, PLAY_MODE_NAMES, REVERB_TYPE_NAMES, RING_MOD_SHAPE_NAMES,
    WAVE_SHAPER_MODE_NAMES, LAYER_NAMES,
};

const LFO_TRIGGER_MODE_NAMES: &[&str] = &["Free Run", "Key Trigger", "Random Start", "Random Unipolar"];

/// Number of frequency points for the filter response curve.
const FILTER_RESP_POINTS: usize = 64;

/// Map a filter_type parameter value to a visualization category:
/// 0 = LP, 1 = HP, 2 = BP, 3 = Notch, 4 = Other (flat).
fn filter_vis_category(filter_type: u32) -> u8 {
    match filter_type {
        0 | 4 | 5 | 6 | 12 | 14 | 18 | 22 | 23 | 25 | 30 | 35 | 37 => 0, // LP variants
        1 | 13 | 15 | 19 | 26 | 31 => 1,                                    // HP variants
        2 | 16 | 20 | 27 | 32 | 38 => 2,                                    // BP variants
        11 | 17 | 21 | 28 | 33 => 3,                                        // Notch variants
        36 => 0,                                                             // SVFMorph (default LP)
        _ => 4,                                                              // Comb/Allpass/S&H/AP warp → flat
    }
}

/// Compute filter magnitude response (dB) at `FILTER_RESP_POINTS` log-spaced frequencies.
fn compute_filter_response(cutoff: f32, resonance: f32, filter_type: u32) -> [(f32, f32); FILTER_RESP_POINTS] {
    let mut points = [(0.0f32, 0.0f32); FILTER_RESP_POINTS];
    let cat = filter_vis_category(filter_type);
    // Map resonance 0..1 → Q (higher resonance = sharper peak).
    let q = (1.0 - resonance * 0.95).max(0.05).recip();
    // For 24dB (4-pole) types, cascade two 2-pole stages.
    let is_4pole = matches!(filter_type, 4 | 12 | 13 | 16 | 17 | 22);
    // 18dB (3-pole) types get 1.5x the slope.
    let is_3pole = matches!(filter_type, 6 | 23);

    for (i, point) in points.iter_mut().enumerate() {
        let t = i as f32 / (FILTER_RESP_POINTS - 1) as f32;
        let freq = 20.0 * (20000.0f32 / 20.0).powf(t);
        let ratio = freq / cutoff.max(1.0);
        let r2 = ratio * ratio;
        let denom = (1.0 - r2).powi(2) + (ratio / q).powi(2);

        let mag_sq = match cat {
            0 => 1.0 / denom,                              // LP
            1 => r2 * r2 / denom,                          // HP
            2 => (ratio / q).powi(2) / denom,              // BP
            3 => (1.0 - r2).powi(2) / denom,               // Notch
            _ => 1.0,                                       // flat
        };

        let mut db = 10.0 * mag_sq.max(1e-10).log10();
        if is_4pole { db *= 2.0; }
        if is_3pole { db *= 1.5; }
        *point = (freq, db.clamp(-24.0, 12.0));
    }
    points
}

/// Map frequency (Hz) to x-coordinate within `rect` (log scale 20–20 kHz).
fn freq_to_x(freq: f32, rect: egui::Rect) -> f32 {
    let t = (freq / 20.0).log10() / (20000.0f32 / 20.0).log10();
    rect.left() + t * rect.width()
}

/// Map dB to y-coordinate within `rect` (−24 dB at bottom, +12 dB at top).
fn db_to_y(db: f32, rect: egui::Rect) -> f32 {
    let t = (db - (-24.0)) / (12.0 - (-24.0));
    rect.bottom() - t * rect.height()
}

/// Draw the filter frequency response curve.
fn draw_filter_response(
    ui: &mut egui::Ui,
    cutoff: f32,
    resonance: f32,
    filter_type: u32,
    _sample_rate: f32,
) {
    let width = ui.available_width().min(300.0);
    let height = 80.0;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    // Background
    painter.rect_filled(rect, 2.0, theme::BG_TIMELINE);

    // Grid lines at 100 Hz, 1 kHz, 10 kHz
    for &freq in &[100.0f32, 1000.0, 10000.0] {
        let x = freq_to_x(freq, rect);
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            egui::Stroke::new(1.0, theme::STROKE_GRID_DARK),
        );
    }

    // 0 dB line
    let y_0db = db_to_y(0.0, rect);
    painter.line_segment(
        [egui::pos2(rect.left(), y_0db), egui::pos2(rect.right(), y_0db)],
        egui::Stroke::new(1.0, theme::STROKE_GUIDE),
    );

    // Compute response curve
    let points = compute_filter_response(cutoff, resonance, filter_type);

    // Draw curve
    let color = theme::CURVE_FILTER;
    for i in 1..FILTER_RESP_POINTS {
        let x0 = freq_to_x(points[i - 1].0, rect);
        let y0 = db_to_y(points[i - 1].1, rect);
        let x1 = freq_to_x(points[i].0, rect);
        let y1 = db_to_y(points[i].1, rect);
        painter.line_segment(
            [egui::pos2(x0, y0), egui::pos2(x1, y1)],
            egui::Stroke::new(1.5, color),
        );
    }

    // Cutoff marker
    let cx = freq_to_x(cutoff.clamp(20.0, 20000.0), rect);
    painter.line_segment(
        [egui::pos2(cx, rect.top()), egui::pos2(cx, rect.bottom())],
        egui::Stroke::new(1.0, theme::CURVE_CUTOFF_MARKER),
    );
}

fn env_shape_combo(
    ui: &mut egui::Ui,
    part: usize,
    label: &str,
    param_key: &str,
    id_salt: &str,
    params: &mut std::collections::BTreeMap<String, f32>,
) -> bool {
    let mut val = params.get(param_key).copied().unwrap_or(0.0) as usize;
    let mut changed = false;
    ui.label(label);
    egui::ComboBox::from_id_salt(format!("{id_salt}_{part}"))
        .selected_text(*ENV_SHAPE_NAMES.get(val).unwrap_or(&"Sqrt"))
        .width(90.0)
        .show_ui(ui, |ui| {
            for (i, name) in ENV_SHAPE_NAMES.iter().enumerate() {
                if ui.selectable_value(&mut val, i, *name).changed() {
                    params.insert(param_key.into(), val as f32);
                    changed = true;
                }
            }
        });
    changed
}

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
    /// Draw a small ADSR envelope curve from parameter values.
    fn draw_adsr_curve(
        ui: &mut egui::Ui,
        _layer: usize,
        params: &std::collections::BTreeMap<String, f32>,
        prefix: &str,
    ) {
        let a = params.get(&format!("{prefix}_attack")).copied().unwrap_or(0.01);
        let d = params.get(&format!("{prefix}_decay")).copied().unwrap_or(0.1);
        let s = params.get(&format!("{prefix}_sustain")).copied().unwrap_or(0.7);
        let r = params.get(&format!("{prefix}_release")).copied().unwrap_or(0.3);

        let w = ui.available_width().min(280.0);
        let h = 32.0_f32;
        let (resp, painter) = ui.allocate_painter(egui::vec2(w, h), egui::Sense::hover());
        let rect = resp.rect;
        painter.rect_filled(rect, 2.0, theme::BG_WIDGET_ALT);

        // Normalize time segments to fit width (sustain gets fixed portion)
        let total_time = a + d + r + 0.001;
        let sustain_w = w * 0.15; // fixed sustain hold width
        let env_w = w - sustain_w;
        let a_w = (a / total_time * env_w).max(2.0);
        let d_w = (d / total_time * env_w).max(2.0);
        let r_w = (r / total_time * env_w).max(2.0);
        // Scale to fit
        let scale = env_w / (a_w + d_w + r_w);
        let a_w = a_w * scale;
        let d_w = d_w * scale;
        let r_w = r_w * scale;

        let bot = rect.bottom() - 2.0;
        let top = rect.top() + 2.0;
        let range = bot - top;

        let p0 = egui::pos2(rect.left() + 1.0, bot);               // start
        let p1 = egui::pos2(rect.left() + 1.0 + a_w, top);         // peak
        let p2 = egui::pos2(p1.x + d_w, bot - s * range);          // sustain level
        let p3 = egui::pos2(p2.x + sustain_w, bot - s * range);    // sustain hold end
        let p4 = egui::pos2(p3.x + r_w, bot);                      // release end

        let color = if prefix == "amp" {
            theme::CURVE_AMP_ENV
        } else {
            theme::CURVE_MOD_ENV
        };
        painter.add(egui::Shape::line(
            vec![p0, p1, p2, p3, p4],
            egui::Stroke::new(1.5, color),
        ));
        // Sustain level dashed line
        let sy = bot - s * range;
        painter.line_segment(
            [egui::pos2(rect.left(), sy), egui::pos2(rect.right(), sy)],
            egui::Stroke::new(0.5, theme::STROKE_SUBTLE),
        );
    }

    pub(super) fn draw_params_editable(&mut self, ui: &mut egui::Ui) {
        let part = self.active_part;
        let patch_idx = self.parts[part].patch_idx;
        let preset_name = self
            .patches
            .get(patch_idx)
            .map(|p| p.name.as_str())
            .unwrap_or("(none)");

        let layer_label = LAYER_NAMES.get(part).unwrap_or(&"?");
        ui.strong(format!("{preset_name} (Layer {layer_label})"));
        ui.add_space(theme::SP_MD);

        let mut changed = false;

        // Macro knobs (always visible at top — key performance controls)
        self.draw_macros(ui);
        ui.add_space(theme::SP_SM);

        // Oscillator section
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
                            self.parts[part].edited_params.insert("osc_type".into(), osc as f32);
                            changed = true;
                        }
                    }
                });

            let mut detune = self.parts[part].edited_params.get("osc_detune").copied().unwrap_or(0.0);
            ui.label("Detune:");
            if ui.add(egui::Slider::new(&mut detune, 0.0..=0.05).step_by(0.001)).on_hover_text("Oscillator pitch detune").changed() {
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
                if ui.add(egui::Slider::new(&mut val, 0.0..=8.0).step_by(1.0).text(*name)).on_hover_text("Organ drawbar level").changed() {
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
                if ui.add(egui::Slider::new(&mut pw_val, 0.05..=0.95)).on_hover_text("Square wave duty cycle").changed() {
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
                                self.parts[part].edited_params.insert("pd_shape".into(), shape as f32);
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
                if ui.add(egui::Slider::new(&mut osc_count, 1..=3)).on_hover_text("Number of oscillators").changed() {
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
                                    self.parts[part].edited_params.insert("osc2_type".into(), osc2 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune2 = self.parts[part].edited_params.get("osc2_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    if ui.add(egui::Slider::new(&mut detune2, -0.05..=0.05).step_by(0.001)).on_hover_text("Osc 2 pitch detune").changed() {
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
                                    self.parts[part].edited_params.insert("osc3_type".into(), osc3 as f32);
                                    changed = true;
                                }
                            }
                        });

                    let mut detune3 = self.parts[part].edited_params.get("osc3_detune").copied().unwrap_or(0.0);
                    ui.label("Detune:");
                    if ui.add(egui::Slider::new(&mut detune3, -0.05..=0.05).step_by(0.001)).on_hover_text("Osc 3 pitch detune").changed() {
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
                            self.parts[part].edited_params.insert("osc_ws_mode".into(), wsm as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "osc_ws_drive", "Drive", 0.0, 1.0, false);
        changed |= self.param_slider(ui, "osc_ws_mix", "Mix", 0.0, 1.0, false);
        }); // end Oscillator frame
        ui.add_space(theme::SP_SM);

        // Filter section
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Primary width for filter — main sound-shaping params
        ui.style_mut().spacing.slider_width = theme::SLIDER_PRIMARY;

        // Filter 1
        ui.strong("Filter 1");
        ui.horizontal(|ui| {
            let mut ft = self.parts[part].edited_params.get("filter_type").copied().unwrap_or(0.0) as usize;
            ui.label("Type:");
            egui::ComboBox::from_id_salt(format!("filter_type_{part}"))
                .selected_text(*FILTER_NAMES.get(ft).unwrap_or(&"?"))
                .show_ui(ui, |ui| {
                    for (i, name) in FILTER_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut ft, i, *name).changed() {
                            self.parts[part].edited_params.insert("filter_type".into(), ft as f32);
                            changed = true;
                        }
                    }
                });
        });

        let filter_type = self.parts[part].edited_params.get("filter_type").copied().unwrap_or(0.0) as u32;
        let is_formant = filter_type == 3;

        if is_formant {
            // Formant filter controls
            ui.horizontal(|ui| {
                let mut fv = self.parts[part].edited_params.get("formant_voice").copied().unwrap_or(0.0) as usize;
                ui.label("Voice:");
                egui::ComboBox::from_id_salt(format!("formant_voice_{part}"))
                    .selected_text(*FORMANT_VOICE_NAMES.get(fv).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FORMANT_VOICE_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut fv, i, *name).changed() {
                                self.parts[part].edited_params.insert("formant_voice".into(), fv as f32);
                                changed = true;
                            }
                        }
                    });

                let mut vw = self.parts[part].edited_params.get("formant_vowel").copied().unwrap_or(0.0) as usize;
                ui.label("Vowel:");
                egui::ComboBox::from_id_salt(format!("formant_vowel_{part}"))
                    .selected_text(*FORMANT_VOWEL_NAMES.get(vw).unwrap_or(&"?"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FORMANT_VOWEL_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut vw, i, *name).changed() {
                                self.parts[part].edited_params.insert("formant_vowel".into(), vw as f32);
                                changed = true;
                            }
                        }
                    });
            });
        } else {
            changed |= self.param_slider_ex(ui, "filter_cutoff", "Cutoff", 20.0, 20000.0, true, " Hz");
            changed |= self.param_slider(ui, "filter_resonance", "Resonance", 0.0, 1.0, false);
            changed |= self.param_slider_ex(ui, "filter_env_amount", "Env Amount", 0.0, 15000.0, false, " Hz");
            changed |= self.param_slider(ui, "filter_key_track", "Key Track", 0.0, 1.0, false);
            // SVF Morph slider: only shown when filter type is SVFMorph (type 36)
            if filter_type == 36 {
                changed |= self.param_slider(ui, "svf_morph", "LP\u{2194}HP", 0.0, 1.0, false);
            }
            // Polivoks sliders: only shown when filter type is PolivoksLP (37) or PolivoksBP (38)
            if filter_type == 37 || filter_type == 38 {
                changed |= self.param_slider(ui, "filter_drive", "Drive", 0.0, 1.0, false);
                changed |= self.param_slider(ui, "filter_starve", "Starve", 0.0, 1.0, false);
            }

            // Filter frequency response curve
            let cutoff = self.parts[part].edited_params.get("filter_cutoff").copied().unwrap_or(1000.0);
            let resonance = self.parts[part].edited_params.get("filter_resonance").copied().unwrap_or(0.0);
            draw_filter_response(ui, cutoff, resonance, filter_type, self.sample_rate as f32);
        }

        // Filter routing (not available with formant filter)
        if !is_formant {
            ui.add_space(theme::SP_SM);
            let mut routing = self.parts[part].edited_params.get("filter_routing").copied().unwrap_or(0.0) as usize;
            ui.horizontal(|ui| {
                ui.label("Routing:");
                egui::ComboBox::from_id_salt(format!("filter_routing_{part}"))
                    .selected_text(*FILTER_ROUTING_NAMES.get(routing).unwrap_or(&"Single"))
                    .show_ui(ui, |ui| {
                        for (i, name) in FILTER_ROUTING_NAMES.iter().enumerate() {
                            if ui.selectable_value(&mut routing, i, *name).changed() {
                                self.parts[part].edited_params.insert("filter_routing".into(), routing as f32);
                                changed = true;
                            }
                        }
                    });
            });

            // Filter 2 controls (only when routing != Single)
            if routing >= 1 {
                ui.add_space(theme::SP_SM);
                ui.strong("Filter 2");
                ui.horizontal(|ui| {
                    let ft2 = self.parts[part].edited_params.get("filter2_type").copied().unwrap_or(0.0) as usize;
                    ui.label("Type:");
                    // Filter 2: LP/HP/BP + Moog (skip Formant index 3)
                    let f2_names: &[&str] = &["LowPass", "HighPass", "BandPass", "Moog 24dB", "Moog 12dB", "Diode 18dB", "Comb", "Allpass", "Comb+", "Comb-"];
                    let f2_values: &[usize] = &[0, 1, 2, 4, 5, 6, 7, 8, 9, 10]; // maps to FilterType param values
                    let f2_display = f2_values.iter().position(|&v| v == ft2).unwrap_or(0);
                    let mut f2_sel = f2_display;
                    egui::ComboBox::from_id_salt(format!("filter2_type_{part}"))
                        .selected_text(*f2_names.get(f2_sel).unwrap_or(&"LowPass"))
                        .show_ui(ui, |ui| {
                            for (i, name) in f2_names.iter().enumerate() {
                                if ui.selectable_value(&mut f2_sel, i, *name).changed() {
                                    let param_val = f2_values[f2_sel];
                                    self.parts[part].edited_params.insert("filter2_type".into(), param_val as f32);
                                    changed = true;
                                }
                            }
                        });
                });
                changed |= self.param_slider_ex(ui, "filter2_cutoff", "Cutoff 2", 20.0, 20000.0, true, " Hz");
                changed |= self.param_slider(ui, "filter2_resonance", "Resonance 2", 0.0, 1.0, false);

                // Inter-filter waveshaper (Serial routing only)
                if routing == 1 {
                    ui.add_space(theme::SP_SM);
                    ui.strong("Inter-Filter Shaper");
                    ui.horizontal(|ui| {
                        let mut wsm = self.parts[part].edited_params.get("inter_ws_mode").copied().unwrap_or(0.0) as usize;
                        ui.label("Mode:");
                        egui::ComboBox::from_id_salt(format!("inter_ws_mode_{part}"))
                            .selected_text(*WAVE_SHAPER_MODE_NAMES.get(wsm).unwrap_or(&"?"))
                            .show_ui(ui, |ui| {
                                for (i, name) in WAVE_SHAPER_MODE_NAMES.iter().enumerate() {
                                    if ui.selectable_value(&mut wsm, i, *name).changed() {
                                        self.parts[part].edited_params.insert("inter_ws_mode".into(), wsm as f32);
                                        changed = true;
                                    }
                                }
                            });
                    });
                    changed |= self.param_slider(ui, "inter_ws_drive", "Drive", 0.0, 1.0, false);
                    changed |= self.param_slider(ui, "inter_ws_mix", "Mix", 0.0, 1.0, false);
                }
            }
        }
        }); // end Filter frame
        ui.add_space(theme::SP_SM);

        // Envelopes section
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Primary width for envelopes — main sound-shaping params
        ui.style_mut().spacing.slider_width = theme::SLIDER_PRIMARY;

        // Amp Envelope (AHDSR)
        ui.strong("Amp Envelope");
        changed |= self.param_slider_ex(ui, "amp_attack",  "Attack",  0.001, 5.0, true, " s");
        changed |= self.param_slider_ex(ui, "amp_hold",    "Hold",    0.0,   5.0, false, " s");
        changed |= self.param_slider_ex(ui, "amp_decay",   "Decay",   0.0,   5.0, false, " s");
        changed |= self.param_slider(ui, "amp_sustain", "Sustain", 0.0,   1.0, false);
        changed |= self.param_slider_ex(ui, "amp_release", "Release", 0.001, 5.0, true, " s");
        Self::draw_adsr_curve(ui, part, &self.parts[part].edited_params, "amp");

        ui.add_space(theme::SP_MD);

        // Filter Envelope (AHDSR)
        ui.strong("Filter Envelope");
        changed |= self.param_slider_ex(ui, "filter_attack",  "Attack",  0.001, 5.0, true, " s");
        changed |= self.param_slider_ex(ui, "filter_hold",    "Hold",    0.0,   5.0, false, " s");
        changed |= self.param_slider_ex(ui, "filter_decay",   "Decay",   0.0,   5.0, false, " s");
        changed |= self.param_slider(ui, "filter_sustain", "Sustain", 0.0,   1.0, false);
        changed |= self.param_slider_ex(ui, "filter_release", "Release", 0.001, 5.0, true, " s");
        Self::draw_adsr_curve(ui, part, &self.parts[part].edited_params, "filter");

        // Envelope shapes
        ui.horizontal(|ui| {
            changed |= env_shape_combo(ui, part, "Atk Shape:", "env_attack_shape", "env_atk_shape", &mut self.parts[part].edited_params);
            changed |= env_shape_combo(ui, part, "Dec:", "env_decay_shape", "env_dec_shape", &mut self.parts[part].edited_params);
            changed |= env_shape_combo(ui, part, "Rel:", "env_release_shape", "env_rel_shape", &mut self.parts[part].edited_params);
        });
        // Filter envelope shapes
        ui.horizontal(|ui| {
            changed |= env_shape_combo(ui, part, "Flt Atk:", "filter_env_attack_shape", "fenv_atk_shape", &mut self.parts[part].edited_params);
            changed |= env_shape_combo(ui, part, "Dec:", "filter_env_decay_shape", "fenv_dec_shape", &mut self.parts[part].edited_params);
            changed |= env_shape_combo(ui, part, "Rel:", "filter_env_release_shape", "fenv_rel_shape", &mut self.parts[part].edited_params);
        });
        }); // end Envelopes frame
        ui.add_space(theme::SP_SM);

        // Dynamics section
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
                            self.parts[part].edited_params.insert("velocity_curve".into(), vc as f32);
                            changed = true;
                        }
                    }
                });
        });
        changed |= self.param_slider(ui, "vel_to_filter", "Vel->Filter", 0.0, 1.0, false);
        }); // end Dynamics frame
        ui.add_space(theme::SP_SM);

        // LFO section
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
                    self.parts[part].edited_params.insert("slfo1_unipolar".into(), if uni { 1.0 } else { 0.0 });
                    changed = true;
                }
                if ui.checkbox(&mut tsync, "Tempo Sync").changed() {
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
                    self.parts[part].edited_params.insert("slfo2_unipolar".into(), if uni { 1.0 } else { 0.0 });
                    changed = true;
                }
                if ui.checkbox(&mut tsync, "Tempo Sync").changed() {
                    self.parts[part].edited_params.insert("slfo2_tempo_sync".into(), if tsync { 1.0 } else { 0.0 });
                    changed = true;
                }
            });
        }
        }); // end LFO frame
        ui.add_space(theme::SP_SM);

        // Mod Matrix section
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        ui.style_mut().spacing.slider_width = theme::SLIDER_SECONDARY;

        // Modulation Matrix
        ui.strong("Mod Matrix");
        {
            use crate::synth::mod_matrix::{ModSource, ModDest, MOD_SLOTS};
            let ep = &mut self.parts[part].edited_params;
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
                    egui::ComboBox::from_id_salt(format!("mod_src_{part}_{slot_idx}"))
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
                    egui::ComboBox::from_id_salt(format!("mod_dst_{part}_{slot_idx}"))
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
                    if ui.add(egui::Slider::new(&mut depth, -1.0..=1.0).text("Depth").step_by(0.01)).on_hover_text("Modulation depth").changed() {
                        ep.insert(format!("{prefix}depth"), depth);
                        changed = true;
                    }
                });
            }
        }
        }); // end Mod Matrix frame
        ui.add_space(theme::SP_SM);

        // Voice section (Play Mode + Portamento + Unison)
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
            if ui.add(egui::Slider::new(&mut uv, 1..=8).text("Voices")).on_hover_text("Unison voice count").changed() {
                self.parts[part].edited_params.insert("unison_voices".into(), uv as f32);
                changed = true;
            }
        }
        changed |= self.param_slider_ex(ui, "unison_detune", "Detune", 0.0, 50.0, false, " ct");
        changed |= self.param_slider(ui, "unison_spread", "Spread", 0.0, 1.0, false);
        }); // end Voice frame
        ui.add_space(theme::SP_SM);

        // Arpeggiator section
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

                let mut amode = self.parts[part].edited_params.get("arp_mode").copied().unwrap_or(0.0) as usize;
                ui.label("Mode:");
                egui::ComboBox::from_id_salt(format!("arp_mode_{part}"))
                    .selected_text(*arp_mode_names.get(amode).unwrap_or(&"Up"))
                    .width(70.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in arp_mode_names.iter().enumerate() {
                            if ui.selectable_value(&mut amode, i, *name).changed() {
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
                                self.parts[part].edited_params.insert("arp_rate".into(), arate as f32);
                                changed = true;
                            }
                        }
                    });
            });

            ui.horizontal(|ui| {
                let mut oct = self.parts[part].edited_params.get("arp_octaves").copied().unwrap_or(1.0) as u32;
                if ui.add(egui::Slider::new(&mut oct, 1..=4).text("Octaves")).on_hover_text("Arpeggiator octave range").changed() {
                    self.parts[part].edited_params.insert("arp_octaves".into(), oct as f32);
                    changed = true;
                }
            });
            changed |= self.param_slider(ui, "arp_gate", "Gate", 0.1, 1.0, false);
        }
        }); // end Arpeggiator frame
        ui.add_space(theme::SP_SM);

        // Effects section
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
                            self.parts[part].edited_params.insert("airwindows_mode".into(), i as f32);
                            changed = true;
                        }
                    }
                });
        });
        }); // end Effects frame
        ui.add_space(theme::SP_SM);

        // Pitch Bend section
        egui::Frame::default()
            .fill(theme::BG_SECTION)
            .corner_radius(4.0)
            .inner_margin(egui::Margin::same(6))
            .show(ui, |ui| {
        // Compact width for pitch bend utility
        ui.style_mut().spacing.slider_width = theme::SLIDER_COMPACT;

        // Asymmetric pitch bend
        ui.strong("Pitch Bend");
        changed |= self.param_slider_ex(ui, "pitch_bend_up",   "Range Up",   0.0, 48.0, false, " st");
        changed |= self.param_slider_ex(ui, "pitch_bend_down", "Range Down", 0.0, 48.0, false, " st");
        }); // end Pitch Bend frame

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
                let l = self.active_part;
                self.load_edited_params(l);
                self.send_edited_params(l);
            }
        });
    }
}
