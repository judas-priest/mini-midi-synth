//! On-screen piano keyboard display.

use std::sync::atomic::Ordering;
use eframe::egui;

use super::App;
use super::NOTE_NAMES;
use super::theme;

fn note_name(note: u8) -> String {
    let name = NOTE_NAMES[(note % 12) as usize];
    let oct = (note as i8 / 12) - 2;
    format!("{name}{oct}")
}

fn is_black_key(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

/// Pad notes in physical order: pads 1-8 (top row), 9-16 (bottom row).
pub const PAD_TOP_NOTES: [u8; 8] = [40, 41, 42, 43, 48, 49, 50, 51];
pub const PAD_BOT_NOTES: [u8; 8] = [36, 37, 38, 39, 44, 45, 46, 47];

impl App {
    pub fn draw_keyboard(&mut self, ui: &mut egui::Ui) {
        // Active notes label above keyboard; detected chord in brackets.
        // Строка ЗАРАНЕЕ фиксированной высоты и рисуется painter'ом одной
        // строкой без переноса: раньше при аккорде текст удлинялся, ярлык
        // переносился на вторую строку, высота панели менялась и весь
        // центральный столб выше «прыгал».
        let held: Vec<u8> = (0..128u8)
            .filter(|i| self.note_state[*i as usize].load(Ordering::Relaxed) > 0)
            .collect();
        let active: Vec<String> = held.iter().map(|n| note_name(*n)).collect();

        let line_h = ui.text_style_height(&egui::TextStyle::Body);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), line_h), egui::Sense::hover());
        let painter = ui.painter();
        if !active.is_empty() {
            let font = egui::TextStyle::Body.resolve(ui.style());
            let color = ui.visuals().text_color();
            let notes = active.join("  ");
            painter.text(
                egui::pos2(rect.left(), rect.center().y),
                egui::Align2::LEFT_CENTER,
                &notes,
                font.clone(),
                color,
            );
            if let Some(c) = crate::chord::detect(&held) {
                let w = painter.layout_no_wrap(notes.clone(), font.clone(), color).size().x;
                painter.text(
                    egui::pos2(rect.left() + w + 10.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    format!("({c})"),
                    font,
                    theme::CURVE_FILTER, // аккорд — акцентным цветом, без смены размера
                );
            }
        }

        ui.horizontal(|ui| {
            // --- Piano keys ---
            let start_note: u8 = 36;
            let end_note: u8 = 96;

            let total_white = (start_note..end_note)
                .filter(|n| !is_black_key(*n))
                .count() as f32;

            // Reserve space for pads on the right: 8 pads * pad_size + gap
            let pad_gap = 3.0_f32;
            let pad_size = ((ui.available_width() * 0.25) / 8.0 - pad_gap).clamp(32.0, 48.0);
            let pad_block_w = 8.0 * (pad_size + pad_gap) + 12.0; // 12px margin

            let available_w = ui.available_width() - pad_block_w;
            let key_w = (available_w / total_white).clamp(6.0, 18.0);
            let key_h = (key_w * 3.5).min(70.0);
            let black_h = key_h * 0.62;

            let (response, painter) = ui.allocate_painter(
                egui::vec2(total_white * key_w, key_h),
                egui::Sense::hover(),
            );
            let rect = response.rect;

            // White keys
            let mut wx = 0.0_f32;
            for note in start_note..end_note {
                if is_black_key(note) { continue; }
                let vel = self.note_state[note as usize].load(Ordering::Relaxed);
                let key_rect = egui::Rect::from_min_size(
                    rect.min + egui::vec2(wx, 0.0),
                    egui::vec2(key_w - 1.0, key_h),
                );
                let color = if vel > 0 {
                    theme::KEY_WHITE_ACTIVE
                } else {
                    theme::KEY_WHITE_IDLE
                };
                painter.rect_filled(key_rect, 1.0, color);
                painter.rect_stroke(key_rect, 1.0, egui::Stroke::new(0.5, theme::STROKE_KEY_BORDER), egui::StrokeKind::Outside);
                wx += key_w;
            }

            // Black keys
            wx = 0.0;
            for note in start_note..end_note {
                if is_black_key(note) {
                    let vel = self.note_state[note as usize].load(Ordering::Relaxed);
                    let bw = key_w * 0.65;
                    let key_rect = egui::Rect::from_min_size(
                        rect.min + egui::vec2(wx - bw * 0.5, 0.0),
                        egui::vec2(bw, black_h),
                    );
                    let color = if vel > 0 {
                        theme::KEY_BLACK_ACTIVE
                    } else {
                        theme::BG_BLACK_KEY
                    };
                    painter.rect_filled(key_rect, 1.0, color);
                } else {
                    wx += key_w;
                }
            }

            ui.add_space(theme::SP_LG);

            // --- Pads (C1–D#2 = MIDI 36–51, MPC layout as 2×8) ---
            // Top row: E1,F1,F#1,G1, C2,C#2,D2,D#2 — cyan
            // Bottom row: C1,C#1,D1,D#1, G#1,A1,A#1,B1 — pink

            let pad_h = (key_h - pad_gap) / 2.0;
            let pad_w = pad_size;
            let total_pad_w = 8.0 * (pad_w + pad_gap);
            let total_pad_h = key_h;

            let (pad_resp, pad_painter) = ui.allocate_painter(
                egui::vec2(total_pad_w, total_pad_h),
                egui::Sense::hover(),
            );
            let pad_origin = pad_resp.rect.min;

            // Click a drawn pad -> remap dialog for that hardware pad (position = pad number)
            let mut clicked_pad: Option<usize> = None;

            // Pad color by the category of the sound it triggers (remap-aware)
            let pad_color = |note: u8| -> egui::Color32 {
                let i = (note - 36) as usize;
                let target = self.drum_remap[i] as usize;
                let kit_idx = target.wrapping_sub(30);
                let name = if kit_idx < self.drum_kit_names.len() {
                    self.drum_kit_names[kit_idx].to_lowercase()
                } else {
                    String::new()
                };
                if name.contains("hat") || name.contains("hh") {
                    theme::DRUM_CAT_HATS
                } else if name.contains("crash") || name.contains("cymbal") || name.contains("ride") || name.contains("china") {
                    theme::DRUM_CAT_CYMBALS
                } else if name.contains("tom") {
                    theme::DRUM_CAT_TOMS
                } else if name.contains("kick") {
                    theme::DRUM_CAT_KICK
                } else if name.contains("snare") {
                    theme::DRUM_CAT_SNARE
                } else {
                    theme::DRUM_CAT_PERC
                }
            };

            // Top row
            for col in 0..8u8 {
                let note = PAD_TOP_NOTES[col as usize];
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let pr = egui::Rect::from_min_size(
                    pad_origin + egui::vec2(col as f32 * (pad_w + pad_gap), 0.0),
                    egui::vec2(pad_w, pad_h),
                );
                let color = if vel > 0 {
                    egui::Color32::WHITE
                } else {
                    pad_color(note)
                };
                pad_painter.rect_filled(pr, 3.0, color);
                pad_painter.rect_stroke(pr, 3.0, egui::Stroke::new(0.5, theme::STROKE_PAD_BORDER), egui::StrokeKind::Outside);
                if ui.interact(pr, egui::Id::new("pad_top").with(col), egui::Sense::click()).clicked() {
                    clicked_pad = Some(col as usize);
                }
            }

            // Bottom row
            for col in 0..8u8 {
                let note = PAD_BOT_NOTES[col as usize];
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let pr = egui::Rect::from_min_size(
                    pad_origin + egui::vec2(col as f32 * (pad_w + pad_gap), pad_h + pad_gap),
                    egui::vec2(pad_w, pad_h),
                );
                let color = if vel > 0 {
                    egui::Color32::WHITE
                } else {
                    pad_color(note)
                };
                pad_painter.rect_filled(pr, 3.0, color);
                pad_painter.rect_stroke(pr, 3.0, egui::Stroke::new(0.5, theme::STROKE_PAD_BORDER), egui::StrokeKind::Outside);
                if ui.interact(pr, egui::Id::new("pad_bot").with(col), egui::Sense::click()).clicked() {
                    clicked_pad = Some(8 + col as usize);
                }
            }

            if let Some(p) = clicked_pad {
                self.remap_pad = Some(p);
                self.show_drum_remap = false;
            }
        });
    }
}
