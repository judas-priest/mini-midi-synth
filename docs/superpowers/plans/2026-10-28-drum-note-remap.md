# Drum Note Remap Implementation Plan

anchor-checked: 2026-10-28 (двойное ревью против репозитория: initial + re-review agents)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Входная нота с пэдов (36–51) переводится в другой внутренний слот ударки через редактируемую в GUI таблицу ремапа, сохраняемую per-SF2-кит (пэды клавы не трогаем).

**Architecture:** Таблица `[u8; 16]` (`remap[in_note-36] = out_slot`) живёт в `DrumEngine` (audio thread, без локов). Перевод — одна строка в `SynthEngine::handle_control` до `record_hit` и диспетчеризации DSP/SF2. Доставка — новый `ControlEvent::DrumSetRemap { table: Box<[u8; 16]> }` (паттерн `DrumLoadKit`/`DrumSetFixedVelocity`). Персист — `Sf2Settings.drums_remaps: HashMap<String, Vec<u8>>` (ключ = имя кита или `"internal"`), `#[serde(default)]`.

**Tech Stack:** Rust, egui (existing GUI), serde, rtrb ControlEvent ring.

**Repo conventions (read first):** `CLAUDE.md` в корне. Коммиты без co-authors. `cargo fmt` по всей базе НЕ запускать. Проверки перед коммитом: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`.

---

## Test Cases (from the requirement)

Requirement: «нажатие пэда (входная нота 36–51) должно играть другой внутренний звук (слот) ударки; таблица правится в GUI и сохраняется per-кит».

- **TC1 Identity:** без ремапа нота 40 → слот 4 (звук не меняется).
- **TC2 Базовый ремап:** `remap[4] = 0` → нота 40 → слот 0 (kick).
- **TC3 Полный диапазон:** `remap[0] = 15` → нота 36 → слот 15.
- **TC4 Вне диапазона:** нота 60 и нота 35 проходят без паники и без изменения (guard в callers остаётся).
- **TC5 Note-off симметрия:** note_off в SF2-пути получает ту же переведённую ноту, что note_on (remap применяется и к off, включая честный 0x8n NoteOff — отдельная ветка `MidiEvent::NoteOff` в mod.rs).
- **TC6 Конфиг старого формата:** config.json без ключа `drums_remaps` загружается (serde default — пустая карта), ремап = identity.
- **TC7 Короткий вектор из конфига:** `drums_remaps` содержит Vec длиной <16 — не паникует, дополняется identity.
- **TC8 Смена кита:** после загрузки другого кита ремап этого кита (или identity) пушится в движок. Автотест не пишется (GUI-уровень, `load_sf2_drums`) — проверяется вручную в Task 7.

## Negative Matrix

- Входная нота вне 36..=51 (60, 35, 0) → покрыто TC4. Внимание: guard живёт внутри `remapped_note` (wrapping_sub + range match), а НЕ в `handle_event` — там для канала 9 guard'а на 36..=51 нет, в drum-ветку попадают любые ch9-ноты; passthrough обеспечивает сам метод.
- Ключ кита отсутствует в `drums_remaps` (новый кит) → identity, при первом редактировании создаётся запись. TC6 покрывает дефолт конфига; TC8 — вручную (Task 7).
- Vec короче 16 из конфига → покрыто TC7.
- Vec длиннее 16 → берутся первые 16 (нормализация в `apply_remap_vec`, покрыто TC7).
- Дублирующий/параллельный `DrumSetRemap` → последнее значение побеждает (кольцевой буфер, единственный writer — GUI). Отдельный тест не нужен, поведение платформы.
- Значение таблицы > 15 (мусор из конфига) → клампится в `apply_remap_vec` (TC7-код, `min(15)`).
- Внешние вызовы/таймауты/часы — N/A (нет внешних вызовов в этом коде).

---

### Task 0: Сохранить план в репозиторий

**Files:**
- Create: `docs/superpowers/plans/2026-10-28-drum-note-remap.md`

- [ ] **Step 1:** Скопировать этот документ в `docs/superpowers/plans/2026-10-28-drum-note-remap.md`.

```bash
cp /home/dima/.claude/plans/zazzy-wiggling-hummingbird.md docs/superpowers/plans/2026-10-28-drum-note-remap.md
git add docs/superpowers/plans/2026-10-28-drum-note-remap.md
git commit -m "docs: drum note remap plan"
```

### Task 1: Тесты ремапа (TDD — failing tests)

**Files:**
- Create: `tests/drum_remap.rs`

- [ ] **Step 1: Написать failing-тесты**

```rust
//! Drum note remap: входная нота -> внутренний слот (см. docs/superpowers/plans/2026-10-28-drum-note-remap.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE, NUM_DRUM_SLOTS};

#[test]
fn tc1_identity_by_default() {
    let eng = DrumEngine::new(48_000.0);
    assert_eq!(eng.remapped_note(40), 40);
}

#[test]
fn tc2_remap_translates_to_other_slot() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[(40 - DRUM_NOTE_BASE) as usize] = 0; // pad note 40 -> kick slot
    assert_eq!(eng.remapped_note(40), DRUM_NOTE_BASE); // 36
}

#[test]
fn tc3_full_range() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[0] = (NUM_DRUM_SLOTS - 1) as u8; // note 36 -> slot 15
    assert_eq!(eng.remapped_note(36), DRUM_NOTE_BASE + 15); // 51
}

#[test]
fn tc4_out_of_range_passthrough_no_panic() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[0] = 3;
    assert_eq!(eng.remapped_note(60), 60);
    assert_eq!(eng.remapped_note(35), 35);
    assert_eq!(eng.remapped_note(0), 0);
}

#[test]
fn tc7_normalize_short_vec() {
    // нормализация вектора из конфига: короткий/длинный не паникует, короткий дополняется identity
    let mut eng = DrumEngine::new(48_000.0);
    eng.apply_remap_vec(&[7, 1]);
    assert_eq!(eng.remapped_note(36), 36 + 7);
    assert_eq!(eng.remapped_note(38), 38); // незаданные позиции = identity
    eng.apply_remap_vec(&[0u8; 20]); // длиннее 16 — берутся первые 16
    assert_eq!(eng.remapped_note(36), 36);
    eng.apply_remap_vec(&[200]); // значение > 15 клампится
    assert_eq!(eng.remapped_note(36), DRUM_NOTE_BASE + 15);
}

#[test]
fn tc6_config_without_remaps_is_default() {
    let s: mini_midi_synth::config::Sf2Settings = serde_json::from_str("{}").unwrap();
    assert!(s.drums_remaps.is_empty());
}

#[test]
fn tc5_on_off_use_same_translation() {
    // note_on и note_off callers вызывают один и тот же remapped_note:
    // переведённая нота для off обязана совпадать с on (иначе SF2-войс зависает)
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[(40 - DRUM_NOTE_BASE) as usize] = 0;
    let on = eng.remapped_note(40);
    let off = eng.remapped_note(40);
    assert_eq!(on, off);
    assert_eq!(on, DRUM_NOTE_BASE);
}
```

- [ ] **Step 2: Убедиться, что тесты не компилируются**

Run: `cargo test --test drum_remap`
Expected: FAIL — `remapped_note`/`apply_remap_vec` не существуют, поле `remap` отсутствует; `config::Sf2Settings` без поля `drums_remaps` тоже FAIL (этот тест станет зелёным в Task 4 — допустимо, финальный прогон в Task 7 обязательный).

- [ ] **Step 3: Commit (红)**

```bash
git add tests/drum_remap.rs
git commit -m "test: drum note remap failing tests"
```
(Коммит красных тестов допустим только если CI не запускается на пуше; иначе объединить с Task 2 шагом commit. В этом репо нет CI-gate на промежуточные коммиты — коммитим.)

### Task 2: DrumEngine.remap + remapped_note + apply_remap_vec

**Files:**
- Modify: `src/synth/drum.rs` (структура `DrumEngine`, ~строка 848; `impl DrumEngine`, ~строка 873)

- [ ] **Step 1: Добавить поле `remap`**

В `pub struct DrumEngine` (после `pub fixed_velocity: bool`, ~строка 853):

```rust
    /// Incoming pad note -> internal slot. remap[note-36] = slot (0..16).
    pub remap: [u8; NUM_DRUM_SLOTS],
```

В `DrumEngine::new` (после `fixed_velocity: false,`):

```rust
            remap: {
                let mut r = [0u8; NUM_DRUM_SLOTS];
                for (i, v) in r.iter_mut().enumerate() { *v = i as u8; }
                r
            },
```

- [ ] **Step 2: Добавить методы**

В `impl DrumEngine` (рядом с `note_on`, ~строка 939):

```rust
    /// Translate an incoming pad note to its target GM note (DRUM_NOTE_BASE + slot).
    /// Notes outside the drum range pass through unchanged.
    pub fn remapped_note(&self, note: u8) -> u8 {
        match note.wrapping_sub(DRUM_NOTE_BASE) {
            i if (i as usize) < NUM_DRUM_SLOTS => DRUM_NOTE_BASE + self.remap[i as usize],
            _ => note,
        }
    }

    /// Load a remap table from config (short vecs are padded with identity, long truncated).
    pub fn apply_remap_vec(&mut self, v: &[u8]) {
        for (i, slot) in self.remap.iter_mut().enumerate() {
            *slot = v.get(i).copied().unwrap_or(i as u8).min((NUM_DRUM_SLOTS - 1) as u8);
        }
    }
```

- [ ] **Step 3: Прогнать тесты**

Run: `cargo test --test drum_remap`
Expected: PASS (5 тестов).

- [ ] **Step 4: RT-safety и полный прогон**

Run: `cargo test --features rt-assert --test rt_safety && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/synth/drum.rs tests/drum_remap.rs
git commit -m "feat: drum engine note remap table (identity default)"
```

### Task 3: ControlEvent + перевод в handle_event

**Files:**
- Modify: `src/synth/mod.rs` (enum ~строка 210; drum-ветка `handle_event` ~строки 1569-1596; `handle_control` ~строка 2077; NoteOff-ветка ~строки 1646-1652)

- [ ] **Step 1: Добавить вариант ControlEvent**

После `DrumSetFixedVelocity { fixed: bool },` (~строка 210):

```rust
    /// Drums: pad-note -> internal slot table (index = in_note-36, value = slot)
    DrumSetRemap { table: Box<[u8; 16]> },
```

- [ ] **Step 2: Arm в handle_control (с возвратом через trash)**

Рядом с `DrumSetFixedVelocity` (~строка 2077). **Box нельзя дропать в audio-потоке** — вернуть потраченный event как `LoadPatch`/`DrumLoadKit` (mod.rs:1907/2027): callback освобождает через `trash.dispose(Trash::Control(...))`:

```rust
            ControlEvent::DrumSetRemap { table } => {
                self.drum_engine.apply_remap_vec(&*table);
                return Some(ControlEvent::DrumSetRemap { table });
            }
```

(`apply_remap_vec` заодно клампит значения > 15.)

- [ ] **Step 3: Перевод в drum-ветке note-on/off**

В `handle_event`, ветка `if velocity > 0 {` — сразу после строки `let velocity = if self.drum_engine.fixed_velocity ...` (~строка 1570) добавить:

```rust
                        let note = self.drum_engine.remapped_note(note);
```

И в `} else {` ветке velocity==0 (~строка 1590) добавить той же первой строкой:

```rust
                        let note = self.drum_engine.remapped_note(note);
```

**Отдельно — честный note-off (статус 0x8n):** в ветке `MidiEvent::NoteOff`, `if channel == 9` (`sampler.drum_note_off` / `drum_engine.note_off`, ~строки 1646-1652) добавить первой строкой:

```rust
                        let note = self.drum_engine.remapped_note(note);
```

Без этого SF2-войс, запущенный переведённой нотой, не получит 0x80 по ней же и зависнет.

Важно: перевод стоит ДО `record_hit` — секвенсор пишет целевой слот (звук, который прозвучал).

- [ ] **Step 4: Прогнать всё**

Run: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/synth/mod.rs
git commit -m "feat: apply drum remap to incoming pad notes (dsp + sf2 paths, on/off)"
```

### Task 4: Персист в конфиге (per-кит)

**Files:**
- Modify: `src/config.rs` (struct `Sf2Settings`, ~строки 27-59)

- [ ] **Step 1: Добавить поле**

В `Sf2Settings` рядом с `drums_sf2` (~строка 52):

```rust
    /// Pad-note remap tables per drums kit (key = kit file name or "internal"), len 16.
    #[serde(default)]
    pub drums_remaps: std::collections::HashMap<String, Vec<u8>>,
```

- [ ] **Step 2: Прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS (старый config.json без ключа грузится — serde default).

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat: persist per-kit drum remap tables in config"
```

### Task 5: GUI — состояние, доставка, персист

**Files:**
- Modify: `src/gui/mod.rs` (App fields ~346-350; reconnect push ~1081; `send_drum_state` ~1389; `load_sf2_drums` ~1458; `unload_sf2_drums` ~1501)

- [ ] **Step 1: Поле App + хелпер ключа**

В `pub struct App` рядом с `drum_params`:

```rust
    pub drum_remap: [u8; 16],
```

(Инициализация в `App::new`: `drum_remap: { let mut r = [0u8; 16]; for (i, v) in r.iter_mut().enumerate() { *v = i as u8; } r },`)

Хелпер в `impl App`:

```rust
    fn drum_remap_key(&self) -> String {
        if self.sf2_drums_loaded_name.is_empty() { "internal".to_string() }
        else { self.sf2_drums_loaded_name.clone() }
    }

    fn push_drum_remap(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetRemap { table: Box::new(self.drum_remap) });
    }

    fn save_drum_remap(&mut self) {
        let key = self.drum_remap_key();
        self.config.sf2.drums_remaps.insert(key, self.drum_remap.to_vec());
        let _ = self.config.save();
    }
```

- [ ] **Step 2: Доставка при старте/reconnect / send_drum_state**

В `send_drum_state` после строки `DrumSetFixedVelocity` (~1389):

```rust
        self.push_drum_remap();
```

В `send_initial_presets` (~1081, вызывается и при reconnect через settings.rs:275) рядом с re-push `DrumSetFixedVelocity`: та же строка. Порядок с load_sf2_drums корректен сам по себе: FIFO-кольцо, поздний per-kit push из load_sf2_drums побеждает; дубли push безвредны (последний wins).

- [ ] **Step 3: Загрузка ремапа при смене кита**

В `load_sf2_drums` (после установки `sf2_drums_loaded_name`, до/после re-push `SetDrumsSf2Mode`):

```rust
        if let Some(v) = self.config.sf2.drums_remaps.get(&self.drum_remap_key()) {
            let mut t = [0u8; 16];
            for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(i as u8).min(15); }
            self.drum_remap = t;
        } else {
            for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = i as u8; }
        }
        self.push_drum_remap();
```

В `unload_sf2_drums` (после сброса `sf2_drums_loaded_name`):

```rust
        for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = i as u8; }
        self.push_drum_remap();
```

- [ ] **Step 4: Прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/gui/mod.rs
git commit -m "feat: gui drum remap state, per-kit load/save, reconnect push"
```

### Task 6: GUI — панель ремапа в drums.rs

**Files:**
- Modify: `src/gui/drums.rs` (после блока V127, ~строка 77)

- [ ] **Step 1: UI — 16 комбо «Pad → звук» + Reset**

Важно: `self.drum_remap` и методы `self` нельзя заимствовать одновременно — изменения собираются в локальную копию `new_remap` и применяются одним блоком.

После `});` закрывающего первую `ui.horizontal` (после V127, ~строка 77) вставить:

```rust
        // Pad remap: incoming pad note -> internal slot
        ui.horizontal(|ui| {
            ui.label("Remap:");
            let mut new_remap = self.drum_remap;
            for (i, slot) in new_remap.iter_mut().enumerate() {
                let pad_note = 36 + i as u8;
                let sel = crate::gui::drum_slot_name(*slot);
                egui::ComboBox::from_label(format!("P{}", pad_note))
                    .width(74.0)
                    .selected_text(sel)
                    .show_ui(ui, |ui| {
                        for (s, name) in crate::gui::drum_slot_names().iter().enumerate() {
                            ui.selectable_value(slot, s as u8, *name);
                        }
                    });
            }
            if new_remap != self.drum_remap {
                self.drum_remap = new_remap;
                self.push_drum_remap();
                self.save_drum_remap();
            }
            if ui.button("Reset").clicked() {
                for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = i as u8; }
                self.push_drum_remap();
                self.save_drum_remap();
            }
        });
```

Хелперы имён — в `src/gui/mod.rs` (или `src/gui/drums.rs`, если уже есть доступ к `drum::DRUM_NAMES` через импорт; проверить существующие импорты drums.rs — там уже используется `DrumKit` из preset, значит путь `crate::synth::drum` доступен):

```rust
pub fn drum_slot_names() -> [&'static str; 16] { crate::synth::drum::DRUM_NAMES }
pub fn drum_slot_name(slot: u8) -> &'static str { crate::synth::drum::DRUM_NAMES[slot as usize] }
```

(Если `use crate::synth::drum;` уже есть в drums.rs — вызывать `drum::DRUM_NAMES` напрямую, хелперы не добавлять.)

- [ ] **Step 2: Прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/gui/drums.rs src/gui/mod.rs
git commit -m "feat: pad remap UI in drum panel (16 combo boxes + reset)"
```

### Task 7: Финальная верификация

- [ ] **Step 1: Полный набор проверок**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```
Expected: всё PASS.

- [ ] **Step 2: Ручная проверка (железо)**

Запустить `cargo run --release`, загрузить SF2 drums (Metal_Drums_v2), в панели ударки поставить P40 → Kick, нажать pad-1 на клаве: слышен kick, в секвенсоре пишется слот Kick. Перезапустить приложение: ремап восстановился (config.json `sf2.drums_remaps`). Смена кита (Task 5 Step 3) — тоже вручную: ремапы разных китов не должны смешиваться. Проверить note-off (TC5): зажать пэд на SF2-звуке с лупом в сэмпле и отпустить — войс не должен зависать (off уходит по переведённой ноте).

Известное ограничение (pre-existing, не чиним здесь): `DrumEngine::set_sample_rate` делает `*self = Self::new(...)` и сбросит ремап (как и паттерны/params) при смене sample rate устройства до reconnect-push.

- [ ] **Step 3: Финальный коммит (если что-то осталось)**

```bash
git status --short
# закоммитить остатки одним коммитом "fix: drum remap followups" если есть
```
