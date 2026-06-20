//! On-screen piano keyboard display.

use std::sync::atomic::Ordering;
use eframe::egui;

use super::App;
use super::NOTE_NAMES;

fn note_name(note: u8) -> String {
    let name = NOTE_NAMES[(note % 12) as usize];
    let oct = (note as i8 / 12) - 2;
    format!("{name}{oct}")
}

fn is_black_key(note: u8) -> bool {
    matches!(note % 12, 1 | 3 | 6 | 8 | 10)
}

impl App {
    pub fn draw_keyboard(&self, ui: &mut egui::Ui) {
        // Active notes label above keyboard
        let active: Vec<String> = (0..128u8)
            .filter_map(|i| {
                let vel = self.note_state[i as usize].load(Ordering::Relaxed);
                if vel > 0 {
                    Some(note_name(i))
                } else {
                    None
                }
            })
            .collect();

        if active.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(100, 100, 100), " ");
        } else {
            ui.label(active.join("  "));
        }

        ui.horizontal(|ui| {
            // --- Piano keys ---
            let start_note: u8 = 36;
            let end_note: u8 = 96;

            let total_white = (start_note..end_note)
                .filter(|n| !is_black_key(*n))
                .count() as f32;

            // Reserve space for pads on the right: 8 pads * pad_size + gap
            let pad_size = 32.0_f32;
            let pad_gap = 2.0_f32;
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
                    egui::Color32::from_rgb(80, 180, 255)
                } else {
                    egui::Color32::from_rgb(220, 220, 220)
                };
                painter.rect_filled(key_rect, 1.0, color);
                painter.rect_stroke(key_rect, 1.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(120, 120, 120)), egui::StrokeKind::Outside);
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
                        egui::Color32::from_rgb(60, 140, 220)
                    } else {
                        egui::Color32::from_rgb(30, 30, 30)
                    };
                    painter.rect_filled(key_rect, 1.0, color);
                } else {
                    wx += key_w;
                }
            }

            ui.add_space(12.0);

            // --- Pads (C1–D#2 = MIDI 36–51, MPC layout as 2×8) ---
            // Top row: E1,F1,F#1,G1, C2,C#2,D2,D#2 — cyan
            // Bottom row: C1,C#1,D1,D#1, G#1,A1,A#1,B1 — pink
            const PAD_TOP: [u8; 8] = [40, 41, 42, 43, 48, 49, 50, 51];
            const PAD_BOT: [u8; 8] = [36, 37, 38, 39, 44, 45, 46, 47];

            let pad_h = (key_h - pad_gap) / 2.0;
            let pad_w = pad_size;
            let total_pad_w = 8.0 * (pad_w + pad_gap);
            let total_pad_h = key_h;

            let (pad_resp, pad_painter) = ui.allocate_painter(
                egui::vec2(total_pad_w, total_pad_h),
                egui::Sense::hover(),
            );
            let pad_origin = pad_resp.rect.min;

            // Top row (cyan)
            for col in 0..8u8 {
                let note = PAD_TOP[col as usize];
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let pr = egui::Rect::from_min_size(
                    pad_origin + egui::vec2(col as f32 * (pad_w + pad_gap), 0.0),
                    egui::vec2(pad_w, pad_h),
                );
                let color = if vel > 0 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(0, 200, 210)
                };
                pad_painter.rect_filled(pr, 3.0, color);
                pad_painter.rect_stroke(pr, 3.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(60, 60, 60)), egui::StrokeKind::Outside);
            }

            // Bottom row (pink)
            for col in 0..8u8 {
                let note = PAD_BOT[col as usize];
                let vel = self.pad_state[note as usize].load(Ordering::Relaxed);
                let pr = egui::Rect::from_min_size(
                    pad_origin + egui::vec2(col as f32 * (pad_w + pad_gap), pad_h + pad_gap),
                    egui::vec2(pad_w, pad_h),
                );
                let color = if vel > 0 {
                    egui::Color32::WHITE
                } else {
                    egui::Color32::from_rgb(220, 60, 150)
                };
                pad_painter.rect_filled(pr, 3.0, color);
                pad_painter.rect_stroke(pr, 3.0, egui::Stroke::new(0.5, egui::Color32::from_rgb(60, 60, 60)), egui::StrokeKind::Outside);
            }
        });
    }
}
