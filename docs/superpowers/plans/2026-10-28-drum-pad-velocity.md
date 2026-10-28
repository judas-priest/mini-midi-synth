# Drum Per-Pad Velocity Override Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Per-пэд velocity-оверрайд: фиксированная сила удара (1–127) или Off на каждый пэд, сохраняется per-SF2-кит, правится в Remap-модалке.

**Architecture:** Таблица `pad_vel: [u8; 16]` (0 = Off) в `DrumEngine`, индексируется по входной ноте (note-36), применяется в `handle_event` ПОСЛЕ глобального триггера V127 и ДО ремапа/`record_hit` — секвенсор пишет уже эффективную велосити. Доставка — `ControlEvent::DrumSetPadVel { table: Box<[u8; 16]> }` с trash-паттерном (как `DrumSetRemap`). Персист — `Sf2Settings.drums_vels: HashMap<String, Vec<u8>>` (ключ = имя кита, как у `drums_remaps`), GUI — DragValue «Vel» в обеих remap-модалках.

**Tech Stack:** Rust, egui, serde, rtrb — без новых зависимостей.

**Приоритет велосити (итог):** явно заданный пэд-оверрайд (1..=127) > глобальный V127-триггер > сырое значение с пэда.

---

## Test Cases (from the requirement)

Requirement: «per-пэд velocity-оверрайд 1–127 или Off; Off = текущее поведение (V127-триггер или сырое значение); оверрайд сильнее триггера; секвенсор пишет с оверрайдом».

- **TC1 Off по умолчанию:** `pad_velocity(40, 57) == 57` на свежем движке (passthrough).
- **TC2 Оверрайд:** `pad_vel[4] = 100` → `pad_velocity(40, 30) == 100` (пришло 30, стало 100).
- **TC3 Изоляция пэдов:** `pad_vel[4] = 100` не влияет на соседний пэд: `pad_velocity(41, 33) == 33`.
- **TC4 Off после сброса:** `pad_vel[4] = 0` → снова passthrough.
- **TC5 Нормализация из конфига:** короткий Vec дополняется нулями (Off), длинный усекается, значения >127 клампятся до 127.
- **TC6 Оверрайд работает при включённом V127:** контракт caller'а — caller применяет `pad_velocity` ПОСЛЕ расчёта fixed_velocity; тест проверяет, что метод сам не зависит от `fixed_velocity` (движок не знает о триггере).

## Negative Matrix

- Нота вне 36..=51 в `pad_velocity` → guard, passthrough (как в `remapped_note`); канал 9 пропускает любые ноты, guard обязателен в методе.
- Vel-значение 200 из конфига → клампится до 127 (`apply_pad_vel_vec`, TC5).
- Vec короче/длиннее 16 → TC5.
- Взаимодействие с `fixed_velocity` → caller-порядок (fixed → pad override); тест TC6 фиксирует независимость метода.
- Velocity == 0 входящая (нота-off как velocity-0 NoteOn) → оверрайд применяется и к ней? Нет: velocity==0 ветка обрабатывается как note-off ДО vel-расчёта — оверрайд не участвует (ветка не доходит до строки).
- Дублирующий push → последний wins, платформенное поведение.
- Внешние вызовы/таймауты/часы — N/A.

---

### Task 0: Сохранить план в репозиторий

- [ ] **Step 1:** План уже создаётся в `docs/superpowers/plans/2026-10-28-drum-pad-velocity.md` (worktree). Закоммитить:

```bash
git add docs/superpowers/plans/2026-10-28-drum-pad-velocity.md
git commit -m "docs: drum pad velocity override plan"
```

### Task 1: Тесты (TDD — failing tests)

**Files:**
- Create: `tests/drum_pad_vel.rs`

- [ ] **Step 1: Написать failing-тесты**

```rust
//! Per-пэд velocity-оверрайд ударки (см. docs/superpowers/plans/2026-10-28-drum-pad-velocity.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE};

#[test]
fn tc1_default_passthrough() {
    let e = DrumEngine::new(48_000.0);
    assert_eq!(e.pad_velocity(40, 57), 57);
    assert_eq!(e.pad_velocity(51, 1), 1);
}

#[test]
fn tc2_override_applies() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    assert_eq!(e.pad_velocity(40, 30), 100);
}

#[test]
fn tc3_override_isolated_per_pad() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    assert_eq!(e.pad_velocity(41, 33), 33);
}

#[test]
fn tc4_zero_returns_to_passthrough() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 0;
    assert_eq!(e.pad_velocity(40, 30), 30);
}

#[test]
fn tc5_normalize_from_config() {
    let mut e = DrumEngine::new(48_000.0);
    e.apply_pad_vel_vec(&[100, 127]);
    assert_eq!(e.pad_velocity(36, 10), 100);
    assert_eq!(e.pad_velocity(37, 10), 127);
    assert_eq!(e.pad_velocity(38, 10), 10);  // незаданные = Off
    e.apply_pad_vel_vec(&[5u8; 20]);          // длиннее 16 — берутся первые 16
    assert_eq!(e.pad_velocity(36, 10), 5);
    e.apply_pad_vel_vec(&[250]);              // >127 клампится
    assert_eq!(e.pad_velocity(36, 10), 127);
}

#[test]
fn tc6_independent_of_fixed_velocity_flag() {
    // оверрайд сам по себе; caller применяет его ПОСЛЕ fixed_velocity
    let mut e = DrumEngine::new(48_000.0);
    e.fixed_velocity = true;
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 64;
    assert_eq!(e.pad_velocity(40, 30), 64);
}

#[test]
fn tc7_out_of_range_passthrough() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    assert_eq!(e.pad_velocity(60, 40), 40);
    assert_eq!(e.pad_velocity(35, 40), 40);
    assert_eq!(e.pad_velocity(0, 40), 40);
}
```

- [ ] **Step 2: Убедиться, что не компилируется**

Run: `cargo test --test drum_pad_vel`
Expected: FAIL — `pad_velocity`/`apply_pad_vel_vec`/`pad_vel` не существуют.

- [ ] **Step 3: Commit**

```bash
git add tests/drum_pad_vel.rs
git commit -m "test: drum pad velocity override failing tests"
```

### Task 2: DrumEngine — pad_vel, pad_velocity, apply_pad_vel_vec

**Files:**
- Modify: `src/synth/drum.rs` (struct `DrumEngine` — после `pub remap`; impl — рядом с `remapped_note`)

- [ ] **Step 1: Поле** — после `pub remap: [u8; NUM_DRUM_SLOTS],`:

```rust
    /// Per-pad velocity override, indexed by incoming note - 36.  0 = Off
    /// (use incoming/master velocity).
    pub pad_vel: [u8; NUM_DRUM_SLOTS],
```

В `DrumEngine::new` (после `remap: ...`):

```rust
            pad_vel: [0u8; NUM_DRUM_SLOTS],
```

- [ ] **Step 2: Методы** — в `impl DrumEngine` (рядом с `remapped_note`):

```rust
    /// Per-pad velocity override for an incoming pad note.  Values outside
    /// 36..=51 pass through; 0 override = passthrough.
    pub fn pad_velocity(&self, note: u8, vel: u8) -> u8 {
        match note.wrapping_sub(DRUM_NOTE_BASE) {
            i if (i as usize) < NUM_DRUM_SLOTS && self.pad_vel[i as usize] > 0 => {
                self.pad_vel[i as usize]
            }
            _ => vel,
        }
    }

    /// Load pad-velocity overrides from config (short vecs padded with 0 = Off,
    /// long truncated, values clamped to 1..=127 — 0 stays Off).
    pub fn apply_pad_vel_vec(&mut self, v: &[u8]) {
        for (i, vel) in self.pad_vel.iter_mut().enumerate() {
            *vel = v.get(i).copied().unwrap_or(0).min(127);
        }
    }
```

- [ ] **Step 3: Прогнать**

Run: `cargo test --test drum_pad_vel`
Expected: PASS (7 тестов).

- [ ] **Step 4: RT-safety + clippy**

Run: `cargo clippy --all-targets -- -D warnings && cargo test --features rt-assert --test rt_safety`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/synth/drum.rs tests/drum_pad_vel.rs
git commit -m "feat: drum engine per-pad velocity override"
```

### Task 3: ControlEvent + применение в handle_event

**Files:**
- Modify: `src/synth/mod.rs` (enum ~212; handle_control ~2090; handle_event ~1570)

- [ ] **Step 1: Вариант enum** — после `DrumSetChokeGroups { table: Box<[u8; 26]> },`:

```rust
    /// Drums: per-pad velocity override (index = in_note-36, 0 = off)
    DrumSetPadVel { table: Box<[u8; 16]> },
```

- [ ] **Step 2: Arm в handle_control** (trash-паттерн, рядом с DrumSetRemap/DrumSetChokeGroups):

```rust
            ControlEvent::DrumSetPadVel { table } => {
                self.drum_engine.apply_pad_vel_vec(&table);
                return Some(ControlEvent::DrumSetPadVel { table });
            }
```

- [ ] **Step 3: Применение в handle_event** — в drum-ветке, между строкой fixed_velocity и строкой remap:

```rust
                        let velocity = if self.drum_engine.fixed_velocity { 127 } else { velocity };
                        let velocity = self.drum_engine.pad_velocity(note, velocity);
                        let note = self.drum_engine.remapped_note(note);
```

(вторая строка — добавляемая; строка remap уже существует. Порядок: оверрайд по ВХОДНОЙ ноте, до record_hit — секвенсор пишет эффективную велосити.)

- [ ] **Step 4: Прогнать всё**

Run: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/synth/mod.rs
git commit -m "feat: apply per-pad velocity override in drum note path"
```

### Task 4: Конфиг — drums_vels per-кит

**Files:**
- Modify: `src/config.rs` (Sf2Settings, рядом с `drums_remaps`)

- [ ] **Step 1: Поле**

```rust
    /// Per-pad velocity overrides per drums kit (key = kit file name or
    /// "internal"), len 16, 0 = off.
    #[serde(default)]
    pub drums_vels: std::collections::HashMap<String, Vec<u8>>,
```

- [ ] **Step 2: Прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat: persist per-kit drum velocity overrides in config"
```

### Task 5: GUI — состояние, доставка, персист

**Files:**
- Modify: `src/gui/mod.rs` (App-поле рядом с `drum_remap`; helpers рядом с `push_drum_remap`; `send_drum_state` ~1472; `send_initial_presets` ~1069; `load_sf2_drums` ~1501; `unload_sf2_drums` ~1554; актуальные номера см. grep
- Modify: `src/lib.rs` (App-литерал ~744)

- [ ] **Step 1: Поля и хелперы**

В `pub struct App` после `pub drum_remap: [u8; 16],`:

```rust
    pub drum_pad_vel: [u8; 16],
```

В `src/lib.rs` App-литерал после `drum_remap: ...`:

```rust
        drum_pad_vel: [0u8; 16],
```

В `impl App` (рядом с `push_drum_remap`/`save_drum_remap`; они private — child-модуль drums.rs имеет доступ, как и эти):

```rust
    fn push_drum_pad_vel(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetPadVel { table: Box::new(self.drum_pad_vel) });
    }

    fn save_drum_pad_vel(&mut self) {
        let key = self.drum_remap_key();
        self.config.sf2.drums_vels.insert(key, self.drum_pad_vel.to_vec());
        let _ = self.config.save();
    }
```

- [ ] **Step 2: Доставка** — `self.push_drum_pad_vel();` добавить:
  - в `send_drum_state` после `self.push_drum_remap();` (актуально ~1482)
  - в `send_initial_presets` после `self.push_drum_remap();` (актуально ~1102)
  - и в `send_initial_presets` ПЕРЕД первым push — восстановление internal-кита
    (зеркало блока `drums_remaps["internal"]` для ремапа на ~1095-1101):

```rust
        if self.sf2_drums_loaded_name.is_empty() {
            if let Some(v) = self.config.sf2.drums_vels.get("internal") {
                let mut t = [0u8; 16];
                for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(0).min(127); }
                self.drum_pad_vel = t;
            }
        }
```

- [ ] **Step 3: Загрузка при смене кита** — в `load_sf2_drums` после блока ремапа (перед `self.push_drum_remap();`):

```rust
        if let Some(v) = self.config.sf2.drums_vels.get(&self.drum_remap_key()) {
            let mut t = [0u8; 16];
            for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(0).min(127); }
            self.drum_pad_vel = t;
        } else {
            self.drum_pad_vel = [0u8; 16];
        }
        self.push_drum_pad_vel();
```

В `unload_sf2_drums` (рядом с identity-ремапом):

```rust
        self.drum_pad_vel = [0u8; 16];
        self.push_drum_pad_vel();
```

- [ ] **Step 4: Прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/gui/mod.rs src/lib.rs
git commit -m "feat: gui drum pad-vel state, per-kit load/save, delivery"
```

### Task 6: GUI — Vel-контролы в remap-модалках

**Files:**
- Modify: `src/gui/drums.rs` (полная модалка `pad_row`, ~строки 94-116; однопэдная модалка ~строки 139-172)

- [ ] **Step 1: Полная модалка — DragValue «Vel» под каждым комбо**

В `pad_row` внутри `ui.vertical_centered` (после ComboBox, до закрывающей скобки):

```rust
                                let mut vel = new_vels[(note - 36) as usize];
                                ui.add(egui::DragValue::new(&mut vel)
                                    .range(0..=127)
                                    .custom_formatter(|v, _| if v <= 0.0 { "Off".into() } else { format!("{}", v as i32) })
                                    .max_width(60.0));
                                new_vels[(note - 36) as usize] = vel;
```

Важно: замыкание `pad_row` ужеCaptures `new_remap`; для `drum_pad_vel` — тот же паттерн: локальная копия `new_vels`, изменения сравниваются и применяются одним блоком после циклов:

```rust
                let mut new_vels = self.drum_pad_vel;
```

в `pad_row` использовать `new_vels[(note - 36) as usize]`, после `pad_row(...)` вызовов и до Reset-кнопки:

```rust
                if new_vels != self.drum_pad_vel {
                    self.drum_pad_vel = new_vels;
                    self.push_drum_pad_vel();
                    self.save_drum_pad_vel();
                }
```

Reset-кнопке добавить сброс велосити: `self.drum_pad_vel = [0u8; 16];` + push + save.

- [ ] **Step 2: Однопэдная модалка** — после ComboBox «Instrument» добавить:

```rust
                ui.add_space(theme::SP_MD);
                ui.label("Velocity (Off = dynamic):");
                let mut vel = self.drum_pad_vel[idx];
                ui.add(egui::DragValue::new(&mut vel)
                    .range(0..=127)
                    .custom_formatter(|v, _| if v <= 0.0 { "Off".into() } else { format!("{}", v as i32) })
                    .max_width(80.0));
                if vel != self.drum_pad_vel[idx] {
                    self.drum_pad_vel[idx] = vel;
                    self.push_drum_pad_vel();
                    self.save_drum_pad_vel();
                }
```

(`idx` уже вычислен в этой модалке из factory-note пэда.)

- [ ] **Step 3: Прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/gui/drums.rs
git commit -m "feat: per-pad velocity drag in remap dialogs"
```

### Task 7: Финальная верификация

- [ ] **Step 1:** `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety` — всё зелёное.

- [ ] **Step 2: Ручная проверка:** запустить приложение, SF2 Drums (Metal_Drums_v2), в Remap-модалке Pad 12 (Hi Hat) → Vel 100; играть пэд 12 — каждый удар одной силы; включить/выключить глобальный V127 — Pad 12 всё равно 100, остальные пэды реагируют на V127. Перезапуск — Vel восстановился (config `sf2.drums_vels`). Секвенсор: записать хит с оверрайдом — в паттерне велосити = 100.

- [ ] **Step 3: Commit остатков** (если есть):

```bash
git status --short
git commit -am "fix: pad velocity followups" || true
```
