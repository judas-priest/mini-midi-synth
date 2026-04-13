/// FX Chain GUI panel — 16 configurable slots.
use eframe::egui;
use super::App;
use crate::synth::fx_chain::{FxChain, FxSlot, FxSlotType, FX_SLOTS};

impl App {
    pub(super) fn draw_fx_chain(&mut self, ui: &mut egui::Ui) {
        let layer = self.active_layer;

        ui.horizontal(|ui| {
            ui.strong("FX Chain");
            ui.label("— 16 слотов, последовательная обработка");
            ui.separator();
            if ui.small_button("Сбросить").clicked() {
                let mut chain = FxChain::default();
                chain.active = true;
                flush_fx_chain(&chain, layer, &mut self.layers[layer].edited_params);
                self.send_edited_params(layer);
            }
            if ui.small_button("Из пресета").clicked() {
                let chain = FxChain::from_legacy_preset_active(
                    &self.layers[layer].edited_params
                );
                flush_fx_chain(&chain, layer, &mut self.layers[layer].edited_params);
                self.send_edited_params(layer);
            }
        });
        ui.add_space(4.0);

        // Read current chain
        let mut chain = if self.layers[layer].edited_params.contains_key("fx0_type") {
            FxChain::from_map(&self.layers[layer].edited_params)
        } else {
            let mut c = FxChain::default();
            c.active = true;
            c
        };

        let mut swap: Option<(usize, usize)> = None;
        let mut changed = false;

        egui::Grid::new(format!("fx_chain_{layer}"))
            .num_columns(1)
            .striped(true)
            .spacing([4.0, 2.0])
            .show(ui, |ui| {
                for i in 0..FX_SLOTS {
                    changed |= draw_slot(ui, layer, i, &mut chain.slots[i]);

                    // Swap buttons (outside closure so we can act on the chain)
                    ui.horizontal(|ui| {
                        if i > 0 && ui.small_button("↑").clicked() {
                            swap = Some((i - 1, i));
                        }
                        if i + 1 < FX_SLOTS && ui.small_button("↓").clicked() {
                            swap = Some((i, i + 1));
                        }
                    });

                    ui.end_row();
                }
            });

        if let Some((a, b)) = swap {
            chain.slots.swap(a, b);
            changed = true;
        }

        if changed {
            chain.active = true;
            flush_fx_chain(&chain, layer, &mut self.layers[layer].edited_params);
            self.send_edited_params(layer);
        }
    }
}

fn flush_fx_chain(
    chain: &FxChain,
    _layer: usize,
    params: &mut std::collections::BTreeMap<String, f32>,
) {
    chain.to_map(params);
}

fn draw_slot(ui: &mut egui::Ui, layer: usize, idx: usize, slot: &mut FxSlot) -> bool {
    let mut changed = false;

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{:2}.", idx + 1)).monospace());

        // Enable
        let mut en = slot.enabled;
        if ui.checkbox(&mut en, "").changed() {
            slot.enabled = en;
            changed = true;
        }

        // Type selector
        let cur = FxSlotType::ALL.iter().position(|&t| t == slot.slot_type).unwrap_or(0);
        let mut sel = cur;
        egui::ComboBox::from_id_salt(format!("fxt_{layer}_{idx}"))
            .width(115.0)
            .selected_text(slot.slot_type.name())
            .show_ui(ui, |ui| {
                for (j, &ty) in FxSlotType::ALL.iter().enumerate() {
                    if ui.selectable_value(&mut sel, j, ty.name()).clicked() && sel != cur {
                        slot.slot_type = FxSlotType::ALL[sel];
                        slot.params = slot.slot_type.default_params();
                        changed = true;
                    }
                }
            });

        if slot.slot_type == FxSlotType::None {
            return;
        }

        // Mix
        let mut mix = slot.mix;
        if ui.add(
            egui::Slider::new(&mut mix, 0.0..=1.0)
                .text("Mix").show_value(false)
        ).changed() {
            slot.mix = mix;
            changed = true;
        }

        // Parameters
        let labels = slot.slot_type.param_labels();
        for (j, label_opt) in labels.iter().enumerate() {
            if let Some(label) = label_opt {
                let (min, max) = slot.slot_type.param_range(j);
                let mut v = slot.params[j];
                if ui.add(
                    egui::Slider::new(&mut v, min..=max)
                        .text(*label).show_value(false)
                ).changed() {
                    slot.params[j] = v;
                    changed = true;
                }
            }
        }
    });

    changed
}
