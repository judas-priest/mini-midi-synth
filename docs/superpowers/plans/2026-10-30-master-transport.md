# Master Transport: Drums + Looper Unified Control (#32, option A)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Один мастер-транспорт (Play/Rec/Stop) управляет драм-секвенсором и лупером ВМЕСТЕ: Master Rec армит запись драм-паттерна + запись лупера (Mix — все части), Master Play стартует оба, Master Stop останавливает оба с дрена́жем нот. Запись перформанса = лупер (он уже пишет ноты всех частей с квантом; выбор юзера 2026-10-30, вариант А).

**Architecture:** Три метода на `SynthEngine` (`master_toggle_play` / `master_stop` / `master_toggle_record`) — чистая композиция существующих `drum_engine.set_playing/set_recording` и `looper.toggle_play/toggle_record`. Новые `ControlEvent::MasterPlay/MasterStop/MasterRecord` (без heap-полей, return None). `seq_target` получает третье значение 2=MASTER (новый дефолт); пэды 82/83/81 и клавиши 123/124 при target=2 идут в мастер; 0/1 — прежняя семантика. GUI: строка Master в docked-панели + LED-консоль (цикл целей 0→1→2, цвета). Питч-секвенсоры НЕ трогаем (живой модулятор).

**Tech Stack:** Rust, rtrb ControlEvent, существующие атомы; egui-строка в gui/drums.rs.

**Отложено в TECHDEBT (в конце плана):** standalone-реплей питч-секв (вариант Б), упрощение pad-меню (#32 фаза 4), переходы паттернов по границе такта (фаза 5), docked-панель лупер+секвенсор с тогглом (#30 — отдельный план).

---

## Test Cases (from the requirement)

Requirement (TECHDEBT #32 фаза 1-2, сужено до варианта А): «Play/Rec/Stop управляют драмами + лупером вместе; Rec армит запись обоих; всё ходит по существующей сетке».

1. **TC1 Master Rec армит оба:** `MasterRecord` при остановленных драмах и Idle-лупере → `drum_engine.sequencer.recording == true` + `playing == true` (auto-play), лупер в `Recording`.
2. **TC2 Master Rec пишет в оба:** при армированном мастере нота ch9 40 пишет шаг драм-паттерна (существующий `record_hit`), нота ch0 60 создаёт событие лупера (`event_count > 0`).
3. **TC3 Master Rec off завершает оба:** `MasterRecord` второй раз → драмы `recording == false` (играют дальше), лупер вышел из Recording (Playing).
4. **TC4 Master Play toggle:** `MasterPlay` → драмы играют, лупер (после записи петли, Idle) → Playing. Повторный `MasterPlay` → драмы остановлены + `current_step == 0`, лупер Idle.
5. **TC5 Master Stop из записи:** при Recording обоих `MasterStop` → драмы `!playing`, step 0; лупер Idle (запись завершилась, потом stop; физический дренаж нот — на следующем `tick()`, как в ручном Stop — вне харнесса теста).
6. **TC6 Пэды на target=2:** 83 (ch9) → master rec arm; 82 → master play toggle; 81 → master stop. Клавиша 123 (ch0) → master play toggle.
7. **TC7 Регрессия target 0/1:** все тесты `tests/console_transport.rs` остаются зелёными БЕЗ правок (они ставят target явно).
8. **TC8 Дефолт:** свежий `SynthEngine::new` имеет `seq_target == 2`; ch9-пэды 36-51 продолжают играть ударку вживую при target=2 (ветка `channel == 9` не завязана на target).
9. **TC9 Idle-лупер не стартует от Master Stop:** `MasterStop` на Idle-лупере оставляет Idle (не запускает).

## Negative Matrix

- **Лупер пустой при Master Play** → Playing без событий: ничего не звучит, безвредно (как ручной Play пустой петли сегодня). TC4 пишет петлю перед проверкой звука; сам факт Playing проверяется.
- **Драмы играют, лупер Idle (расхождение)** → Master Play: `master_toggle_play` смотрит на ANY-состояние: если хоть что-то играет — это Stop. Запуск расхождения = Master Stop сначала. Задокументировано как семантика toggle-кнопки (одна кнопка — одно состояние).
- **Лупер Recording при Master Play (toggle → stop)** → сначала `toggle_record` (finish), потом stop — TC5. Не теряет слой.
- **Двойной MasterRecord подряд** (двойное нажатие) → второй вызов снимает арм (toggle). Платформенно: rtrb FIFO, порядок сохраняется. N/A.
- **seq_target=2 и старые GUI-записи 1** → все `.store(1)` на `seq_target_atom` в GUI заменяются на 2 (Task 3); пропущенное место = консоль возвращается из master в looper-режим — ловится grep-шагом.
- **Дрейф clocks** — лупер уже синхронизируется per-block (`sync_clock`, S5); мастер не вводит новых часов. N/A.
- **RT-safety:** master-методы — композиция существующих RT-safe вызовов. Известная ОСОЗНАННАЯ страховка: arm-путь `set_recording(true)` → `save_snapshot()` аллоцирует ~4KB (Box паттернов) на аудиопотоке при пэде 83 — это ПРЕ-ЭКСИСТИНГ долг (идентично DrumSeqRecord/pad 83 target 0), не расширяется этим планом; фикс — отдельный QW-пункт. Проверяется `rt_safety` (он этот путь не тестирует).

---

### Task 1: Движок — master-методы, события, роутинг

**Files:**
- Modify: `src/synth/mod.rs` (enum ControlEvent ~203-284; impl SynthEngine — рядом с looper-хендлерами ~2373-2440; `seq_target` init ~1204; pad/keys роутинг 1783-1829)

- [x] **Step 1: Три варианта ControlEvent.** Рядом с `LooperUndo` (~225):

```rust
    /// Master transport (#32): toggle play drums + looper together
    MasterPlay,
    /// Master transport: stop drums + looper (drain note-offs)
    MasterStop,
    /// Master transport: arm/disarm recording on drums + looper
    MasterRecord,
```

- [x] **Step 2: Методы SynthEngine** (в `impl SynthEngine`, рядом с `record_for_looper` ~1676). Понадобится `use` — путь `LooperState` уже резолвится в mod.rs (`crate::synth::looper::LooperState`):

```rust
    /// Master transport (#32): start both transports (drums + looper).
    fn master_start(&mut self) {
        self.drum_engine.set_playing(true);
        if matches!(self.looper.state, crate::synth::looper::LooperState::Idle) {
            self.looper.toggle_play();
        }
    }

    /// Master transport: stop both; a recording looper finishes its layer
    /// first, then stops (stop() flags needs_note_offs; actual note-offs
    /// drain on the next looper tick, как и в ручном Stop).
    pub fn master_stop(&mut self) {
        self.drum_engine.set_playing(false);
        self.drum_engine.sequencer.reset();
        match self.looper.state {
            crate::synth::looper::LooperState::Recording
            | crate::synth::looper::LooperState::Overdubbing => {
                self.looper.toggle_record();
                self.looper.toggle_play();
            }
            crate::synth::looper::LooperState::Playing => self.looper.toggle_play(),
            _ => {}
        }
    }

    /// Master transport: toggle play — if anything plays, stop everything;
    /// otherwise start everything.
    pub fn master_toggle_play(&mut self) {
        let any = self.drum_engine.sequencer.playing
            || matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Playing
                    | crate::synth::looper::LooperState::Overdubbing
                    | crate::synth::looper::LooperState::Recording
            );
        if any {
            self.master_stop();
        } else {
            self.master_start();
        }
    }

    /// Master transport: toggle record arm on both (drums auto-play, looper
    /// Idle→Recording / Playing→Overdubbing).
    pub fn master_toggle_record(&mut self) {
        let rec = self.drum_engine.sequencer.recording
            || matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing
            );
        if rec {
            self.drum_engine.set_recording(false);
            if matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing
            ) {
                self.looper.toggle_record();
            }
        } else {
            self.drum_engine.set_recording(true);
            if !self.drum_engine.sequencer.playing {
                self.drum_engine.set_playing(true);
            }
            if matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Idle | crate::synth::looper::LooperState::Playing
            ) {
                self.looper.toggle_record();
            }
        }
    }
```

- [x] **Step 3: handle_control arms** (рядом с лупер-ветками ~2373-2396; return None — heap-полей нет):

```rust
            ControlEvent::MasterPlay => {
                self.master_toggle_play();
                None
            }
            ControlEvent::MasterStop => {
                self.master_stop();
                None
            }
            ControlEvent::MasterRecord => {
                self.master_toggle_record();
                None
            }
```

- [x] **Step 4: Роутинг пэдов/клавиш (target 2).** В `handle_event`:
  - Блок 123/82 (~1788-1801): заменить `if target == 0 { ... } else { ... }` на:

```rust
                    if target == 0 {
                        // Drums mode (set_playing keeps play_atom coherent
                        // for the GUI between audio ticks)
                        let playing = !self.drum_engine.sequencer.playing;
                        self.drum_engine.set_playing(playing);
                        if !playing { self.drum_engine.sequencer.reset(); }
                    } else if target == 1 {
                        // Looper mode
                        self.looper.toggle_play();
                    } else {
                        // Master mode (#32): both together
                        self.master_toggle_play();
                    }
```

  - Блок 124/83 (~1802-1816): та же схема — `else if target == 1 { self.looper.toggle_record(); } else { self.master_toggle_record(); }`.
  - Блок 81 (~1819-1829): заменить `if target == 0 {...} else if matches!(...) {...}` на:

```rust
                    let target = self.seq_target.load(std::sync::atomic::Ordering::Relaxed);
                    if target == 0 {
                        self.drum_engine.set_playing(false);
                        self.drum_engine.sequencer.reset();
                    } else if target == 1 {
                        if matches!(self.looper.state,
                                    crate::synth::looper::LooperState::Playing
                                    | crate::synth::looper::LooperState::Overdubbing) {
                            self.looper.toggle_play();
                        }
                    } else {
                        self.master_stop();
                    }
```

- [x] **Step 5: Дефолт target = 2.** `seq_target` init — объявление поля ~1204, конструктор ~1470: `Arc::new(AtomicU8::new(1))` → `Arc::new(AtomicU8::new(2))`.

- [x] **Step 6: Тесты (TDD — писать ДО Step 1-5, прогнать красными, потом зелёными).** Создать `tests/master_transport.rs` (харнесс — копия setup из tests/console_transport.rs):

```rust
//! Master transport (#32): Play/Rec/Stop drive drums + looper together.

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

/// Record a minimal loop so the looper can play (Idle + empty loop won't start).
fn record_loop(e: &mut SynthEngine) {
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn tc8_default_target_is_master() {
    let e = engine();
    assert_eq!(e.seq_target_atom().load(Ordering::Relaxed), 2);
}

#[test]
fn tc1_master_record_arms_both() {
    let mut e = engine();
    assert!(!e.drum_engine.sequencer.playing);
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "drums auto-play on master rec");
    assert!(matches!(e.looper.state, LooperState::Recording));
}

#[test]
fn tc2_master_record_writes_both() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    // drum hit (ch9, note 40 -> slot 4): пишется в steps[SLOT][STEP] —
    // steps[slot] — линия инструмента, второй индекс — шаг. Без tick_block
    // phase_acc = 0, квант даёт шаг (0+16-1)%16 = 15 (just-passed).
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 40 });
    let hit = e.drum_engine.sequencer.patterns
        [e.drum_engine.sequencer.current_pattern as usize]
        .steps[4]
        .iter()
        .any(|s| s.velocity > 0);
    assert!(hit, "pad note 40 must land in drum pattern slot 4 (any step)");
    // pitched note (ch0) lands in the looper. Атомы публикуются только в
    // update_atoms (per-sample из tick) — без tick_block они нулевые,
    // поэтому проверяем len() (увеличивается синхронно в record_event).
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(e.looper.len() > 0, "note must be in the looper");
}

#[test]
fn tc3_master_record_off_finishes_both() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(!e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "drums keep playing after disarm");
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn tc4_master_play_toggle() {
    let mut e = engine();
    record_loop(&mut e);
    let _ = e.handle_control(ControlEvent::LooperTogglePlay); // -> Idle
    assert!(!e.drum_engine.sequencer.playing);

    let _ = e.handle_control(ControlEvent::MasterPlay);
    assert!(e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Playing));

    let _ = e.handle_control(ControlEvent::MasterPlay);
    assert!(!e.drum_engine.sequencer.playing);
    assert_eq!(e.drum_engine.sequencer.current_step, 0, "stop resets position");
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn tc5_master_stop_from_recording() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord); // both recording
    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(!e.drum_engine.sequencer.playing);
    assert_eq!(e.drum_engine.sequencer.current_step, 0);
    assert!(matches!(e.looper.state, LooperState::Idle), "finish layer then stop");
}

#[test]
fn tc6_pads_route_to_master_at_target_2() {
    let mut e = engine();
    e.seq_target_atom().store(2, Ordering::Relaxed);
    // 83 = master rec arm
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(e.drum_engine.sequencer.recording);
    assert!(matches!(e.looper.state, LooperState::Recording));
    // 81 = master stop
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Idle));
    // 82 = master play toggle (record_loop first so looper can play)
    record_loop(&mut e);
    let _ = e.handle_control(ControlEvent::LooperTogglePlay); // -> Idle
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn tc6b_key_123_routes_to_master() {
    let mut e = engine();
    e.seq_target_atom().store(2, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
}

#[test]
fn tc9_master_stop_idle_looper_stays_idle() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(matches!(e.looper.state, LooperState::Idle), "stop must not start an idle looper");
    assert!(!e.drum_engine.sequencer.playing);
}

#[test]
fn tc8b_ch9_pads_still_play_drums_at_master_target() {
    let mut e = engine();
    e.seq_target_atom().store(2, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 40 });
    // no panic, no transport change
    assert!(!e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Idle));
}
```

- [x] **Step 7: Прогнать:** `cargo test --test master_transport` — сначала RED (нет вариантов/методов — не компилируется), после Step 1-5 — GREEN. Затем полный гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety` (console_transport.rs обязан остаться зелёным — TC7).

- [x] **Step 8: Commit**

```bash
git add src/synth/mod.rs tests/master_transport.rs
git commit -m "feat: master transport — Play/Rec/Stop drive drums + looper together (#32)"
```

### Task 2: GUI — Master-строка в docked-панели

**Files:**
- Modify: `src/gui/drums.rs` (draw_drum_seq_docked, ~18-66 — перед существующей транспортной строкой)

- [x] **Step 1: Master-строка.** Вставить ПЕРЕД существующим `ui.horizontal` с Play/Rec драм-секвенсора (~строка 25):

```rust
        // ── Master transport (#32): drums + looper together ──────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Master").strong().small());
            let lstate = crate::synth::looper::LooperState::from_u8(
                self.looper_atoms.state.load(std::sync::atomic::Ordering::Relaxed));
            let looper_active = matches!(lstate,
                crate::synth::looper::LooperState::Playing
                | crate::synth::looper::LooperState::Overdubbing
                | crate::synth::looper::LooperState::Recording);
            let any_playing = self.drum_playing || looper_active;
            if ui.selectable_label(any_playing, "\u{25B6} Play")
                .on_hover_text("Master play: drums + looper together").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::MasterPlay);
            }
            let rec = self.drum_recording
                || matches!(lstate,
                    crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing);
            if ui.selectable_label(rec, "\u{25CF} Rec")
                .on_hover_text("Master record: arm drums pattern + looper (all parts)").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::MasterRecord);
            }
            if ui.button("\u{25A0} Stop").on_hover_text("Master stop: stop both, drain notes").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::MasterStop);
            }
            ui.separator();
            // engine status dots
            ui.colored_label(
                if self.drum_playing { egui::Color32::from_rgb(40, 200, 80) } else { egui::Color32::GRAY },
                "\u{25CF} drums",
            );
            ui.colored_label(
                if looper_active { egui::Color32::from_rgb(40, 200, 80) } else { egui::Color32::GRAY },
                "\u{25CF} looper",
            );
        });
```

  Проверить импорты drums.rs: `ControlEvent` уже в scope (там пушатся DrumSeq*), `self.looper_atoms`, `self.drum_playing`, `self.drum_recording`, `self.ctrl_tx` — существующие поля App.

- [x] **Step 2: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [x] **Step 3: Commit**

```bash
git add src/gui/drums.rs
git commit -m "feat: master transport row in docked sequencer panel"
```

### Task 3: seq_target в GUI — master как основная цель + LED-консоль

**Files:**
- Modify: `src/gui/mod.rs` (part-switch writes, execute_console_action ToggleTransportTarget ~2002-2008, LED snapshot ~779-794)
- Modify: `src/gui/layers.rs` (part-row click `seq_target_atom.store(1, …)` ~318)
- Modify: `src/gui/settings.rs` (NAV feedback ~420-422)
- Modify: `src/gui/led_console.rs` (menu slot 3 цвет ~226)
- Modify: `src/lib.rs` (headless evdev: SwitchPart ~552, ToggleDrumsSynth ~573-575)

- [x] **Step 1: Инвентаризация записей.** Grep `seq_target_atom.store` и `seq_target_local.store` по src/gui и src/lib.rs. Правила: `store(0, …)` (явный выбор drums-транспорта: Drums-вкладка и headless) ОСТАВИТЬ; все остальные `store(1, …)` заменить на `store(2, …)` — включая src/lib.rs:552 (headless SwitchPart). Каждое место перечислить в отчёте. Заодно обновить устаревшие doc-комментарии: mod.rs:1203 («0 = drums, 1 = looper» → добавить «2 = master, default»), mod.rs:1470 («default: looper…» → «default: master»), led_console.rs:49 («drums <-> looper» → «drums -> looper -> master cycle»).

- [x] **Step 2: ToggleTransportTarget — цикл трёх целей.** В `execute_console_action` (~2002-2008) заменить flip 0↔1 на цикл (лог сохранить):

```rust
            led_console::Action::ToggleTransportTarget => {
                let t = self.seq_target_atom.load(Ordering::Relaxed);
                let next = (t + 1) % 3;
                self.seq_target_atom.store(next, Ordering::Relaxed);
                log::info!("[console] seq_target -> {next} ({})", match next {
                    0 => "drums", 1 => "looper", _ => "master" });
            }
```

- [x] **Step 3: LED — цвет цели и статус низа.**
  - led_console.rs ~226 (Menu slot 3 — показывает текущую цель):

```rust
                        3 => if seq_target == 0 { AVAIL }
                             else if seq_target == 1 { [0x00, 0xFF, 0x00] }
                             else { [0xFF, 0xFF, 0xFF] }, // MASTER: white
```

  (белый — из валидированной палитры; комментарий «blue = drums, green = looper» над блоком дополнить «white = master».)
  - gui/mod.rs LED snapshot (~785): ветку `if target == 0 { drums } else { looper }` заменить на:

```rust
            let (playing, recording) = if target == 0 {
                (self.drum_playing, self.drum_recording)
            } else if target == 1 {
                (
                    matches!(lstate, crate::synth::looper::LooperState::Playing
                            | crate::synth::looper::LooperState::Overdubbing),
                    matches!(lstate, crate::synth::looper::LooperState::Recording
                            | crate::synth::looper::LooperState::Overdubbing),
                )
            } else {
                // MASTER: show the union — LED reflects "something runs"
                (
                    self.drum_playing
                        || matches!(lstate, crate::synth::looper::LooperState::Playing
                                | crate::synth::looper::LooperState::Overdubbing
                                | crate::synth::looper::LooperState::Recording),
                    self.drum_recording
                        || matches!(lstate, crate::synth::looper::LooperState::Recording
                                | crate::synth::looper::LooperState::Overdubbing),
                )
            };
```

- [x] **Step 3b: Headless ToggleDrumsSynth (src/lib.rs ~572-577).** Точный код сейчас:

```rust
                    KeyAction::ToggleDrumsSynth => {
                        let current = seq_target_local.load(Ordering::Relaxed);
                        let new = if current == 0 { 1 } else { 0 };
                        seq_target_local.store(new, Ordering::Relaxed);
                        eprintln!("[input] Mode: {}", if new == 0 { "Drums" } else { "Synth" });
                    }
```

Заменить на цикл %3:

```rust
                    KeyAction::ToggleDrumsSynth => {
                        let next = (seq_target_local.load(Ordering::Relaxed) + 1) % 3;
                        seq_target_local.store(next, Ordering::Relaxed);
                        eprintln!("[input] Mode: {}", match next {
                            0 => "Drums", 1 => "Looper", _ => "Master" });
                    }
```

(И в Step 1: `seq_target_local.store(1, …)` в SwitchPart (lib.rs:552) → `store(2, …)` — правило из Step 1 распространяется на `seq_target_local` тоже.)

- [x] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

(led_console selftest — `cargo test led_console` входит в cargo test.)

- [x] **Step 5: Commit**

```bash
git add src/gui/mod.rs src/gui/layers.rs src/gui/settings.rs src/gui/led_console.rs src/lib.rs
git commit -m "feat: seq_target gains MASTER (2) as default; LED console 3-target cycle"
```

### Task 4: TECHDEBT + финальная верификация

**Files:**
- Modify: `docs/TECHDEBT.md` (#32)

- [x] **Step 1: Полные гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety && cargo build --release
```

- [x] **Step 2: Дописать в #32 (после «Phase 1 (clock): ВЫПОЛНЕНО…»):**

```markdown
Phase 2 (master transport, вариант А): ВЫПОЛНЕНО (2026-10-30) — MasterPlay/
MasterStop/MasterRecord; seq_target=2 MASTER (новый дефолт); пэды 82/83/81 и
клавиши 123/124 при target=2 управляют драмами+лупером вместе. Запись
перформанса = лупер (Mix — все части, квант 16-е, реплей текущим пресетом,
парт-мьюты) — юзер выбрал вариант А 2026-10-30: питч-секв остаётся живым
модулятором.

ОТЛОЖЕНО (не терять):
- Вариант Б (#32 фаза 2 буквально): питч-секв как standalone-плеер нот
  (anchor-нота, триггер голосов из tick_block, синхронизация позиции с
  мастер-транспортом). Большая движковая работа, риск для живого мод-режима.
- Фаза 4: упрощение pad-меню под мастер-транспорт.
- Фаза 5: переходы паттернов на границе такта + чейнинг.
- #30: docked-панель лупер+секвенсор с тогглом docked/in-tab и полным юнитом
  (BPM/Swing/Vol/Metro/Kit I/O из вкладки Drums) — отдельный план; Master-строку
  из Task 2 туда перенести.
```

- [x] **Step 3: Commit**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: #32 phase 2 master transport realized (option A); deferred items recorded"
```

- [ ] **Step 4: Ручной чеклист (юзер):**
1. Master Rec одним нажатием: драмы пишут шаги, лупер пишет ноты всех частей; повторное — завершает оба слоя
2. Master Play/Stop гоняют оба транспорта; Stop сбрасывает драм-позицию и глушит лупер-ноты
3. Пэд 27 (Rec) / 25 (Play) / 26 (Stop) и клавиши 123/124 работают как мастер после старта приложения (дефолт MASTER)
4. Menu slot 3 на LED-консоли циклит Drums → Looper → MASTER (белый)
5. Вкладка Drums по-прежнему переключает транспорт на драмы (индивидуальный режим)
