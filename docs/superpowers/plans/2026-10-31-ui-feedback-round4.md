# UI Feedback Round 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-10-31 (база 8add062, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** L/T работают на SF2-китах (P/D честно DSP-only); строка партов — виджеты на одной высоте; tofu-квадрат «✕» → «×».

**Architecture:** GUI (`src/gui/`) + применение params в SF2-ветке потребления триггеров (`src/synth/mod.rs`, уже имеет доступ к `drum_engine.params`). Работа в одном worktree, интеграция PR-ом.

**Tech Stack:** Rust, egui 0.31.1. Факты по сэмплеру: `drum_note_on(note, vel)` = raw MIDI ch.9 → **T** = транспозиция note, **L** = масштаб velocity; **P** — CC10 пер-channel, канал барабанов один → в SF2 невозможно; **D** — release_scale глобальный → в SF2 невозможно.

---

## Test Cases (from the requirement)

- TC1 (визуально): в парт-строке все элементы на одной высоте при любых значениях.
- TC2 (визуально): «Split: ON ×» без tofu-квадрата.
- TC3 (на слух/визуально): SF2-кит + T-слайдер — питч удара меняется; L — громкость; P/D — серые с hover-причиной; строки-подсказки «DSP-engine only» больше нет.

## Negative Matrix

- zero/empty — L=0 → velocity 0 → удара нет: осознанно (уровень нуль = тишина); clamp 1..=127 НЕ применять при L=0 (иначе не заглушить). Код: `if level<=0 { skip note_on }`.
- duplicate/concurrent, clock skew, missing field, external timeout — N/A.

---

### Task 1: Строка партов — единая высота виджетов (п. «элементы гуляют по высоте»)

**Files:**
- Modify: `src/gui/layers.rs` — `draw_part_row`

Все `add_sized([w, 0.0], …)` в строке → `add_sized([w, 22.0], …)` (константа `ROW_H: f32 = 22.0;` в начале `draw_part_row`): label-button, engine-кнопка, instrument-кнопка, Mute/Root-чекбоксы, Pan/Tr/Volume-слайдеры, Vel-драгзначения, «✕»-кнопка удаления. Разные авто-высоты центрировались по-разному — отсюда вертикальный разброс.

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `fix: part-row widgets share one height — no vertical drift`

### Task 2: Tofu «✕» → «×» (U+00D7)

**Files:**
- Modify: `src/gui/layers.rs` — «Split: ON ✕» и small_button("✕") удаления парта
- Modify: `src/gui/mod.rs` — кнопка очистки поиска в instrument browser («\u{2715}»)

Grep `\u{2715}`/`✕` по src/gui → заменить на `"×"`. (Скриншот: U+2715 отсутствует в шрифте — рисуется квадратом.)

- [ ] Step 1: правки, clippy
- [ ] Step 2: commit `fix: replace U+2715 (missing glyph) with U+00D7`

### Task 3: L и T работают на SF2-китах; P/D честно DSP-only; повторяющуюся строку-подсказку убрать

**Files:**
- Modify: `src/synth/mod.rs` — SF2-ветка потребления `sf2_triggers` (~3414-3425)
- Modify: `src/gui/drums.rs` — per-slot controls: убрать строку-подсказку (она рисовалась 16 раз), L/T активны всегда, P/D — только в DSP

**Движок.** В SF2-ветке на каждый удар применять `drum_engine.params[slot]`:

```rust
for i in 0..sf2.count {
    let (slot, vel) = sf2.triggers[i];
    let p = &self.drum_engine.params[slot as usize];
    // L: масштаб velocity (0 = тишина); T: транспозиция в полутонах
    if p.level <= 0.0 { continue; }
    let vel = ((vel as f32 * p.level).round() as u8).clamp(1, 127);
    let note = (base_note as i32 + p.tune.round() as i32).clamp(0, 127) as u8;
    self.sampler.drum_note_on(note, vel);
}
```

(`base_note` — фактическое вычисление note в существующем коде: remap/choke — сохранить логику, добавить только level/tune; свериться с текущим телом перед правкой.)

**GUI** (drums.rs, per-slot):

- удалить блок `if !dsp_kit { ui.label(... "L T D P: DSP-engine only ...") }` (он повторялся на каждой из 16 строк — скриншот);
- L-слайдер: `add_enabled(true)` (убрать dsp_kit), hover-хвост для L убрать;
- T-слайдер: аналогично всегда активен;
- P и D: остаются `add_enabled(dsp_kit)`, hover-хвост `{dsp_hint}` сохраняется — единственное место, где говорится про SF2;
- Mute-чекбокс без изменений (работает всюду).

- [ ] Step 1: движок, cargo test + rt_safety
- [ ] Step 2: GUI, clippy + cargo test
- [ ] Step 3: commit `feat: Level and Tune drive SF2 drum hits; P/D stay DSP-only, drop repeated hint line`

### Task 4: Верификация и запуск

- [ ] clippy / cargo test / rt_safety
- [ ] Пересобрать verify из смерженного master, запустить: TC1-TC3 на экране
- [ ] PR → squash merge → перезапуск

## Out of scope

- P/D в SF2 — упирается в rustysynth (один drum-канал для CC10; глобальный release scale). TECHDEBT #33 обновить формулировкой «L/T реализованы, P/D ограничение rustysynth».
- «Каша» в редакторе параметров — Task 9 плана round 3, ждёт скриншот.
