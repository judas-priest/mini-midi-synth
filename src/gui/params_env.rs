//! Envelope section of the params editor (split from params.rs, verbatim).

use eframe::egui;

use super::App;
use super::theme;
use super::ENV_SHAPE_NAMES;

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

    /// Verbatim frame section (env) extracted from draw_params_editable_for.
    pub(super) fn draw_sec_env(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
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
        changed
    }
}
