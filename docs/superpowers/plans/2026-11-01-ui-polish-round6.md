# UI Polish Round 6 (#36-39, #44, #45) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-01 (база = origin/master после #28, worktree `~/Projects/mini_midi_synth-verify`)
status: executed 2026-11-01 (Task 1 удалён ревью — уже сделан в раунде 5)

**Goal:** UI-проход: отступ Add part (#36), браузер — скролл-flag (#37) и фикс-ширина (#38), индикатор редактируемой шкалы в топ-баре + тултипы при drag (#39+#44), телеметрия в настройки (#44), колесо меняет слайдеры/ComboBox (#45).

**Architecture:** только `src/gui/` + `src/config.rs` (флаг show_telemetry). Research (2026-11-01): персист позиции в сессии достаточен (#34 хвост — отложен); ComboBox-клавиатура — egui-ограничение, закрываем колесом (#45).

**Tech Stack:** Rust, egui 0.31.1.

---

## Test Cases (from the requirement)

TC1: ~~отступ Add part~~ (сделан в раунде 5 — Task 1 удалён). TC2: колесо в браузере скроллит список, список НЕ прыгает к выделению; стрелки по-прежнему доскролливают. TC3: ввод в поиск не растягивает окно. TC4: крутишь Pan/Tr/Vol/L/T/D/P — в правом углу топ-бара «Имя: значение». TC5: телеметрия скрыта по умолчанию, чекбокс в Settings возвращает. TC6: колесо над слайдером меняет значение и НЕ скроллит контейнер (ComboBox-колесо отложено).

## Negative Matrix

- zero/empty — active_knob None (ничего не крутили): правый угол пуст, без паники.
- duplicate/concurrent — два слайдера под рукой: индикатор показывает последний changed() — приемлемо.
- wheel-конфликт: smooth_scroll_delta обнуляется ТОЛЬКО при hovered-виджете; иначе остаётся контейнеру. Тест на части-строке в ScrollArea (контейнер НЕ скроллит).
- missing field / clock skew / timeout — N/A.

---

### Task 1: УДАЛЁН при ревью

#36 (отступ над «+ Add part») уже сделан в раунде 5 (коммит 4ce2635, уже в master) — задача из плана удалена, чтобы не задвоить отступ.

### Task 2: Браузер — скролл-flag (#37) и фикс-ширина (#38)

**Files:** Modify: `src/gui/mod.rs` — `draw_instrument_browser_modal`; поле `inst_browser_scroll: bool` (рядом с `inst_browser_index` ~:431) + init `inst_browser_scroll: false,` в lib.rs; `src/gui/layers.rs`/`mod.rs` — места `inst_browser_part = Some(i)` не трогаем

1. Флаг: ставить `true` при (a) движении стрелками (`moved != 0` в навигационном блоке), (b) сбросе фильтра (в блоке `inst_browser_last_search`), (c) Enter-применении — не нужен (окно и так применит). Снять после первого `scroll_to_me` на выделенной строке:

```rust
        if moved != 0 {
            let new_idx = self.inst_browser_index as i32 + moved;
            self.inst_browser_index = new_idx.max(0) as usize;
            self.inst_browser_scroll = true; // (#37) скролл только по смене индекса
        }
```

(при открытии браузера — оба места `inst_browser_part = Some(i)` в layers.rs — ставить `self.inst_browser_scroll = true;`: открытие центрирует текущий выбор.)

В обоих ветках рендера (GM/DSP) заменить:

```rust
                                    if is_hi { r.scroll_to_me(Some(egui::Align::Center)); }
```

на:

```rust
                                    if is_hi && self.inst_browser_scroll {
                                        r.scroll_to_me(Some(egui::Align::Center));
                                        self.inst_browser_scroll = false; // (#37) один раз
                                    }
```

2. Ширина (#38): сразу после `ui.label(egui::RichText::new(title).strong());`:

```rust
            // (#38) фикс-минимальная ширина: длинные строки поиска/результатов
            // не распирают окно
            ui.set_min_width(380.0);
```

И обрезать длинные имена строк до 30 символов с hover-полным (GM: `format!("  {}", &name[..name.len().min(30)])` — по границе char: `name.chars().take(30).collect::<String>()`; hover — `r.on_hover_text(name.clone())`). DSP-ветка: то же для `patch.name`.

- [ ] Step 1: правки, clippy + cargo test
- [ ] Step 2: commit `fix: browser — scroll only on index change, fixed min width, truncated rows (#37/#38)`

### Task 3: Индикатор шкалы в топ-баре + телеметрия в настройки (#39+#44)

**Files:**
- Modify: `src/gui/mod.rs` — App-поле `pub active_knob: Option<(String, String)>` (None по умолчанию; init в lib.rs `active_knob: None,`); метод-хелпер:

```rust
    /// Запомнить «что сейчас крутят» — для индикатора в правом углу топ-бара
    /// (#39/#44: значение видно весь drag, тултип не обязателен).
    pub(crate) fn knob_feedback(&mut self, r: &egui::Response, name: &str, value: String) {
        if r.dragged() || r.changed() || r.hovered() {
            self.active_knob = Some((name.to_string(), value));
        }
    }
```

- Сайты вызова (после создания response, до/после обработчиков — не важно):
  - layers.rs парт-строка: Pan (`knob_feedback(&pan_r, "Pan", format!("{pan:+.2}"))`), Tr (`"Tr", format!("{tr:+} st")`), Volume (`"Vol", format!("{vol:.2}")`), Split point slider в draw_part_rows (`"Split", note_name(self.split_point)`).
  - drums.rs L/T/D/P: `knob_feedback(&r, "L", format!("{level:.2}"))` — response сейчас инлайн в if; развернуть в `let r = ui.add(...)` + `if r.changed() {...}`.
  - mod.rs мастер F1-F4 слайдеры (Tone/Reverb/Vol/PB) — аналогично.
- Правый угол топ-бара: в `ui.with_layout(right_to_left, ...)` ПЕРЕД телеметрией:

```rust
                    // (#44) индикатор редактируемой шкалы
                    if let Some((name, val)) = &self.active_knob {
                        ui.label(egui::RichText::new(format!("{name}: {val}")).strong());
                        ui.separator();
                    }
```

- Телеметрия: поле `pub show_telemetry: bool` в `ConfigUi`-секцию (src/config.rs, struct ui-настроек, `#[serde(default)]`), init false; весь блок `dropped|errs` + label с host/SR/buf/dsp (mod.rs ~:891-925) обернуть в `if self.config.ui.show_telemetry { ... }`; чекбокс в settings.rs (рядом с led_companion): `ui.checkbox(&mut self.config.ui.show_telemetry, "Show telemetry (drops/errs, JACK, dsp load)")` + save_config.
  - ВАЖНО: `take_avg/take_worst` читать только когда телеметрия видна; когда скрыта — раз в кадр всё равно вызывать `take_*` и выбрасывать (иначе worst накопится за минуты и выстрелит при включении) — оставить чтение безусловным, скрыть только отрисовку.

- [ ] Step 1: поле+хелпер+сайты, clippy
- [ ] Step 2: телеметрия-флаг + settings-чекбокс, clippy + cargo test
- [ ] Step 3: commit `feat: active-knob indicator in top bar; telemetry behind settings flag (#39/#44)`

### Task 4: Колесо меняет слайдеры (#45) — переписано по ревью

**Files:** Modify: `src/gui/layers.rs` (Pan/Tr/Vol), `src/gui/drums.rs` (L/T/D/P), `src/gui/mod.rs` (F2 Tone / F3 Reverb / F4 Vol / PB — F1 это кнопка Sustain, не слайдер)

РЕВЬЮ-ФАКТЫ (egui 0.31.1): ScrollArea ест `smooth_scroll_delta` в `end()` — ПОСЛЕ контент-closure, значит обнуление изнутри виджета работает; обнулять надо `i.smooth_scroll_delta` (НЕ raw_scroll_delta — оно для контейнера no-op). `Slider::id()` в 0.31 НЕ существует; `read_response` прошлого кадра по предсказанному id не сработает. Рабочий путь — post-add hover: изменить значение ПОСЛЕ response и выполнить push вручную (значение примет следующий кадр, ~16 мс задержки — незаметно).

Шаблон (Pan, layers.rs; остальные 10 сайтов аналогично, после существующего `let x_r = ui.add(...)`):

```rust
            // (#45) колесо над слайдером = шаг значения; глотаем smooth-дельту,
            // чтобы ScrollArea не скроллил список под слайдером
            if pan_r.hovered() {
                let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    pan = (pan - dy * 0.0025).clamp(-1.0, 1.0);
                    ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                    self.parts[i].pan = pan;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan });
                    self.knob_feedback(&pan_r, "Pan", format!("{pan:+.2}"));
                }
            }
```

Сайты и шаги: Pan/Vol/L ±0.0025/px, Tr (i32) `tr += (dy * 0.05).round() as i32` (нотч ≈1 шаг на ~20px), T ±0.05 st/px, D ±0.01 s/px, F2 (логарифмический — шаг кратный, допустимо), F3/F4 float, PB (i32, 1..=24) — как Tr: `(dy * 0.05).round() as i32`. НАПРАВЛЕНИЕ: колесо-вверх (dy>0) = УВЕЛИЧЕНИЕ значения — единый знак `value += dy * k` на всех 11 сайтах. Для drums-сайтов сохранять `add_enabled(dsp_kit, ...)` для D/P (hovered на disabled=false — колесо само мёртвое, гейт оставлен belt-and-suspenders). НИЧЕГО из существующих hover-тултипов не удалять.

- [ ] Step 1: 11 сайтов, clippy + cargo test + ручной TC6 (слайдеры)
- [ ] Step 2: commit `feat: mouse wheel changes slider values (#45)`

### Task 5: Верификация

- [ ] clippy/test/rt_safety; `cargo run --release`; TC1-TC6 на экране
- [ ] PR → squash merge → перезапуск release

## Out of scope

- #34 (персист позиции в конфиг), #35-хвост (клавиатура в ComboBox — заменено колесом, Task 4 ограничен слайдерами).
- Каскад wheel-конфликтов с кастомными гридами — тестировать на части-строках; при проблемах сузить до master F1-F4.
