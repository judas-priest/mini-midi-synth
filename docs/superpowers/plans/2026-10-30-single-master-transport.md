# Single Master Transport: kill per-engine transports + target cycling (#32 фаза 4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Master Play/Rec/Stop — ЕДИНСТВЕННЫЙ транспорт: строки Play/Rec драмов и лупера удаляются из панели, пэды 25/26/27 и клавиши 123/124 ВСЕГДА управляют мастером, переключатель цели (LED-консоль slot 3, цикл 0→1→2) удаляется; `seq_target` остаётся только как режим живых драм-пэдов (0 = drum-note mode).

**Architecture:** В движке — упрощение `handle_event`: transport-ноты (123/82, 124/83, 81) идут в master_* без ветвления по target. `seq_target` сужается до «drum-note mode» (чтение `target == 0` в `drum_mode` остаётся, дефолт 1). В GUI — одна транспортная строка docked-юнита (Master + seq Undo/Clear + looper Undo/Clear + BPM + Metro + beat-flash), из `draw_looper` удаляется транспортная строка (остаются state-лейбл и прогресс-бар). LED-консоль: `Action::ToggleTransportTarget` удаляется, menu slot 3 гаснет.

**Tech Stack:** Rust, rtrb ControlEvent, egui; тесты SynthEngine + led_console selftest.

**СОГЛАСОВАНО С ЮЗЕРОМ (2026-10-30):** несколько Play-кнопок — ошибка реализации; спека #32 фаза 4 требовала упрощение переключателя. Изменение семантики тестов `console_transport.rs` (пэды теперь мастер при ЛЮБОМ target) — это РЕАЛИЗАЦИЯ одобренного поведения, не ослабление тестов (маркер в коммите не нужен: тесты переписываются под новую спеку, а не чтобы пройти).

---

## Test Cases (from the requirement)

Requirement (юзер, 2026-10-30): «зачем мне несколько кнопок Play?» + спека #32 фаза 4 «pad-меню упростить»; единая панель с одним транспортом.

1. **TC1 Пэды всегда мастер:** при `seq_target` = 0, 1, 2 — note 82 (ch9) тогглит МАСТЕР (драмы+лупер вместе), 83 — master record arm, 81 — master stop. Клавиша 123 — master play toggle.
2. **TC2 seq_target = drum-note mode:** при target=0 ноты 36-51 на НЕ-drum-канале триггерят ударку (`drum_mode`), при target=1 — играют парты; transport-ноты (81/82/83/123/124) НЕ зависят от target в обоих случаях.
3. **TC3 Дефолт:** `SynthEngine::new` → `seq_target == 1` (НЕ drum-note mode).
4. **TC4 LED-консоль без slot 3:** tap slot 3 в Menu возвращает `None`; `Action::ToggleTransportTarget` не существует (compile-факт); menu slot 3 — OFF цвет.
5. **TC5 GUI один транспорт:** в docked-панели нет кнопок Play/Rec драм-строки и лупер-строки (compile/визуальный факт — grep `DrumSeqPlay` в gui/ остаётся только в keybinds/headless/master-paths, а в drums.rs Play/Rec драмов не рендерятся); Metro-чекбокс в транспортной строке юнита.
6. **TC6 Питч/драмы не сломаны:** `DrumSeqPlay`/`LooperTogglePlay` события остаются валидными ControlEvent (их шлют keybinds и headless), движковые ветки работают.

## Negative Matrix

- **Master Stop на Recording-лупере** — существующая семантика `master_stop` (finish→stop) без изменений, покрыта тестами master_transport.rs. N/A.
- **Старые сохранённые конфиги** — `seq_target` не персистится (сессионная атомика); дефолт меняется свободно. N/A.
- **Keybinds/headless (DrumTogglePlay, LooperRecord и т.п.)** — остаются: это ЯВНЫЕ индивидуальные команды с клавиатуры компа, не дубли транспортной строки. Осознанно сохранены.
- **Переписанные console_transport.rs** — старые ассерты «82 тогглит лупер при target=1» больше не истинны по спеке; тесты переписаны под новое поведение (пэды = мастер всегда), охват предыдущих сценариев сохранён (drums/looper различимость проверяется через drum-note mode TC2). Изменение одобрено юзером.
- **led_console selftest** — тесты slot 3 переписываются (slot 3 = None). Охват menu-навигации сохраняется.
- **Гонки/дубли нажатий** — rtrb FIFO, как раньше. N/A.

---

### Task 1: Движок — transport-ноты всегда мастер, seq_target = drum-note mode

**Files:**
- Modify: `src/synth/mod.rs` (handle_event transport-блоки ~1874-1917; seq_target init ~1481; doc-комментарий поля ~1206)

- [ ] **Step 1: Блок 123/82** — заменить ветвление по target на безусловный мастер (заодно обновить doc-комментарий над блоком ~1865-1868 «…follow seq_target…» → «always drive the master transport»):

```rust
                if velocity > 0 && (note == 123 || (channel == 9 && note == 82)) {
                    // Master transport (#32 фаза 4): single transport, no target
                    self.master_toggle_play();
                    return;
                }
```

- [ ] **Step 2: Блок 124/83:**

```rust
                if velocity > 0 && (note == 124 || (channel == 9 && note == 83)) {
                    self.master_toggle_record();
                    return;
                }
```

- [ ] **Step 3: Блок 81:**

```rust
                if channel == 9 && velocity > 0 && note == 81 {
                    self.master_stop();
                    return;
                }
```

- [ ] **Step 4: seq_target — сузить семантику.** Doc-комментарий поля (~1206) заменить:

```rust
    /// Drum-note mode gate for live pads: 0 = pads 36-51 trigger drums on ANY
    /// channel (drum_mode), nonzero = pads play parts. NOT a transport target —
    /// transport notes (81/82/83/123/124) always drive the master transport.
```

Конструктор (~1481): `AtomicU8::new(2)` → `AtomicU8::new(1)` (комментарий «default: master…» → «default: non-drum mode»). Заодно обновить устаревший комментарий в drum-пути (~1927-1928: «83/82/81 moved above — they follow seq_target now» → «83/82/81 moved above — master transport now, no target»).

- [ ] **Step 5: Тесты — переписать `tests/master_transport.rs`.** Заменить tc6/tc6b/tc8/tc8b на:

```rust
#[test]
fn tc1_pads_are_always_master_any_target() {
    for target in [0u8, 1, 2] {
        let mut e = engine();
        e.seq_target_atom().store(target, Ordering::Relaxed);
        // 83 = master rec arm
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
        assert!(e.drum_engine.sequencer.recording, "target {target}");
        assert!(matches!(e.looper.state, LooperState::Recording), "target {target}");
        // 81 = master stop
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
        assert!(!e.drum_engine.sequencer.playing, "target {target}");
        assert!(matches!(e.looper.state, LooperState::Idle), "target {target}");
        // 82 = master play toggle
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
        assert!(e.drum_engine.sequencer.playing, "target {target}");
        assert!(matches!(e.looper.state, LooperState::Idle), "82 must not start empty looper, target {target}");
    }
}

#[test]
fn tc1b_key_123_is_always_master() {
    for target in [0u8, 1] {
        let mut e = engine();
        e.seq_target_atom().store(target, Ordering::Relaxed);
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
        assert!(e.drum_engine.sequencer.playing, "target {target}");
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
        assert!(!e.drum_engine.sequencer.playing, "target {target}");
    }
}

#[test]
fn tc3_default_target_is_non_drum_mode() {
    let e = engine();
    assert_eq!(e.seq_target_atom().load(Ordering::Relaxed), 1);
}

#[test]
fn tc2_target_still_gates_drum_note_mode() {
    // target=0: note 40 on channel 0 triggers drums (drum_mode)
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    // transport untouched by a plain pad note
    assert!(!e.drum_engine.sequencer.playing);
    // target=1: same note goes to the part synth path (no panic, no transport)
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Idle));
}
```

(Старые tc8 «default == 2», tc6, tc6b, tc8b — УДАЛИТЬ ЦЕЛИКОМ; их заменяют новые tc1_pads_are_always_master_any_target / tc1b_key_123_is_always_master / tc2_target_still_gates_drum_note_mode / tc3_default_target_is_non_drum_mode из сниппета выше — имена уникальны, конфликтов со старыми tc1_master_record_arms_both / tc2_master_record_writes_both / tc3_master_record_off_finishes_both нет: разные суффиксы. Старые tc1_master_record_arms_both, tc2_master_record_writes_both, tc3_master_record_off_finishes_both, tc4_master_play_toggle, tc5_master_stop_from_recording, tc9_master_stop_idle_looper_stays_idle — НЕ ТРОГАТЬ, helper `record_loop` остаётся (используется tc4).)

- [ ] **Step 6: Переписать `tests/console_transport.rs`** под новую семантику (заголовок: «Transport pads drive the MASTER transport regardless of seq_target; seq_target gates drum-note mode only»):

```rust
//! Transport pads (bank-2 notes 81/82/83 ch9) + keys 123/124 ALWAYS drive the
//! master transport (#32 фаза 4). seq_target no longer selects a transport —
//! it only gates drum-note mode (0 = pads trigger drums on any channel).

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

#[test]
fn pad_play_82_is_master_any_target() {
    for target in [0u8, 1, 2] {
        let mut e = engine();
        e.seq_target_atom().store(target, Ordering::Relaxed);
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
        assert!(e.drum_engine.sequencer.playing, "target {target}: 82 must start master");
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
        assert!(!e.drum_engine.sequencer.playing);
        assert_eq!(e.drum_engine.sequencer.current_step, 0, "stop resets position");
    }
}

#[test]
fn pad_rec_83_arms_master_and_snapshots() {
    let mut e = engine();
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "rec on stopped transport auto-plays");
    assert!(matches!(e.looper.state, LooperState::Recording));
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(!e.drum_engine.sequencer.recording);
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn pad_stop_81_stops_master() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
    // idle looper must NOT be started by stop
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn key_123_is_master() {
    let mut e = engine();
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
}

#[test]
fn transport_notes_on_other_channels_play_notes_not_transport() {
    let mut e = engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 82, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 1, note: 83, velocity: 100 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(!e.drum_engine.sequencer.recording);
}

#[test]
fn seq_target_zero_enables_drum_note_mode_for_plain_pads() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    // note 40 on channel 0: with drum_mode on it hits the drum engine (no panic)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Idle));
}
```

- [ ] **Step 7: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test --test master_transport --test console_transport && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 8: Commit**

```bash
git add src/synth/mod.rs tests/master_transport.rs tests/console_transport.rs
git commit -m "feat: transport pads/keys always drive master; seq_target is drum-note mode only (#32 p4)"
```

### Task 2: GUI — одна транспортная строка в docked-юните

**Files:**
- Modify: `src/gui/drums.rs` (Master row ~24-61; drum transport row ~63-106; draw_looper transport row ~825-860; draw_drum_kit_controls Metro ~281-412)
- Modify: `src/gui/mod.rs` ( ничего — панель уже колонки)

- [ ] **Step 1: Заменить Master-строку и драм-транспортную строку на ОДНУ.** В `draw_drum_seq_docked` удалить блок «Master transport» (строки ~24-61 целиком) и транспортный `ui.horizontal` (~64-106: Play/Stop/Rec/flash/Undo/Clear/BPM). На их месте — одна строка:

```rust
        // ── Single master transport (#32 фаза 4) ─────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Master").strong().small());
            let lstate = crate::synth::looper::LooperState::from_u8(
                self.looper_atoms.state.load(std::sync::atomic::Ordering::Relaxed));
            let looper_active = matches!(lstate,
                crate::synth::looper::LooperState::Playing
                | crate::synth::looper::LooperState::Overdubbing
                | crate::synth::looper::LooperState::Recording);
            let any_playing = self.drum_playing || looper_active;
            let play_label = if any_playing { "\u{23F9} Stop" } else { "\u{25B6} Play" };
            if ui.button(play_label).on_hover_text("Master: drums + looper together").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::MasterPlay);
            }
            let rec = self.drum_recording
                || matches!(lstate,
                    crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing);
            let rec_label = if rec { "\u{23FA} Rec" } else { "\u{26AB} Rec" };
            let rec_color = if rec { egui::Color32::RED } else { egui::Color32::GRAY };
            if ui.button(egui::RichText::new(rec_label).color(rec_color))
                .on_hover_text("Master record: arm drums pattern + looper (all parts)").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::MasterRecord);
            }

            // Downbeat flash indicator (lights ~150 ms after beat 1)
            let flashing = self.drum_beat_flash
                .map(|t| t.elapsed() < std::time::Duration::from_millis(150))
                .unwrap_or(false);
            let (flash_rect, _) =
                ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(
                flash_rect.center(), 5.0,
                if flashing { egui::Color32::from_rgb(255, 190, 60) }
                else { egui::Color32::from_gray(45) },
            );

            if ui.button("Undo").on_hover_text("Undo last sequencer edit").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqUndo);
            }
            if ui.button("Clear").on_hover_text("Clear all steps").clicked() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqClear);
            }
            if ui.add_enabled(looper_undo_avail, egui::Button::new("L-Undo"))
                .on_hover_text("Undo last looper layer").clicked()
            {
                let _ = self.ctrl_tx.push(ControlEvent::LooperUndo);
            }

            ui.separator();
            ui.label("BPM:");
            if ui.add(egui::DragValue::new(&mut self.drum_bpm).range(40.0..=300.0).speed(0.5)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqBpm { bpm: self.drum_bpm });
            }
            // Паттерн мутации поля — как drum_bpm выше (capture &mut self).
            // Персист обязателен: старый чекбокс звал save_global_to_config().
            if ui.checkbox(&mut self.drum_metronome, "Metro").on_hover_text("Metronome click on beats").changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSetMetronome { on: self.drum_metronome });
                self.save_global_to_config();
            }
        });
```

  `looper_undo_avail` вычислить ПЕРЕД `ui.horizontal` (атомики читаю заранее, замыкание ловит `&mut self` целиком):

```rust
        let looper_undo_avail = {
            let a = &self.looper_atoms;
            a.layer_count.load(std::sync::atomic::Ordering::Relaxed) > 1
                && a.event_count.load(std::sync::atomic::Ordering::Relaxed) > 0
        };
```

  `self.drum_metronome` — поле App (сейчас мутится чекбоксом в draw_drum_kit_controls ~337-348); мутация в замыкании — тот же паттерн, что `drum_bpm` (drums.rs:103 мутирует поле прямо в замыкании — это компилируется). Персист `save_global_to_config()` ОБЯЗАТЕЛЕН (старый чекбокс его звал; `config.ui.drum_metronome` читается при старте — gui/mod.rs:1498, ре-пуш 1419).

- [ ] **Step 2: Убрать Metro из вкладки Drums.** В `draw_drum_kit_controls` (drums.rs ~281-412) удалить чекбокс Metro (`DrumSetMetronome`) — он переехал в транспортную строку. Swing/Vol/V127/Kit I/O НЕ трогать (SETUP остаётся на вкладке, спека #30).

- [ ] **Step 3: Убрать из строки лупера только транспортные кнопки.** Транспортная строка `draw_looper` — ОДИН `ui.horizontal` на drums.rs:832-900, в котором живут: Play/Stop (`LooperTogglePlay`, ~832-840), Rec (`LooperRecord`/`LooperStopRecord`, ~840-855), Undo (`LooperUndo`, ~855-859), Clear с 2s-подтверждением (`self.looper_clear_confirm`, ~860-877: клик ставит confirm, expiry-проверка ~875-877, второй клик в окне 2с делает `LooperClear`), state-лейбл (`status_color`, ~881-893), лейбл `{event_count}ev L{layer_count}` (~894-896), ProgressBar с гейтом `if state != Idle` (~897-899, `position as f32 / 255.0`). УДАЛИТЬ только Play/Stop, Rec, Undo. Всё остальное (Clear с его логикой подтверждения, state-лейбл, ev/L-лейбл, ProgressBar с гейтом) ОСТАЁТСЯ на месте в той же строке — код не переносить, не переименовывать.

- [ ] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add src/gui/drums.rs
git commit -m "feat: single master transport row in docked unit; looper/drum transport rows removed"
```

### Task 3: LED-консоль и GUI — убрать переключатель цели

**Files:**
- Modify: `src/gui/led_console.rs` (Action enum ~45-58; tap() ~123-131; Menu slot 3 цвет ~216-230; selftest-тесты ~280-509)
- Modify: `src/gui/mod.rs` (execute_console_action arm ~2011-2018; top_colors caller ~810-820; seq_target store-сайты)
- Modify: `src/gui/layers.rs` (~318), `src/gui/settings.rs` (~420), `src/lib.rs` (~552, ~572-577)

- [ ] **Step 1: Удалить Action::ToggleTransportTarget.** В led_console.rs: убрать вариант из enum; в `tap()` slot 3 → `_ => None` (dark slot); в Menu-цветах slot 3 → `[0x00, 0x00, 0x00]` (OFF). Параметр `seq_target: u8` в `top_colors(...)` удалить (больше не читается), обновить doc-комментарий.
- [ ] **Step 2: selftest led_console.** Затронутые тесты (anchors по факту): `menu_slot3_toggles_target_and_stays_menu` (~415) и `menu_slot3_led_shows_target` (~426) — перевести: tap slot 3 в Menu → `None`, цвет slot 3 → OFF; тесты без слота 3, но передающие последний аргумент seq_target в `top_colors` — `window_offsets_map_targets_to_slots` (~391, 398) и `mute_mode_windows_over_all_parts` (~446, 451) — механически убрать лишний аргумент.
- [ ] **Step 3: execute_console_action** (gui/mod.rs, arm `ToggleTransportTarget` ~2025-2031) — удалить arm. Caller `top_colors(...)` в LED-снапшоте — убрать последний аргумент `seq_target`. ТАМ ЖЕ (gui/mod.rs:782-805): нижний ряд LED-транспорта ветвится `target == 0 → драмы / target == 1 → лупер / else → union` — после удаления cycling значения 2 не бывает, а пэды всегда мастер → ветку упростить ДО union-варианта безусловно (удалить if/else, оставить union-вычисление); удалить ставший мёртвым `let target = self.seq_target_atom.load(...)` (~782) и обновить устаревший комментарий над блоком (~779-781 «Bottom row shows the ACTIVE target's transport…»).
- [ ] **Step 4: seq_target store-сайты → 1.** Все не-драмовые записи: layers.rs:318, settings.rs:420 (`!show_drums` ветка), gui/mod.rs:1536 (SwitchPart), gui/mod.rs:1559 (ToggleDrumsSynth !show_drums ветка; соседний store(0) в 1561 оставить), lib.rs:552 — `store(2, …)` → `store(1, …)`. lib.rs ToggleDrumsSynth (~572-577) — вернуть flip 0↔1:

```rust
                    KeyAction::ToggleDrumsSynth => {
                        let next = if seq_target_local.load(Ordering::Relaxed) == 0 { 1 } else { 0 };
                        seq_target_local.store(next, Ordering::Relaxed);
                        eprintln!("[input] Mode: {}", if next == 0 { "Drums" } else { "Synth" });
                    }
```

- [ ] **Step 5: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 6: Commit**

```bash
git add src/gui/led_console.rs src/gui/mod.rs src/gui/layers.rs src/gui/settings.rs src/lib.rs
git commit -m "feat: drop transport-target cycling — LED menu slot 3 dark, seq_target is drum-note mode (#32 p4)"
```

### Task 4: TECHDEBT + финал

**Files:**
- Modify: `docs/TECHDEBT.md` (#32)

- [ ] **Step 1:** Полные гейты + `cargo build --release`.

- [ ] **Step 2: В #32 дописать:**

```markdown
Phase 4 (упрощение pad-меню): ВЫПОЛНЕНО (2026-10-30) — transport-ноты
(пэды 81/82/83 ch9, клавиши 123/124) ВСЕГДА управляют master-транспортом;
переключатель seq_target (LED menu slot 3, цикл 0→1→2) удалён. seq_target
сужен до drum-note mode (0 = пэды 36-51 триггерят ударку на любом канале),
переключение — вкладка Drums / headless ToggleDrumsSynth (flip 0↔1).
GUI: одна транспортная строка в docked-юните (Master Play/Rec + Undo/Clear
секв + L-Undo лупера + BPM + Metro + beat-flash); строки Play/Rec драмов и
лупера удалены; state/progress/Clear лупера сохранены.
```

И в списке отложенного убрать пункт «Фаза 4: упрощение pad-меню…» (выполнена).

- [ ] **Step 3: Commit**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: #32 phase 4 single transport realized; pad-menu simplification done"
```

- [ ] **Step 4: Ручной чеклист (юзер):**
1. В docked-юните ОДНА транспортная строка (Master Play/Rec, Undo/Clear, L-Undo, BPM, Metro, flash)
2. У лупера — только state + прогресс + Clear; у драмов — Pattern/сигнатура/Steps/сетка
3. Пэды 25/26/27 и клавиши 123/124 = мастер при любом режиме
4. LED menu: slot 3 погашен, тап ничего не делает
5. Вкладка Drums: Metro исчез, остальное (Swing/Vol/V127/Kit) на месте
