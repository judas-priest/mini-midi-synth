//! Единая обёртка окон приложения (модалок): Esc-стек, стабильный Id окна,
//! футер Done. Эталон поведения — Instrument Browser.
//!
//! Esc-стек (Escape Hatch Law): popup (ComboBox/меню) гасится egui'ем раньше;
//! окно закрывается Esc'ом только когда popup'ов нет. Окнам с внутренним
//! «временным режимом» (capture клавиши) — `esc_closes(false)` + свой обработчик.

use super::theme;
use eframe::egui;

/// Открыт ли какой-либо popup (ComboBox и т.п.) — Esc идёт ему, не окну.
// TODO(Task 2): remove — temporary until first caller appears.
#[allow(dead_code)]
fn any_popup_open(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.any_popup_open())
}

/// Единая обёртка окна. `id_salt` — стабильный Id (память позиции/размера),
/// `title` — динамический заголовок-отображение. `esc_closes(false)` — окно
/// само обрабатывает Esc (внутренний режим). `footer_done` — футер с кнопкой
/// Done 80×28 справа (клик = закрыть). `resizable` — окно ресайзится.
// TODO(Task 2): remove — temporary until first caller appears.
#[allow(dead_code)]
pub struct ModalWindow<'a> {
    ctx: &'a egui::Context,
    id_salt: &'a str,
    title: String,
    win_open: &'a mut bool,
    default_width: f32,
    default_height: f32,
    esc_closes: bool,
    footer_done: bool,
    resizable: bool,
}

// TODO(Task 2): remove — temporary until first caller appears.
#[allow(dead_code)]
impl<'a> ModalWindow<'a> {
    pub fn new(
        ctx: &'a egui::Context,
        id_salt: &'a str,
        title: impl Into<String>,
        win_open: &'a mut bool,
        default_width: f32,
    ) -> Self {
        Self {
            ctx,
            id_salt,
            title: title.into(),
            win_open,
            default_width,
            default_height: 540.0,
            esc_closes: true,
            footer_done: true,
            resizable: true,
        }
    }

    #[must_use]
    pub fn esc_closes(mut self, on: bool) -> Self {
        self.esc_closes = on;
        self
    }

    #[must_use]
    pub fn footer_done(mut self, on: bool) -> Self {
        self.footer_done = on;
        self
    }

    #[must_use]
    pub fn resizable(mut self, on: bool) -> Self {
        self.resizable = on;
        self
    }

    #[must_use]
    pub fn default_height(mut self, h: f32) -> Self {
        self.default_height = h;
        self
    }

    /// Показать окно. Возвращает `Some(Option<R>)`: внешняя Option — окно было
    /// отрисовано, внутренняя — Some(R) из body (None = body вернул ()).
    /// После вызова: `!*win_open` = окно закрылось (Esc/Done/✕).
    pub fn show<R>(
        self,
        body: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<Option<R>> {
        if self.esc_closes
            && self.ctx.input(|i| i.key_pressed(egui::Key::Escape))
            && !any_popup_open(self.ctx)
        {
            *self.win_open = false;
        }
        let footer_done = self.footer_done;
        let resizable = self.resizable;
        let mut done_clicked = false;
        let win = egui::Window::new(egui::RichText::new(self.title).strong())
            .id(egui::Id::new(self.id_salt))
            .open(self.win_open)
            .resizable(resizable)
            .default_width(self.default_width)
            .default_height(self.default_height);
        let resp = win.show(self.ctx, |ui| {
            let r = body(ui);
            if footer_done {
                ui.add_space(theme::SP_SM);
                done_clicked = ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| ui.add_sized([80.0, 28.0], egui::Button::new("Done")),
                )
                .inner
                .clicked();
            }
            r
        });
        if done_clicked {
            *self.win_open = false;
        }
        resp.map(|inner| inner.inner)
    }
}
