//! Filter section of the params editor (split from params.rs, verbatim).

use eframe::egui;

use super::App;
use super::theme;
use super::{
    FILTER_NAMES, FILTER_ROUTING_NAMES, FORMANT_VOICE_NAMES, FORMANT_VOWEL_NAMES,
    WAVE_SHAPER_MODE_NAMES,
};

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

impl App {
    /// Verbatim frame section (filter) extracted from draw_params_editable_for.
    pub(super) fn draw_sec_filter(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
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
                            self.push_undo_snapshot_for(part);
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
                                self.push_undo_snapshot_for(part);
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
                                self.push_undo_snapshot_for(part);
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
                                self.push_undo_snapshot_for(part);
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
                                    self.push_undo_snapshot_for(part);
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
                                        self.push_undo_snapshot_for(part);
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
        changed
    }
}
