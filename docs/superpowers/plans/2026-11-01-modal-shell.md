# Modal Window Shell + Migration Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-01 (база = origin/master после #47, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** Единая обёртка всех окон приложения (`src/gui/modal.rs`): Esc-стек (popup → окно), константный Id окна (память позиции при динамическом заголовке), футер Done 80×28 справа, hover на иконочных кнопках. Миграция: Pad Remap (+Reset 2s-confirm), Pad N, Keybindings (Esc-stack capture→окно, предупреждение о краже бинда, tooltip'ы), Settings + CC Mapping (ScrollArea), Pad Performances, Instrument Browser.

**Architecture:** Новый `src/gui/modal.rs` — `ModalWindow`-билдер: `ModalWindow::new(ctx, id_salt, title, &mut open, width).footer_done(true).show(|ui| ...)`; Esc гасит окно только когда не открыт ни один popup (`Memory::is_any_popup_open`); Id через `Window::id(Id::new(id_salt))` — заголовок становится динамическим без потери памяти позиции. Деструктив — существующий `confirm::confirm_button`.

**Tech Stack:** Rust, egui 0.31.1. Research 2026-11-01: Escape Hatch Law (Esc гасит слои от лёгкого к тяжёлому), ComboBox эллипсис без tooltip = недоступный текст, иконочная кнопка без tooltip = не делать, Window Id ≠ заголовок.

**⚠ Верифицировать при имплементации:** `Window::id_salt` в egui 0.31 — предыдущий ревьюер сказал «не существует»; docs.rs 0.31 показывает `Window::id(mut self, id: Id)`. Использовать `.id(egui::Id::new(id_salt))`; если и его нет — константный заголовок окна, динамичный лейбл внутрь тела (как в браузере).

---

## Test Cases (from the requirement)

GUI-only — юнит-тестов нет (рисование); каждый таск: clippy + cargo test зелёные, проверка на экране в Task 7.

- TC1: Esc в любом окне закрывает его; при открытом ComboBox-попапе первый Esc гасит попап, второй — окно.
- TC2: Keybindings: первый Esc отменяет capture («Press key...» → обычный вид), второй закрывает окно.
- TC3: Pad Remap «Reset» — красный «Sure?» 2 с → применяет дефолт; Done/Закрыть/Esc — без ресета.
- TC4: Pad N — заголовок «Pad N», позиция окна помнится между открытиями при смене пэда.
- TC5: перебинд на занятую клавишу — toast «steal» предупреждение.
- TC6: все окна закрываются Done/Esc/✕; tooltips на ⚙ ⌨ x.

## Negative Matrix

- popup открыт + Esc → окно НЕ закрывается (is_any_popup_open guard) — явный код.
- capture активен + окно закрыто крестиком → capture остаётся висеть? — нет: отмена capture при закрытии окна (явный код в Task 4).
- дубликат запроса Esc в кадре — key_pressed одноразовый за кадр, N/A.
- config без полей (старый JSON) — новых конфиг-полей нет. missing field/timeout — N/A.

---

### Task 1: `src/gui/modal.rs` — единая обёртка

**Files:**
- Create: `src/gui/modal.rs`
- Modify: `src/gui/mod.rs` — `mod modal;` (после `mod confirm;`)

```rust
//! Единая обёртка окон приложения (модалок): Esc-стек, стабильный Id окна,
//! футер Done. Эталон поведения — Instrument Browser.
//!
//! Esc-стек (Escape Hatch Law): popup (ComboBox/меню) гасится egui'ем раньше;
//! окно закрывается Esc'ом только когда popup'ов нет. Окнам с внутренним
//! «временным режимом» (capture клавиши) — `esc_closes(false)` + свой обработчик.

use super::theme;
use eframe::egui;

/// Открыт ли какой-либо popup (ComboBox и т.п.) — Esc идёт ему, не окну.
fn any_popup_open(ctx: &egui::Context) -> bool {
    ctx.memory(|m| m.any_popup_open())
}

/// Единая обёртка окна. `id_salt` — стабильный Id (память позиции/размера),
/// `title` — динамический заголовок-отображение. `esc_closes(false)` — окно
/// само обрабатывает Esc (внутренний режим). `footer_done` — футер с кнопкой
/// Done 80×28 справа (клик = закрыть). `resizable` — окно ресайзится.
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
                .inner;
            }
            r
        });
        if done_clicked {
            *self.win_open = false;
        }
        resp.and_then(|inner| inner.inner)
    }
}
```

+ в `mod.rs` gui: `use theme;` внутри modal.rs — `use super::theme;`.

**⚠ Если `Window::id(...)` не существует в 0.31** — ревью подтвердил: `Window::id(Id)` СУЩЕСТВУЕТ (window.rs:75); `id_salt` — нет. Использовать `.id(egui::Id::new(id_salt))`.

- [ ] Step 1: файл + регистрация модуля; компиляция `Window::id` — при отсутствии fallback
- [ ] Step 2: clippy; commit `feat: modal.rs — shared window shell (Esc-stack, stable Id, Done footer)`

### Task 2: Instrument Browser → shell

**Files:** Modify: `src/gui/mod.rs` — `draw_instrument_browser_modal`

- Убрать свой Esc-блок (`if ctx.input(... Escape) ... return`) — `esc_closes(true)` в shell закроет окно тем же путём (`inst_browser_part = None` — в shell вместо этого `*win_open=false`; адаптировать: браузер использует локальный `win_open` + пост-обработку `if !win_open { close = true; }` — сохраняется).
- Убрать свой футер Done (horizontal с кнопкой и hint) — `footer_done(true)`.
- **НЕ потерять #34**: позиция (default_pos + per-frame rect → config.ui.inst_browser_pos) — shell это НЕ делает; читать rect из `InnerResponse.response.rect.min` (resp первого уровня) и писать в config в существующих точках (:1665-1670, :1749-1751); `.default_height(540.0)` — потерян в shell → добавить builder-метод `default_height(f32)` в ModalWindow или передать через дополнительный параметр (минимум: добавить `default_height: f32` + `#[must_use] pub fn default_height(...)`).
- Вызовы shell:

```rust
        let mut win_open = true;
        let title = ...; // как сейчас
        let Some(inner) = modal::ModalWindow::new(
            ctx, "inst_browser", format!("Instrument Browser — {layer_label}"),
            &mut win_open, 400.0,
        ).show(|ui| {
            ui.label(egui::RichText::new(&title).strong());
            ...существующее тело без своего футера и без своего Esc...
        }) else {
            self.inst_browser_part = None;
            return;
        };
        let _ = inner;
        if !win_open { close = true; }
```

(Shell возвращает Option<R>; закрытие окна = None → `inst_browser_part = None`. R — тип результата body; body сейчас возвращает `()` — возвращать `()` и игнорировать.)

- [ ] Step 1: миграция, clippy + cargo test (TC1: Esc/Done/Esc-при-попапе)
- [ ] Step 2: commit `refactor: instrument browser on the shared modal shell`

### Task 3: Pad Remap → shell + Reset 2s-confirm + combo tooltips

**Files:** Modify: `src/gui/drums.rs` — `draw_drum_remap_modals` (~:555)

- `ModalWindow::new(ctx, "pad_remap", "Pad Remap", &mut win_open, 920.0).footer_done(false)` — футер не нужен (Close-логика: shell Done не рисуем, окно закрывается Esc/✕; отдельную кнопку Close убрать — редундантна при Esc/✕).
- `Reset` (:599-618) → `super::confirm::confirm_button(ui, "Reset", &mut self.pad_remap_reset_confirm, true, "Reset all 16 mappings to 1:1 and zero velocities")` — confirm оборачивает ВЕСЬ блок (remap 1:1 + нулевые velocities + persist). Поле `pad_remap_reset_confirm: Option<std::time::Instant>` в App + lib.rs init (None).
- ComboBox'ы китов (:572-579): `.response.on_hover_text(sel.clone())` — tooltip полного имени.
- [ ] Step 1: правки, clippy + test
- [ ] Step 2: commit `feat: pad remap on modal shell — Reset 2s-confirm, combo tooltips`

### Task 4: Pad N → shell (константный Id, Done)

**Files:** Modify: `src/gui/drums.rs` — single-pad remap (~:648)

- `ModalWindow::new(ctx, "pad_remap_single", format!("Pad {}", p + 1), &mut win_open, 320.0)` — константный id_salt «pad_remap_single»: позиция помнится при смене пэда, заголовок динамический.
- Убрать `ui.strong("Pad N")`-дубликат и кнопку Close (Done из shell).
- [ ] Step 1: правка, clippy + test
- [ ] Step 2: commit `feat: single-pad remap on modal shell (stable id, Done)`

### Task 5: Keybindings → shell + Esc-стек capture + steal-warning + tooltips

**Files:** Modify: `src/gui/mod.rs` — `draw_keybinds_window` (~:1994-2043), `src/gui/keybinds.rs` (set_key_for_action :66-78 — вернуть факт кражи), handler вызова (gui/mod.rs:721).

⚠ РЕВЬЮ-CRITICAL (порядок Esc): capture-cancel сейчас живёт в update-loop (gui/mod.rs:714-720), который выполняется ДО отрисовки окна — первый Esc очищает capture, и окно закроется тем же нажатием (TC2 сломан). Фикс: убрать Esc-ветку из update-loop (keybind_capturing отменяется ТОЛЬКО внутри окна), Esc-стек полностью переезжает в окно.

- Shell: `ModalWindow::new(ctx, "keybindings", "Keybindings", &mut open, 480.0).esc_closes(false).footer_done(false).resizable(false)` — Esc-логика своя в теле окна (первая строка body):

```rust
            let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            if esc {
                if self.keybind_capturing.is_some() {
                    self.keybind_capturing = None; // 1-й Esc: отменить capture
                } else {
                    open = false;                  // 2-й Esc: закрыть окно
                }
            }
```

(и убрать Esc-ветку из update-loop :714-720 — там останется только перехват обычных клавиш при capture.)

- Unbind «x» → `egui::Button::new("×").small()` + hover «Unbind» (2s-confirm unbind_confirm сохраняется).
- «Press key...» кнопка — hover «Press Escape to cancel».
- Steal-warning: `set_key_for_action` (src/gui/keybinds.rs:66-78) → возвращает `Option<String>` — имя действия, чей бинд украден (там `let _ = entry... retain` — переработать: найти жертву до перезаписи); единственный вызов gui/mod.rs:721; GUI: если Some → toast `format!("{}: key was reassigned from {}", key, stolen)`.

- [ ] Step 1: shell + Esc-стек + перенос cancel из update-loop, clippy
- [ ] Step 2: tooltips + steal-toast (TC5)
- [ ] Step 3: commit `feat: keybindings on modal shell — Esc-stack, steal warning, tooltips`

### Task 6: Settings/CC Mapping/Pad Perf/Help → shell

**Files:** Modify: `src/gui/settings.rs` (CC Mapping :132-187, Pad Performances :27, Settings :605)

- CC Mapping (settings.rs:130-188, fn draw_help): тело в `egui::ScrollArea::vertical().max_height(480)`; заголовок окна → «SMK-37 Controller Map», дубль-heading внутри убрать; shell `footer_done(false)`.
- Pad Performances: shell `footer_done(false)`, Esc закрывает (+persist как у закрытия крестиком — тот же вызов `save_global_to_config` в `!open`-ветке settings.rs:123-126).
- Settings: shell `footer_done(false)`, Esc закрывает (окно рисуется каждый кадр при open — per-frame Esc-гейт shell'а работает).
- Верхняя панель (gui/mod.rs:981-991): hover'ы на ⚙/⌨/? УЖЕ есть (Settings/CC Mapping Help/Keybindings) — шаг сводится к проверке, новых правок не требуется.
- [ ] Step 1: правки, clippy + test
- [ ] Step 2: commit `feat: settings/CC-map/pad-perf on modal shell — Esc, scroll, tooltips`

### Task 7: Верификация

- [ ] clippy/test/rt_safety; `cargo run --release`
- [ ] Экран: TC1-TC6 по всем окнам (Esc/Done/попапы, Reset-confirm, Pad N позиция, steal-toast)
- [ ] PR → squash merge → перезапуск release

## Out of scope

- #35 полный (клавиатура в egui ComboBox) — egui-ограничение.
- hold-to-confirm для Reset (исследование предлагает как альтернативу 2s) — у нас единый 2s-паттерн везде.
