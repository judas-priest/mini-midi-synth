//! Mod-matrix section of the params editor (split from params.rs, verbatim).

use eframe::egui;

use super::App;
use super::theme;

impl App {
    /// Verbatim frame section (matrix) extracted from draw_params_editable_for.
    pub(super) fn draw_sec_matrix(&mut self, ui: &mut egui::Ui, part: usize) -> bool {
        let mut changed = false;
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
            for slot_idx in 0..MOD_SLOTS {
                let prefix = format!("mod_{slot_idx}_");
                let src_val = self.parts[part].edited_params.get(&format!("{prefix}source")).copied().unwrap_or(0.0);
                let dst_val = self.parts[part].edited_params.get(&format!("{prefix}dest")).copied().unwrap_or(0.0);
                let depth_val = self.parts[part].edited_params.get(&format!("{prefix}depth")).copied().unwrap_or(0.0);
                let src = ModSource::from_param(src_val);
                let dst = ModDest::from_param(dst_val);

                // Only show if this slot or any prior slot is active, plus one empty row
                if src == ModSource::None && dst == ModDest::None && slot_idx > 0 {
                    // Check if previous slot was also empty — stop showing
                    let prev_src = self.parts[part].edited_params.get(&format!("mod_{}_source", slot_idx - 1)).copied().unwrap_or(0.0);
                    let prev_dst = self.parts[part].edited_params.get(&format!("mod_{}_dest", slot_idx - 1)).copied().unwrap_or(0.0);
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
                                    self.push_undo_snapshot_for(part);
                                    self.parts[part].edited_params.insert(format!("{prefix}source"), ModSource::ALL[src_sel] as u8 as f32);
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
                                    self.push_undo_snapshot_for(part);
                                    self.parts[part].edited_params.insert(format!("{prefix}dest"), ModDest::ALL[dst_sel] as u8 as f32);
                                    changed = true;
                                }
                            }
                        });

                    // Depth slider
                    let mut depth = depth_val;
                    if ui.add(egui::Slider::new(&mut depth, -1.0..=1.0).text("Depth").step_by(0.01)).on_hover_text("Modulation depth").changed() {
                        self.push_undo_snapshot_for(part);
                        self.parts[part].edited_params.insert(format!("{prefix}depth"), depth);
                        changed = true;
                    }
                });
            }
        }
        }); // end Mod Matrix frame
        changed
    }
}
