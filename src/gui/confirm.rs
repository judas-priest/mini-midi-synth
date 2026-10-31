//! Two-stage destructive-action confirmation ("Sure?" timer, 2 s).
//!
//! Первый клик взводит: кнопка превращается в красный «Sure?» на 2 секунды.
//! Второй клик внутри окна — действие выполняется; истечение окна снимает
//! взвод молча. Один механизм для кнопок и хоткеев (audit Medium п.9,
//! Strategic п.3; модалки остаются только для удаления партии).

use std::time::{Duration, Instant};

use eframe::egui;

/// How long the armed "Sure?" state stays valid.
pub const CONFIRM_WINDOW: Duration = Duration::from_secs(2);

/// Draw a destructive button with two-stage confirmation.
/// Returns `true` only on the confirming (second) click.
pub fn confirm_button(
    ui: &mut egui::Ui,
    label: &str,
    armed: &mut Option<Instant>,
    enabled: bool,
    tooltip: &str,
) -> bool {
    // Expire stale arming.
    if armed.map(|t| t.elapsed() >= CONFIRM_WINDOW).unwrap_or(false) {
        *armed = None;
    }
    let pending = armed.is_some();
    let text = if pending {
        egui::RichText::new("Sure?").color(egui::Color32::RED).strong()
    } else {
        egui::RichText::new(label)
    };
    let clicked = ui
        .add_enabled(enabled, egui::Button::new(text))
        .on_hover_text(tooltip)
        .clicked();
    if clicked {
        if pending {
            *armed = None;
            return true;
        }
        *armed = Some(Instant::now());
    }
    false
}
