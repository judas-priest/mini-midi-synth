# UI/UX Polish Round 5 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-10-31 (база 3f7397b = origin/master, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** UI/UX-бэклог раунда 5: клавиатурная навигация в браузере инструментов, стабильная позиция окна браузера, отступ Add part, таймлайн по сигнатуре, контраст pad-perf, хит-зоны FX-стрелок, tooltips на обрезанных именах, один restart-баннер, slider_width из темы.

**Architecture:** только `src/gui/`. Движок не трогается. Практики (Tavily-исследование 2026-10-31): навигация pick-list — виртуальный выделенный индекс в состоянии (не roving focus), фильтр сбрасывает индекс, без wrap-around (VS Code Quick Pick); окно — «последняя позиция» в течение сессии через стабильный Id; хит-цели ≥24×24 (WCAG 2.5.8); эллипсис без tooltip недопустим (M3); restart-required — persistent-баннер по состоянию, не toast.

**Tech Stack:** Rust, egui 0.31.1 (`Response::scroll_to_me`, `ctx.input`, `Window::id_salt`).

---

## Test Cases (from the requirement)

TC1 (клавиатура): открыт браузер → ↑/↓ двигают выделение (в т.ч. при фокусе в поиске), Enter выбирает, Esc закрывает; ввод в поиск сбрасывает выделение на первый результат.
TC2 (окно): закрыл и открыл браузер — окно в той же позиции, что при закрытии; заголовок одинаковый для всех партов.
TC3 (визуально): над «+ Add part» воздух; таймлайн лупера в 3/4 — бар-линии каждые 3 бита.
TC4 (контраст, тест в theme.rs): PAD_PERF_TOP/BOT ≥3:1 к BG_PAD_EMPTY; чёрный текст ≥4.5:1 на обоих.
TC5 (визуально): FX ↑/↓ — кликабельные прямоугольники ≥24px, не вплотную; обрезанные имена MIDI-треков и pad-perf показывают полное имя по hover; «Restart to apply» видно только когда есть несохранённая разница.

## Negative Matrix

- zero/empty — пустой результат поиска: flat-список пуст, индекс зажат в 0, Enter ничего не делает (явный код).
- duplicate/concurrent — Enter и клик в одном кадре: один путь применения (picked-переменные), двойное применение невозможно (picked перезаписывается тем же значением).
- clock skew / external timeout / missing field / null — N/A: отрисовка и usize-индекс.
- индекс вне диапазона после смены фильтра/парта: зажим `idx = idx.min(flat.len().saturating_sub(1))` каждый кадр — явный код.

---

### Task 1: Отступ над «+ Add part» (TECHDEBT #36)

**Files:** Modify: `src/gui/layers.rs` — `draw_part_rows`, оба вхождения `+ Add part` (~127-136, ~143-152)

Перед каждым `if a_count < PARTS_PER_ZONE && ui.small_button("+ Add part")...` и аналогом для зоны B:

```rust
            ui.add_space(theme::SP_SM); // воздух над Add part (TECHDEBT #36)
            let a_count = ZONE_A.filter(|&i| self.parts[i].enabled).count();
```

(для зоны B — перед `let b_count`, после цикла; `theme::SP_SM` = 4).

- [ ] Step 1: правка, `cargo clippy --all-targets -- -D warnings`
- [ ] Step 2: commit `fix: breathing room above + Add part buttons (TECHDEBT 36)`

### Task 2: Таймлайн лупера по реальной сигнатуре (аудит #21)

**Files:** Modify: `src/gui/drums.rs` — `draw_looper_timeline`, блок бар-линий (~1150-1163)

`beat % 4 == 0` — жёсткая 4/4. Заменить:

```rust
        let loop_steps = self.looper_atoms.loop_steps.load(std::sync::atomic::Ordering::Relaxed);
        let loop_steps = if loop_steps == 0 { self.looper_bars as u32 * 16 } else { loop_steps as u32 };
        let beats = loop_steps / 4;
        // Сигнатура = сигнатура текущего паттерна (лупер пишется поверх него);
        // раньше бар-линии были жёстко каждые 4 бита — в 3/4 врали (аудит #21).
        let bpb = self.drum_patterns[self.drum_current_pattern as usize].beats_per_bar.max(1) as u32;
        for beat in 1..beats {
            let x = rect.left() + (beat as f32 / beats as f32) * total_w;
            let color = if beat % bpb == 0 { theme::STROKE_BAR_LINE } else { theme::STROKE_GRID_DARK };
```

(строка `let color = if beat % 4 == 0 {` и тело цикла иначе без изменений; сверить, что `self.drum_patterns` доступен — метод `&self`, поле есть.)

- [ ] Step 1: правка, clippy + `cargo test`
- [ ] Step 2: commit `fix: looper timeline bar lines follow the pattern time signature`

### Task 3: Окно браузера — стабильный заголовок/Id (TECHDEBT #34)

**Files:** Modify: `src/gui/mod.rs` — `draw_instrument_browser_modal` (~1242-1260)

Per-part заголовок «GM Instrument — Part A1» создаёт разный Id окна (egui хранит позицию по Id) → каждый парт «своя» позиция, сброс при каждом открытии. Стабилизировать:

```rust
        // egui хранит позицию окна по Id, а Id выводится из заголовка —
        // per-part заголовок сбрасывал её на каждом парте (TECHDEBT #34).
        // Константный заголовок = стабильный Id = память позиции в сессии.
        // (Window::id_salt в egui 0.31 НЕ существует — не добавлять.)
        let win = egui::Window::new("Instrument Browser")
            .open(&mut win_open)
            .resizable(true)
            .default_width(400.0)
            .default_height(540.0)
            .show(ctx, |ui| {
            ui.label(egui::RichText::new(title).strong()); // прежний per-part заголовок — внутрь
            ui.add_space(theme::SP_XS);
```

(переменная `title` уже строится в текущем коде; `.default_pos` не нужен — egui сам разместит по центру при первом появлении, дальше помнит последнюю позицию.)

- [ ] Step 1: правка, clippy
- [ ] Step 2: commit `fix: instrument browser — stable window id/title, session position memory`

### Task 4: Клавиатурная навигация в браузере (TECHDEBT #35, TC1)

**Files:**
- Modify: `src/gui/mod.rs` — App-поля `inst_browser_index: usize`, `inst_browser_last_search: String` (рядом с `inst_browser_part`, ~:431) + init в `src/lib.rs` (`inst_browser_index: 0, inst_browser_last_search: String::new(),`)
- Modify: `src/gui/mod.rs` — `draw_instrument_browser_modal`

Паттерн VS Code Quick Pick: фокус остаётся в поиске (single-line TextEdit не использует ↑/↓), стрелки двигают виртуальный индекс, Enter применяет, Esc закрывает, фильтр сбрасывает индекс, wrap-around нет.

1. Навигационный ввод — В НАЧАЛЕ `draw_instrument_browser_modal`, сразу после `let Some(part) = ... else { return };` и до `let mut close = false;` (НЕ внутри Window-closure: `return` там вернул бы только из closure; `query`/`searching` для этого блока и не нужны):

```rust
        // Клавиатурная навигация (VS Code Quick Pick): виртуальный индекс в
        // состоянии; фильтр сбрасывает; wrap-around нет. Single-line поиск
        // не использует ↑/↓, поэтому перехват безопасен при любом фокусе.
        let moved = ctx.input(|i| {
            let mut d: i32 = 0;
            if i.key_pressed(egui::Key::ArrowDown) { d += 1; }
            if i.key_pressed(egui::Key::ArrowUp) { d -= 1; }
            d
        });
        if moved != 0 {
            let new_idx = self.inst_browser_index as i32 + moved;
            self.inst_browser_index = new_idx.max(0) as usize;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.inst_browser_part = None;
            return;
        }
```

2. Сбор видимого плоского списка. В начале функции (рядом с `let mut picked_program`) объявить `let mut flat: Vec<(String, usize)> = Vec::new();`. В GM-ветке клик-строка превращается в:

```rust
                                for i in matches {
                                    let name = GM_PROGRAM_NAMES[i];
                                    flat.push((format!("  {name}"), i));
                                    let selected = current_program == i as u8;
                                    let is_hi = self.inst_browser_index == flat.len() - 1;
                                    let r = ui.selectable_label(selected || is_hi, format!("  {name}"));
                                    if is_hi { r.scroll_to_me(egui::Align::Center); }
                                    if r.clicked() && !selected {
                                        self.inst_browser_index = flat.len() - 1;
                                        picked_program = Some(i as u8);
                                    }
                                }
```

DSP-ветка — аналогично вокруг существующего `selectable_label(selected, format!("  {}", patch.name))` (индекс `i`, `picked_patch`). Категорийные заголовки в flat не попадают — только видимые элементы. Клик пишет и picked_*, и индекс — один источник.

3. После `.show` окна (post-closure, где читаются picked_*) — зажим индекса:

```rust
        let flat_len = flat.len();
        if self.inst_browser_index >= flat_len {
            self.inst_browser_index = flat_len.saturating_sub(1);
        }
```

(сброс при фильтре — шаг 6; дублировать механизм здесь не нужно.)

4. Enter — применить `flat[self.inst_browser_index]`. ВАЖНО: `flat` объявлен рядом с `picked_program` и мутируется внутри Window-closure — читать его после `.show` можно (тот же паттерн, что picked_*):

```rust
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !flat.is_empty() {
            let (_, idx) = &flat[self.inst_browser_index.min(flat.len() - 1)];
            if is_sf2 { picked_program = Some(*idx as u8); } else { picked_patch = Some(*idx); }
        }
```

(расположить в post-closure рядом с шагом 3; borrow `&flat` при записи `picked_*` не конфликтует — разные переменные.)

```rust
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) && flat_len > 0 {
            let (_, idx) = &flat[self.inst_browser_index.min(flat_len - 1)];
            if is_sf2 { picked_program = Some(*idx as u8); } else { picked_patch = Some(*idx); }
        }
```

5. Выделенная строка прокручивается в зону видимости: `response.scroll_to_me(egui::Align::Center)` (egui 0.31 `Response::scroll_to_me(Align)`) — уже в коде шага 2.

6. Сброс индекса при изменении фильтра — в блоке шага 1:

```rust
        if self.inst_browser_last_search != self.preset_search {
            self.inst_browser_index = 0;
            self.inst_browser_last_search = self.preset_search.clone();
        }
```

Поле `inst_browser_last_search: String` добавить рядом с `inst_browser_index`, init `String::new()` в lib.rs рядом с `inst_browser_index: 0`. Честная семантика (ревью): сброс срабатывает на ИЗМЕНЕНИЕ текста поиска; повторное открытие окна с тем же текстом индекс НЕ сбрасывает (безвредно — список тот же, зажим шага 3 покрывает SF2↔DSP).

- [ ] Step 1: поля + init, clippy
- [ ] Step 2: навигация/ввод/сброс, clippy + cargo test
- [ ] Step 3: commit `feat: keyboard navigation in instrument browser (arrows/Enter/Esc)`

### Task 5: Контраст pad-perf (аудит UI п.5, TC4)

**Files:**
- Modify: `src/gui/theme.rs` — `PAD_PERF_TOP`, `PAD_PERF_BOT`
- Modify: `src/gui/settings.rs` — цвет текста имени перфа WHITE → BLACK (~:83)
- Modify: `src/gui/theme.rs` — тесты в `mod tests`

```rust
/// Pad-perf top-row accent (≥3:1 к BG_PAD_EMPTY; чёрный текст ≥4.5:1).
pub const PAD_PERF_TOP: Color32 = Color32::from_rgb(0, 140, 160);
/// Pad-perf bottom-row accent.
pub const PAD_PERF_BOT: Color32 = Color32::from_rgb(220, 60, 150);
```

settings.rs: `egui::Color32::WHITE` в painter.text имени → `egui::Color32::BLACK`, `RichText`/FontId без изменений (painter.text(color: BLACK)).

Тест в `mod tests` theme.rs:

```rust
    /// Аудит UI п.5: пэды перфа видимы на пустом фоне (3:1), чёрное имя
    /// читается (4.5:1) — раньше белый текст на бирюзовом был 3.14:1 при 11px.
    #[test]
    fn pad_perf_colors_meet_contrast() {
        for (name, c) in [("TOP", PAD_PERF_TOP), ("BOT", PAD_PERF_BOT)] {
            let ce = contrast(c, BG_PAD_EMPTY);
            assert!(ce >= 3.0, "PAD_PERF_{name} vs BG_PAD_EMPTY: {ce:.2}:1 < 3:1");
            let ct = contrast(Color32::BLACK, c);
            assert!(ct >= 4.5, "BLACK on PAD_PERF_{name}: {ct:.2}:1 < 4.5:1");
        }
    }
```

- [ ] Step 1: правки + тест (красный на старых цветах → зелёный на новых)
- [ ] Step 2: clippy + `cargo test --lib gui::theme` (4 passed)
- [ ] Step 3: commit `fix: pad-perf contrast — brighter pads, black labels (WCAG)`

### Task 6: FX-стрелки ≥24×24 + словарь (аудит UI п. Low, WCAG 2.5.8)

**Files:** Modify: `src/gui/fx_chain.rs` (~:14, ~:49-53)

В `confirm_button`-вызове (~:14) только переименование ярлыка (2s-confirm сохраняется!):

```rust
            if super::confirm::confirm_button(ui, "Reset FX", &mut self.fx_reset_confirm, true, "Reset FX chain to defaults") {
```

Стрелки (~:50-56) — хит-зона 24×24 (WCAG 2.5.8):

```rust
                    ui.horizontal(|ui| {
                        let up = ui.add_sized([24.0, 24.0], egui::Button::new("↑").small())
                            .on_hover_text("Move up");
                        if i > 0 && up.clicked() { swap = Some((i - 1, i)); }
                        let down = ui.add_sized([24.0, 24.0], egui::Button::new("↓").small())
                            .on_hover_text("Move down");
                        if i + 1 < FX_SLOTS && down.clicked() { swap = Some((i, i + 1)); }
                    });
```

- [ ] Step 1: правки, clippy + cargo test
- [ ] Step 2: commit `fix: FX reorder buttons 24px targets; Reset labeled Reset FX`

### Task 7: Tooltips на обрезанных именах (M3: эллипсис без tooltip недопустим)

**Files:**
- Modify: `src/gui/midi_seq.rs` — `ui.label(name)` (~:195) → `ui.label(name).on_hover_text(track.name.clone())`
- Modify: `src/gui/settings.rs` — pad-perf: hover полного имени на `response` пэда (allocate_exact_size ~:48). `on_hover_text` потребляет response — переприсвоить: `let response = response.on_hover_text(self.pad_perf_map[idx].as_deref().unwrap_or("empty — click to assign"));` (полное имя, не 10 символов; вызов ДО использований `response.clicked()/secondary_clicked()` ниже)

- [ ] Step 1: правки, clippy
- [ ] Step 2: commit `fix: full-name tooltips for truncated track and pad-perf labels`

### Task 8: Один restart-баннер вместо лейбл+toast (research: состояние, не событие)

**Files:** Modify: `src/gui/settings.rs` (~:659-677)

```rust
                ui.add_space(theme::SP_SM);
                // Restart-required — состояние (настройки ≠ активные), а не
                // событие: один persistent-баннер, никакого toast. Backend
                // тоже требует рестарта — включён в сравнение.
                let pending_backend = self
                    .available_hosts
                    .get(self.selected_host_idx)
                    .map(|(_, name)| self.config.audio.backend != *name)
                    .unwrap_or(false);
                let pending = pending_backend
                    || self.selected_sample_rate != self.config.audio.sample_rate
                    || self.selected_buffer_size != self.config.audio.buffer_size;
                if pending {
                    ui.colored_label(theme::TEXT_WARN, "Restart required to apply audio changes.");
                }
                if ui.button("Save Audio Settings").clicked() {
                    #[cfg(not(target_os = "android"))]
                    if let Some((_, host_name)) = self.available_hosts.get(self.selected_host_idx) {
                        self.config.audio.backend = host_name.to_string();
                        self.config.audio.sample_rate = self.selected_sample_rate;
                        self.config.audio.buffer_size = self.selected_buffer_size;
                        self.save_config();
                    }
                    #[cfg(target_os = "android")]
                    {
                        self.config.audio.sample_rate = self.selected_sample_rate;
                        self.config.audio.buffer_size = self.selected_buffer_size;
                        self.save_config();
                    }
                }
```

(после save pending исчезает сам — selected == config; старые `show_toast("Audio saved. Restart to apply.")` ×2 и статичный лейбл :660 удалить.)

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `fix: single conditional restart banner replaces label+toast duplication`

### Task 9: slider_width из темы

**Files:** Modify: `src/gui/mod.rs` ~:497

```rust
            style.spacing.slider_width = theme::SLIDER_PRIMARY;
```

- [ ] Step 1: правка, clippy
- [ ] Step 2: commit `fix: slider_width from theme constant`

### Task 10: Верификация и запуск

- [ ] `cargo clippy --all-targets -- -D warnings`; `cargo test`; `cargo test --features rt-assert --test rt_safety`
- [ ] Пересобрать verify из смерженного master, чек-лист TC1-TC5 на экране
- [ ] PR → squash merge → перезапуск

## Out of scope

- «Каша» в params editor — юзер 2026-10-31: «забей пока» (Task 9 round-3 плана отложен).
- Margin::same(6) в params.rs — осознанно отклонено аудитом (внутри файла консистентно).
- Навигация стрелками в pad-remap ComboBox и settings — egui-ComboBox не даёт списка наружу; второй этап #35 после браузера.
- #30/#31 — отдельные design-раунды.
