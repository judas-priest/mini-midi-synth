# Engine-Follow Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-02 (база = origin/master после #56, worktree `~/Projects/mini_midi_synth-verify`)
status: executed 2026-11-02

**Goal:** Браузер показывает оба раздела (DSP-патчи + GM-инструменты); выбор применяет движок автоматически (клик по GM → SF2+program, клик по DSP-патчу → DSP+патч). Кнопка DSP/SF2 в строке парта открывает браузер вместо тоггла.

**Architecture:** `draw_instrument_browser_modal` рисует ОБОИХ дерева (GM-секция только при загруженном SF2); клик пишет в `picked_program`/`picked_patch` + новый флаг `picked_switch_to_sf2: bool`; post-closure применяет sf2_mode по секции. Тумблер DSP/SF2 в парт-строке → открывает браузер.

**Tech Stack:** Rust, egui 0.31.1.

---

## Test Cases (from the requirement)

- TC1 (вручную): парт DSP → в браузере клик по GM-инструменту → парт переключается на SF2, играет выбранный инструмент, браузер открыт.
- TC2 (вручную): парт SF2 → клик по DSP-патчу → парт переключается на DSP, играет патч.
- TC3: DSP-кнопка в строке открывает браузер (не тогглит).
- TC4: нет загруженного SF2 → GM-секция = hint, DSP-секция работает.
- TC5: двойной клик по инструменту → применить + закрыть (существующее поведение сохранено для GM; для DSP-патчей добавить).

## Negative Matrix

- SF2 не загружен — GM-секция скрыта целиком (guard по sf2_keys_soundfont), hint вместо неё.
- клик по текущему патчу/инструменту — no-op (существующий `!selected` guard).
- двойной клик по GM-инструменту — применить + закрыть (существующее поведение).
- pulse/порядок секций — DSP сверху (текущий движок парта первым — osc/партия DSP по умолчанию). Остальное N/A.

---

### Task 1: Браузер — оба дерева + применеие движка по секции

**Files:** Modify: `src/gui/mod.rs` — `draw_instrument_browser_modal` (~:1530-1720)

1. Новый локальный флаг: `let mut picked_switch_to_sf2: Option<bool> = None;` — Some(true) = клик из GM-секции, Some(false) = клик из DSP-секции.
2. Рендер: СЕКЦИЯ DSP ПЕРВАЯ (заголовок-строка «DSP — патчи» жирным + существующее DSP-дерево), ПОТОМ секция «SF2 — GM-инструменты» (заголовок + существующее GM-дерево) — GM-секция только при `sf2_keys_soundfont.is_some()`; иначе на её месте hint «SF2: load a soundfont in Settings».
   - Активный движок парта: его заголовок с маркером (bold, без тофу), другой — обычный.
   - Существующие ветки деревьев НЕ трогать внутри (клики, flat.push, seed) — поменять только пишемое в picked_*: клики в DSP-дереве дополнительно ставят `picked_switch_to_sf2 = Some(false)`, клики в GM-дереве — `Some(true)`.
3. Post-closure: ПЕРЕД применением picked_program/picked_patch:

```rust
        // Движок следует за выбранным звуком (#браузер-фидбек)
        if let Some(sf2) = picked_switch_to_sf2 {
            if self.parts[part].sf2_mode != sf2 {
                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                self.parts[part].sf2_mode = sf2;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: sf2 });
                if part == 0 { self.config.sf2.layer_a_sf2 = sf2; }
                else if part == 1 { self.config.sf2.layer_b_sf2 = sf2; }
                self.persist_parts_state();
            }
        }
```

(вставить сразу после `if let Some(cat) = toggled_cat {...}` блока, до picked_program-обработки.)

4. Двойной клик (существующий close=true в GM-ветке) — оставить; в DSP-ветке добавить так же.

- [ ] Step 1: правки, clippy + cargo test
- [ ] Step 2: commit `feat: browser shows DSP patches and GM instruments — engine follows the pick`

### Task 2: Кнопка DSP/SF2 в строке — открывает браузер

**Files:** Modify: `src/gui/layers.rs` — парт-строка, кнопка DSP/SF2

```rust
            // Кнопка движка: открывает браузер — движок следует за выбором звука
            let eng_label = if sf2 { "SF2" } else { "DSP" };
            if ui.add_sized([44.0, 22.0], egui::Button::new(eng_label).small())
                .on_hover_text(format!("{} — open sound browser", if sf2 { "SF2 engine" } else { "DSP engine" }))
                .clicked()
            {
                self.preset_search.clear();
                self.inst_browser_part = Some(i);
                self.inst_browser_scroll = true;
                self.inst_browser_seeded = false;
            }
```

(заменяет существующий тоггл-блок с AllNotesOff/SetPartSf2Mode/persist — он переезжает в Task 1 post-closure. Существующий вызов браузера из кнопки инструмента (рядом) — не дублировать: там уже `inst_browser_part = Some(i) + scroll + seeded`; объединить или оставить один.)

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `feat: DSP/SF2 button opens the sound browser (engine follows the pick)`

### Task 3: Верификация

- [ ] clippy/test/rt_safety; `cargo run --release`
- [ ] Экран: TC1-TC5
- [ ] PR → squash merge → перезапуск release

## Out of scope

- Удаление sf2_mode-логики тумблера из других мест (LED-консоль, перфы) — только кнопка в строке парта.
- Скрытие секций — обе всегда видны (проще и предсказуемее).
