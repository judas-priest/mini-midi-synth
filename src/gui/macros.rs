//! Macro knobs panel — 8 named user-controllable modulation sources.
use eframe::egui;
use crate::synth::ControlEvent;
use super::App;
use super::theme;

impl App {
    /// Draw the macro panel (called from params.rs, near top of synth params).
    pub(super) fn draw_macros(&mut self, ui: &mut egui::Ui) {
        let part = self.active_part;

        ui.horizontal(|ui| {
            ui.strong("Macros");
            ui.label(egui::RichText::new("— assign in Mod Matrix as sources Macro 1-8").small().weak());
        });
        ui.add_space(2.0);

        // 8 macros in a 4×2 grid
        egui::Grid::new(format!("macros_{part}"))
            .num_columns(4)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for i in 0..8 {
                    let val = self.parts[part].macro_vals[i];
                    let name = self.parts[part].macro_names.get(i)
                        .cloned()
                        .unwrap_or_else(|| format!("Macro {}", i + 1));

                    ui.vertical(|ui| {
                        // Editable name
                        let mut name_edit = name.clone();
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut name_edit)
                                .desired_width(80.0)
                                .font(egui::TextStyle::Small)
                                .hint_text(format!("Macro {}", i + 1))
                        );
                        if resp.changed() {
                            if let Some(n) = self.parts[part].macro_names.get_mut(i) {
                                *n = name_edit.clone();
                            }
                            // Persist name into edited_params as a special float key
                            // (encode first 4 chars as f32 — not ideal but avoids string params)
                            // Instead, names stay GUI-side only (saved in Performance)
                        }

                        // Value slider (vertical feel via horizontal progress)
                        let mut v = val;
                        if ui.add(
                            egui::Slider::new(&mut v, 0.0..=1.0)
                                .show_value(true)
                                .text("")
                        ).on_hover_text("Macro modulation value").changed() {
                            self.parts[part].macro_vals[i] = v;
                            // Update edited_params so it's saved in patch
                            self.parts[part].edited_params.insert(
                                format!("macro_{i}"), v
                            );
                            // Send to audio engine immediately (real-time)
                            let _ = self.ctrl_tx.push(ControlEvent::SetMacro {
                                part, index: i, value: v,
                            });
                        }
                    });

                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });

        ui.add_space(8.0);
        self.draw_xy_pad(ui);
    }

    /// XY Pad widget: drag to control two macros simultaneously.
    fn draw_xy_pad(&mut self, ui: &mut egui::Ui) {
        let part = self.active_part;
        let macro_labels: Vec<String> = (0..8)
            .map(|i| {
                self.parts[part].macro_names.get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("Macro {}", i + 1))
            })
            .collect();

        // Axis assignment combo boxes
        ui.horizontal(|ui| {
            ui.strong("XY Pad");
            ui.add_space(12.0);

            ui.label("X:");
            egui::ComboBox::from_id_salt(format!("xy_x_{part}"))
                .selected_text(macro_labels[self.parts[part].xy_macro_x].as_str())
                .width(90.0)
                .show_ui(ui, |ui| {
                    for (i, label) in macro_labels.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.parts[part].xy_macro_x,
                            i,
                            label,
                        );
                    }
                });

            ui.add_space(8.0);
            ui.label("Y:");
            egui::ComboBox::from_id_salt(format!("xy_y_{part}"))
                .selected_text(macro_labels[self.parts[part].xy_macro_y].as_str())
                .width(90.0)
                .show_ui(ui, |ui| {
                    for (i, label) in macro_labels.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.parts[part].xy_macro_y,
                            i,
                            label,
                        );
                    }
                });
        });

        ui.add_space(4.0);

        let xi = self.parts[part].xy_macro_x;
        let yi = self.parts[part].xy_macro_y;
        if xi == yi {
            ui.colored_label(egui::Color32::YELLOW, egui::RichText::new("X and Y use the same macro").small());
        }
        let x_name = &macro_labels[xi];
        let y_name = &macro_labels[yi];

        // Pad area
        let size = egui::vec2(200.0, 200.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        response.clone().on_hover_text("Drag to control two macros");
        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 4.0, theme::BG_PANEL);
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, theme::STROKE_PANEL_BORDER), egui::StrokeKind::Outside);

        // Grid lines (4×4 subdivisions)
        for i in 1..4 {
            let frac = i as f32 / 4.0;
            let x = rect.left() + frac * rect.width();
            let y = rect.top() + frac * rect.height();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(0.5, theme::STROKE_GRID),
            );
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(0.5, theme::STROKE_GRID),
            );
        }

        // Current position
        let x_val = self.parts[part].macro_vals[xi];
        let y_val = self.parts[part].macro_vals[yi];
        let x_frac = x_val;
        let y_frac = 1.0 - y_val; // inverted so top = 1
        let dot_pos = egui::pos2(
            rect.left() + x_frac * rect.width(),
            rect.top() + y_frac * rect.height(),
        );

        // Crosshair lines through dot
        painter.line_segment(
            [egui::pos2(dot_pos.x, rect.top()), egui::pos2(dot_pos.x, rect.bottom())],
            egui::Stroke::new(0.5, theme::XY_CROSSHAIR),
        );
        painter.line_segment(
            [egui::pos2(rect.left(), dot_pos.y), egui::pos2(rect.right(), dot_pos.y)],
            egui::Stroke::new(0.5, theme::XY_CROSSHAIR),
        );

        // Dot
        painter.circle_filled(dot_pos, 7.0, theme::CURVE_FILTER);
        painter.circle_stroke(dot_pos, 7.0, egui::Stroke::new(1.0, egui::Color32::WHITE));

        // Axis labels
        painter.text(
            egui::pos2(rect.center().x, rect.bottom() + 10.0),
            egui::Align2::CENTER_TOP,
            x_name,
            egui::FontId::proportional(11.0),
            theme::TEXT_OVERLAY,
        );
        // Y label (rotated text not trivial in egui, place to the left)
        painter.text(
            egui::pos2(rect.left() - 4.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            y_name,
            egui::FontId::proportional(11.0),
            theme::TEXT_OVERLAY,
        );

        // Handle drag / click
        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                let new_x = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                let new_y = 1.0 - ((pos.y - rect.top()) / rect.height()).clamp(0.0, 1.0);

                // Update X macro
                self.parts[part].macro_vals[xi] = new_x;
                self.parts[part].edited_params.insert(format!("macro_{xi}"), new_x);
                let _ = self.ctrl_tx.push(ControlEvent::SetMacro {
                    part, index: xi, value: new_x,
                });

                // Update Y macro
                self.parts[part].macro_vals[yi] = new_y;
                self.parts[part].edited_params.insert(format!("macro_{yi}"), new_y);
                let _ = self.ctrl_tx.push(ControlEvent::SetMacro {
                    part, index: yi, value: new_y,
                });
            }
        }
    }
}
