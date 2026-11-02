# UI Feedback Round 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-10-31 (код 8555f16, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** Раунд фидбека 3: цвет имён партов, зоны-заголовки с диапазоном (один на руку) вместо пер-партовых; убрать карту зон внизу; таймлайн лупера виден без нот; док ресайзится; L/T/D/P с видимой причиной неактивности в SF2; Mute/Root полными словами; Mute слота ударных; «Undo Layer»→«Undo»; отступы.

**Architecture:** GUI (`src/gui/`) + один точечный движковый фич-эддт (mute слота ударных на уровне диспетчера триггеров — работает в DSP и SF2). Работа в ОДНОМ worktree (`~/Projects/mini_midi_synth-verify`), интеграция через PR (squash), без новых папок.

**Tech Stack:** Rust, egui 0.31.1. Research: identity (цвет парта) и selection (тинт фона) — разные каналы, без рамок; вертикальный ритм 2/4/8/12 (`theme::SP_*`); счётчики — только в записи (RC-505/Mobius/Ableton).

---

## Test Cases (from the requirement)

GUI-визуальные правки — юнит-тестов нет; каждый таск: `cargo clippy --all-targets -- -D warnings` + `cargo test` зелёные, визуальная проверка — чек-лист Task 9. Для движкового Task 7 — тест rt_safety обязателен + юнит на mute-гейт если предоставляет точку (см. таск).

## Negative Matrix

- missing field / null / clock skew / external timeout — N/A: отрисовка и булев флаг в движке.
- zero/empty — пустой лупер: Task 2 специализируется на этом (таймлайн при 0 событий).
- duplicate/concurrent — двойное переключение Mute слота: идемпотентный set; Undo Layer при 0 слоёв: кнопка add_enabled (уже есть).

---

### Task 1: Зоны вместо пер-партовых диапазонов; цвет имени парта; карту зон убрать (п.2 + п.5 + уточнение про range)

**Files:**
- Modify: `src/gui/layers.rs` — `draw_part_rows` (заголовки зон, вызов карты ~165-168), `draw_part_row` (label ~304-321, range ~383-387), удалить `draw_key_zone_map` (~511-конец)

**1a. Вернуть заголовки зон** (убраны во 2-м раунде по недоразумению — пользователь просил убрать КАРТУ ВНИЗУ, не заголовки). Диапазон руки пишется ОДИН раз в заголовке. После `// ── Zone A ──` :

```rust
            let a_header = if split_on {
                format!("◀ Left hand (A) — 0..{}", self.split_point.saturating_sub(1))
            } else {
                "◀ Left hand (A) — 0..127".to_string()
            };
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(a_header).weak().small());
            });
```

В зоне B (после `ui.add_space(theme::SP_XS);`):

```rust
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("▶ Right hand (B) — {}..127", self.split_point)).weak().small());
                });
```

**1b. Цвет имени парта + тинт активного** (в `draw_part_row`, заменить `SelectableLabel`-блок):

```rust
            // Цвет парта = цвет руки на карте (identity); активный — тинт
            // фоном (selection), без egui-selection-синевы и рамок (research).
            let label = format!("{zone}{num}");
            let muted = self.parts[i].mute;
            let part_color = theme::PART_COLORS[i % theme::PART_COLORS.len()];
            let name_color = if muted { egui::Color32::GRAY } else { part_color };
            let mut label_text = egui::RichText::new(label).strong().color(name_color);
            if muted { label_text = label_text.strikethrough(); }
            let btn = egui::Button::new(label_text)
                .fill(if is_active { part_color.gamma_multiply(0.30) } else { egui::Color32::TRANSPARENT })
                .stroke(egui::Stroke::NONE);
            if ui.add_sized([34.0, 0.0], btn)
                .on_hover_text(format!("Select part {zone}{num} for editing (muted = gray + strikethrough)"))
                .clicked()
            {
                // ... существующий обработчик без изменений
```

**1c. Убрать range-лейбл из парт-строки** (блок `// Key range (derived from split — display only)`, add_sized [92.0,...]) — диапазон теперь в заголовке зоны.

**1d. Убрать карту зон**: вызов в `draw_part_rows` (layers.rs:158) и саму `fn draw_key_zone_map` (505-548) — целиком (иначе dead-code error). Заодно: устаревший комментарий «минус карта зон» (layers.rs:112-113), бюджет высоты `max_h = available - 36.0` (114) пересчитать — место карты освобождается; упоминание layers.rs:516 в doc-комментарии theme.rs поправить.

**Проверка:** заголовок с диапазоном один на зону; в строках диапазона нет; имена партов цветные; активный тинтом; карты внизу нет.

- [ ] Step 1: правки, clippy + cargo test
- [ ] Step 2: commit `fix: zone headers carry the range; colored part names; drop key-zone map`

### Task 2: Таймлайн лупера виден без нот (п.3) — 2 исправления по ревью

**Files:**
- Modify: `src/gui/drums.rs` — вызов `draw_looper_timeline` (~1013, гейт `if event_count > 0`) и тело (~1128-1175)

**2a. Гейт вызова:** `draw_looper_timeline` вызывается только под `if event_count > 0` — при пустом лупере функция недостижима. Заменить гейт на (паттерн кнопки Clear, drums.rs:838):

```rust
        if event_count > 0 || state != LooperState::Idle {
            self.draw_looper_timeline(ui, state, position, &part_colors);
        }
```

**2b. Внутри функции:** убрать ранний выход `if display.events.is_empty() { return; }` (оставив только try_lock Err → return):

```rust
        // Пустой лупер НЕ выход: при WAIT/REC таймлайн с сеткой и playhead
        // должен быть виден (фидбек: «полоса записи не показывается, когда
        // нот нет»).
        let (events, current_layer) = {
            let display = match self.looper_display.try_lock() {
                Ok(d) => d,
                Err(_) => return,
            };
            (display.events.clone(), display.current_layer)
        };
```

**2c. Второй ранний выход** `if min_note > max_note { return; }` (после расчёта диапазона нот — при пустом events всегда true) обернуть секцию точек, не выходя:

```rust
        if min_note <= max_note {
            // ... весь существующий блок расчёта range/dot_w/dot_h и цикл for ev in &events без изменений
        }
```

Блок Playhead идёт ПОСЛЕ и остаётся безусловным (доступен при `state != Idle`).

**Проверка:** пустой лупер + Rec → таймлайн с сеткой и playhead; с нотами — как раньше.

- [ ] Step 1: правки, clippy + cargo test
- [ ] Step 2: commit `fix: looper timeline renders empty — grid + playhead during capture`

### Task 3: Док снова ресайзится (п.4)

**Files:**
- Modify: `src/gui/mod.rs` — `drum_seq_panel` (~1084-1090)

```rust
        egui::TopBottomPanel::bottom("drum_seq_panel")
            .resizable(true)
            .default_height(420.0)
            .height_range(240.0..=760.0)
            .show(ctx, |ui| {
```

(заменяет `.exact_height(500.0)` с комментарием про фикс-высоту; дефолт ниже — меньше места, но панель всегда видна).

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `fix: docked record panel resizable again, default 420`

### Task 4: L/T/D/P — видимая причина неактивности в SF2 (п.6)

**Files:**
- Modify: `src/gui/drums.rs` — блок per-instrument controls (после `let dsp_kit = ...`, ~268)

Перед `ui.horizontal(|ui| { ... L/T/D/P ... });`:

```rust
                if !dsp_kit {
                    ui.label(egui::RichText::new("L T D P: DSP-engine only — no effect on SF2 kits")
                        .small().weak());
                }
```

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `fix: visible hint that L/T/D/P are DSP-only under SF2 kits`

### Task 5: Mute/Root полными словами (п.11)

**Files:**
- Modify: `src/gui/layers.rs` — чекбоксы M/R в `draw_part_row`

```rust
            if ui.add_sized([52.0, 0.0], egui::Checkbox::new(&mut m, "Mute")).on_hover_text("Mute this part").changed() {
```
```rust
            if ui.add_sized([52.0, 0.0], egui::Checkbox::new(&mut r, "Root")).on_hover_text(
```

(ширина одинаковая — колонки строк совпадают; освобождается место от убранного range.)

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `fix: Mute/Root checkboxes spelled out`

### Task 6: «Undo Layer» → «Undo» (п.12)

**Files:**
- Modify: `src/gui/drums.rs` — кнопка в `draw_looper`

```rust
            if ui.add_enabled(layer_count > 1 && event_count > 0, egui::Button::new("Undo"))
                .on_hover_text(format!("Undo last looper layer (L{layer_count})")).clicked()
```

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `fix: relabel Undo Layer to Undo`

### Task 7: Mute слота ударных рядом с L/T/D/P (п.10) — движок + GUI

**Files:**
- Modify: `src/synth/mod.rs` — `SynthEngine`: поле `drum_slot_mutes: [bool; NUM_DRUM_SLOTS]`, обработка `ControlEvent::DrumSetMute`, гейт в диспетчере триггеров ударных
- Modify: `src/synth/mod.rs` (ControlEvent enum) — новый вариант
- Modify: `src/gui/drums.rs` — чекбокс Mute в ряду L/T/D/P
- Modify: `src/lib.rs` / GUI — ничего (событие через существующее кольцо)

**Движок (по ревью): единой точки в SynthEngine НЕТ — поле живёт на DrumEngine.**

1. `ControlEvent` (src/synth/mod.rs ~205, рядом с DrumSetParam): `DrumSetMute { slot: u8, mute: bool },`
2. Поле на `DrumEngine` (src/synth/drum.rs:954, рядом с `pub params`): `pub slot_mutes: [bool; NUM_DRUM_SLOTS],` — init `[false; NUM_DRUM_SLOTS]` в конструкторе.
3. `handle_control` (SynthEngine): `ControlEvent::DrumSetMute { slot, mute } => { if let Some(m) = self.drum_engine.slot_mutes.get_mut(slot as usize) { *m = mute; } }` — БЕЗ раннего `return Some(...)`: plain-data варианты проваливаются к общему `None` в конце (инвариант «reaching here means plain data», mod.rs:2885).
4. Гейты (mute = «дорожка инструмента выключена» — глушим всё):
   a) Секвенсор DSP: внутри `DrumEngine::tick` (drum.rs:1253-1262) перед `self.trigger_slot(...)`;
   b) Секвенсор SF2: цикл потребления `sf2_triggers` (synth/mod.rs:3414-3425) — skip при mute;
   c) Живые удары (пэды/MIDI): точка dispatch (synth/mod.rs:2058-2092) — не звать `drum_engine.note_on`/`sampler.drum_note_on`. Запись в секвенсор (`sequencer.record_hit`, ~2070) НЕ гейтовать — mute это решение микса, реплей продолжит писать события.
   Все три гейта читают `self.drum_engine.slot_mutes[slot]` — RT-safe (bool-чтение).

**GUI** (ряд с L/T/D/P, drums.rs): перед слайдерами

```rust
                    let mut sm = self.drum_slot_mutes_view[slot];
                    if ui.add_sized([52.0, 0.0], egui::Checkbox::new(&mut sm, "Mute"))
                        .on_hover_text("Mute this drum: sequencer and live hits are silent (recording continues)")
                        .changed()
                    {
                        self.drum_slot_mutes_view[slot] = sm;
                        let _ = self.ctrl_tx.push(ControlEvent::DrumSetMute { slot: slot as u8, mute: sm });
                    }
```

GUI-зеркало `drum_slot_mutes_view: [bool; NUM_DRUM_SLOTS]` в App (+init в lib.rs) — движок не ретранслирует состояние назад; источник истины — GUI (как drum_params).

- [ ] Step 1: движок (event, поле, гейт), cargo test + rt_safety
- [ ] Step 2: GUI чекбокс + зеркало, clippy + cargo test
- [ ] Step 3: commit `feat: per-slot drum mute — sequencer/replay only, live hits unaffected`

### Task 8: Отступы по шкале (п.7 + п.8)

**Files:**
- Modify: `src/gui/layers.rs` — после `self.draw_perf_row(ui);`:
```rust
        ui.add_space(theme::SP_MD); // отделить perf от split-ряда
```
- Modify: `src/gui/drums.rs` — `draw_drum_seq_docked`, после закрытия транспортного `ui.horizontal` (перед `// Pattern selector + length`):
```rust
        ui.add_space(theme::SP_MD); // ритм секций: транспорт | паттерн | грид
```
- Осмотр соседних стыков (split→парт-ряды, grid→mix-ряд): зазор 0 → `SP_SM`; между секциями — `SP_MD`. Только добавление где зазора нет (перед grid уже есть SP_SM, drums.rs:138 — не трогать).

- [ ] Step 1: правки, clippy + cargo test
- [ ] Step 2: commit `fix: vertical rhythm — SP_MD between perf/split and transport/pattern rows`

### Task 9: Редактор параметров — переносы строк и отступы (п.13 «в настройках каша»)

**Files:**
- Modify: `src/gui/params.rs` — `draw_params_editable` (инвентаризация → нормализация)

**Гипотезы для инвентаризации (аудит 2026-10-31; часть не подтвердилась в дереве — проверять на месте):** `Margin::same(6)` ×10 подтверждено; «5 вариантов grid-spacing» и «заголовки 3 приёмами» НЕ нашли в params.rs — считать неподтверждёнными до инвентаризации. Главный симптом — переносы посреди строки при нехватке ширины.

- [ ] Step 1: Инвентаризация — запустить, открыть редактор параметров (все секции), зафиксировать конкретные места «каши»: переносы mid-row, скачущие отступы, разнобой заголовков. Grep: `Margin::` / `add_sized` / `Grid::new` / `item_spacing` в params.rs.
- [ ] Step 2: Нормализация: 1) margin/spacing привести к SP_XS/SM/MD/LG; 2) строки, переносящиеся mid-row, — явные `add_sized` ширины или разбиение на подстроки так, чтобы перенос был МЕЖДУ контролами, не внутри; 3) заголовки секций — один приём (strong + SP_MD сверху/SP_SM снизу).
- [ ] Step 3: clippy + cargo test, визуальная сверка
- [ ] Step 4: commit `fix: params editor — spacing scale, no mid-widget line wraps, one section-header style`

### Task 10: Верификация и запуск

- [ ] `cargo clippy --all-targets -- -D warnings`; `cargo test`; `cargo test --features rt-assert --test rt_safety`
- [ ] Пересобрать verify из смерженного master, запустить; чек-лист:
  1. Заголовки зон с диапазоном (1 на руку), в парт-строках range нет
  2. Карты зон внизу нет; имена партов цветные, активный тинтом, рамок нет
  3. Пустой лупер + Rec → таймлайн с сеткой и playhead
  4. Док тянется за разделитель, дефолт 420
  5. SF2-кит → строка «DSP-engine only» над L/T/D/P
  6. Чекбоксы Mute/Root полными словами, колонки выровнены
  7. «Undo» (не «Undo Layer») рядом с Clear
  8. Mute слота ударных: гасит и секвенсор, и живой удар; запись в сетку продолжается
  9. Зазоры perf|split и транспорт|pattern
- [ ] PR → squash merge → обновить verify → перезапустить приложение

## Out of scope

- Окно instrument browser: стабильная позиция + запоминание — записано в TECHDEBT #34 (юзер: «позже сделаем»).
- L/T/D/P → реальные SF2-параметры (TECHDEBT #33).
- Stripe-маркеры слева от парт-строк (research-альтернатива) — выбран цвет имени.
- Персист mute слотов в кит — по запросу.
