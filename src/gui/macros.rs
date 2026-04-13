/// Macro knobs panel — 8 named user-controllable modulation sources.
use eframe::egui;
use crate::synth::ControlEvent;
use super::App;

impl App {
    /// Draw the macro panel (called from params.rs, near top of synth params).
    pub(super) fn draw_macros(&mut self, ui: &mut egui::Ui) {
        let layer = self.active_layer;

        ui.horizontal(|ui| {
            ui.strong("Macros");
            ui.label(egui::RichText::new("— assign in Mod Matrix as sources Macro 1-8").small().weak());
        });
        ui.add_space(2.0);

        // 8 macros in a 4×2 grid
        egui::Grid::new(format!("macros_{layer}"))
            .num_columns(4)
            .spacing([8.0, 4.0])
            .show(ui, |ui| {
                for i in 0..8 {
                    let val = self.layers[layer].macro_vals[i];
                    let name = self.layers[layer].macro_names.get(i)
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
                            if let Some(n) = self.layers[layer].macro_names.get_mut(i) {
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
                        ).changed() {
                            self.layers[layer].macro_vals[i] = v;
                            // Update edited_params so it's saved in preset
                            self.layers[layer].edited_params.insert(
                                format!("macro_{i}"), v
                            );
                            // Send to audio engine immediately (real-time)
                            let _ = self.ctrl_tx.push(ControlEvent::SetMacro {
                                layer, index: i, value: v,
                            });
                        }
                    });

                    if i % 4 == 3 {
                        ui.end_row();
                    }
                }
            });
    }
}
