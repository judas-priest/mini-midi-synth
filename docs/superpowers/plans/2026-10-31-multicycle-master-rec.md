# Multi-Cycle Quantized Capture: «сколько сыграл» — закрытие на границе, петля = N целых циклов

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Master Rec: старт на границе такта → играешь сколько угодно циклов → второй Rec закрывает на СЛЕДУЮЩЕЙ границе → петля = РОВНО N целых циклов паттерна. Реплей нот мультитрековый (ноты уже тегированы партами — есть). Ноты во время ARMED не пишутся (ожидаемо), во время захвата — пишутся все части (Mix).

**Architecture:** Возврат `finish_pending` + счётчик циклов вместо «авто-закрытия после 1 цикла»: на каждой границе во время квантованного захвата `cycles += 1`; если `finish_pending` — закрыть и `set_loop_len_ticks(cycles * cycle_ticks)`. Без второго нажатия захват продолжается бесконечно (юзер сам решает длину). Диагноз (тестом, 2026-10-31): авто-закрытие после 1 цикла + игнор второго нажатия = «не пишет ноты/перф» — играл после первого цикла, запись уже не шла.

**МУЛЬТИТРЕК (репорт юзера 2026-10-31, перф Halloween):** запись тегировала ноту для ВСЕХ включённых частей (record_for_looper внутри цикла диспетчера, без гейта диапазона), а route_note_on звучит только в частях, чей диапазон покрывает ноту. Реплей гонял ноту во все тегированные части с их транспонированием → «ниже чем нужно» и каша. Фикс: `route_note_on -> bool` (реально прозвучала), запись только для прозвучавших; offs фильтруются в `looper.record_event` по `active_notes` (off принадлежит части, держащей ноту — убирает 15 мусорных off-событий на ноту).

**Tech Stack:** Rust; тесты tests/master_rec_sync.rs (уже с tick_block-харнессом).

---

## Test Cases (from the requirement)

Requirement (юзер, 2026-10-31): «не пишет ноты, не пишет перф по мультитрекам» — захват должен длиться до второго Rec и собрать всё сыгранное.

1. **TC-A Мультицикл:** arm → старт на границе → цикл 1: нота 60 (part 0) → Rec (finish_pending) → цикл 2: нота 67 → на границе закрылось → `Playing`, `loop_len_ticks == 2 * cycle_ticks` (96000*2 при 4/4 120BPM), обе ноты в лупере (`len >= 4` с учётом off-событий).
2. **TC-B Позиции внутри петли:** ноты цикла 2 сидят в ПОЛОВИНЕ петли (pos fraction 0.5..1.0) — мультицикл реально удлинил петлю, а не перезаписал первый цикл.
3. **TC-C Без второго Rec — не закрывается:** arm → старт → 3 цикла игры → всё ещё `Recording/Overdubbing`, len растёт с каждым циклом (события пишутся).
4. **TC-D Rec после закрытия = overdub** (остаётся): Rec на Playing → Overdubbing; закрытие на границе с `finish_pending` → петля прежней длины.
5. **TC-E MasterStop во время мультицикл-захвата:** всё останавливает, pending снят.
6. **TC-F ARMED-ноты не пишутся** (документируем): нота между нажатием и границей — не попадает в лупер.
7. **TC-G Free-режим не изменился** (tests/master_transport.rs зелёные без правок).
8. **TC-H Мультитрек по сплит-перфу:** два парта с диапазонами 0-59 / 60-127 (enabled, разные transpose), capture идёт; нота 40 → пишется ТОЛЬКО part с диапазоном 0-59 (others: `looper.len()` не растёт от части вне диапазона); в display событиях у on-события part = звучащая часть. Ноты вне диапазона частей (нет покрывающего парта) — не пишутся вовсе.
9. **TC-I Off-гигиена:** после ноты on+off в capture — ровно 2 события (не 17): off записывается только для части, держащей ноту.

## Negative Matrix

- **finish_pending стоит, drums умерли** → fallback `!playing && capturing` закрывает немедленно, длина = фактические тики (set_loop_len_ticks не зовём — нечётный кусок; редкий аварийный путь). Покрыт существующим tc9-подобным поведением, не тестируем отдельно.
- **cycles == 0 при закрытии** (второй Rec в том же шаге, что и старт — границ не было) → невозможно: закрытие происходит ТОЛЬКО на границе, и первый пересчёт границы увеличивает cycles до 1 до проверки. N/A.
- **BPM изменился посреди захвата** — sync_clock/regrid лупера рескейлит length; cycle_ticks устаревает → закрытие поставит cycles*старый_cycle. Мид-BPM-смена в живой записи — редкий сценарий; принять (задокументировано), не чинить.
- **Overdub после закрытия** — следующий MasterRecord идёт через ARM-ветку, can_quant=false (лупер не Idle) → free-путь: немедленный Overdubbing, закрытие следующим Rec немедленное (не на границе). Документировано (TC-D уточнён 2026-10-31 ревью).
- **RT-safety** — только bool/u32 поля. Гейт rt_safety.
- **event_count/слои** — offs-мусор (16 на ноту) устраняется фильтром active_notes (Task 1b Step 3, ревью Minor #6: слот чистится на записанном off — wrap-Overdubbing не выпустит forced-off по несущейся ноте).

---

### Task 1: Движок — finish_pending + счётчик циклов

**Files:**
- Modify: `src/synth/mod.rs` (поля ~1213; master_toggle_record диспетчер ~1766; master_stop ~1722; tick_block квантованный блок ~2850)

- [ ] **Step 1: Поля.** Добавить (рядом с `looper_capture_quantized`):

```rust
    /// Досрочное закрытие мультицикл-захвата (закрытие на следующей границе).
    looper_finish_pending: bool,
    /// Целых циклов паттерна, записанных квантованным захватом.
    looper_rec_cycles: u32,
```

Конструктор: `false`/`0`. В `master_stop`: `self.looper_finish_pending = false; self.looper_rec_cycles = 0;`.

- [ ] **Step 2: Диспетчер.** Заменить «игнорируем» ветку:

```rust
        // Мультицикл-захват: второе нажатие = закрыть на следующей границе
        if looper_rec && self.looper_capture_quantized {
            self.looper_finish_pending = true;
            return;
        }
```

- [ ] **Step 3: tick_block — счёт циклов и закрытие.** Текущий квантованный блок (arm branch / `boundary && capturing` branch / fallback) заменить на:

```rust
            if boundary && self.looper_arm_pending {
                self.looper_arm_pending = false;
                self.master_rec_pending.store(false, std::sync::atomic::Ordering::Relaxed);
                self.looper.toggle_record(); // Idle -> Recording (length = bars * bar)
                // Перекроить петлю: cycle = длина паттерна (базовая единица)
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                let pattern_len = self.drum_engine.sequencer.patterns[pat].length.max(1);
                let bpm = self.drum_engine.sequencer.bpm.max(1.0);
                let samples_per_step = (self.sample_rate as f64 * 60.0) / (bpm as f64 * 4.0);
                self.looper_cycle_ticks = (pattern_len as f64 * samples_per_step) as u32;
                self.looper.set_loop_len_ticks(self.looper_cycle_ticks);
                self.looper_rec_cycles = 1;
            } else if boundary && capturing {
                if self.looper_finish_pending {
                    // Закрыть: петля = сыгранные целые циклы. cycles = полных
                    // циклов ДО этой границы; закрывающая граница завершает
                    // ещё один → total = cycles + 1 (иначе петля короче игры).
                    let total = self.looper_rec_cycles.saturating_add(1);
                    self.looper.set_loop_len_ticks(
                        total.saturating_mul(self.looper_cycle_ticks));
                    self.looper_finish_pending = false;
                    self.looper_rec_cycles = 0;
                    self.drum_engine.set_recording(false); // сетка тоже разармирована
                    self.looper.toggle_record(); // Recording/Overdubbing -> Playing
                } else {
                    self.looper_rec_cycles += 1;
                }
            } else if !playing && capturing {
                // fallback: часы умерли посреди записи — закрыть сейчас
                self.looper_finish_pending = false;
                self.looper_rec_cycles = 0;
                self.drum_engine.set_recording(false);
                self.looper.toggle_record();
            }
```

  Гейт блока сверху расширить на finish_pending: `if self.looper_arm_pending || self.looper_finish_pending || quantized_looping { ... }` (quantized_looping — как сейчас, с Playing для drift correction; drift-correction ветка остаётся, но ДОБАВИТЬ guard длины — см. ниже). Порядок веток: arm → capturing-close → drift(seek) → fallback.

  **Drift-seek только для одноцикловых петель (ревью Critical #3):** `capture_quantized` не сбрасывается на закрытии → гейт пускает Playing всегда; seek_start() на каждой границе убивал бы петли длиннее цикла (вторая половина не играла бы никогда). Guard:

```rust
            } else if boundary
                && lstate == crate::synth::looper::LooperState::Playing
                && self.looper.loop_len_ticks() == self.looper_cycle_ticks
            {
                // Drift correction (Mobius-style): только для петли = 1 цикл
                self.looper.seek_start();
            }
```

  (`looper_cycle_ticks` устанавливается при старте захвата; для N>1 петель drift не корректируется — длины целотактные, дрейф минимален; отдельный пункт при необходимости.)

- [ ] **Step 4: Гейты (старые tc2/tc3 master_rec_sync упадут — Task 2 переписывает).**

- **Arp-мультитрек (ревью Important #4)** — Task 1b покрывает: arp-путь (mod.rs ~3175-3180) гейтится так же `if route_note_on(..) { record_for_looper(..) }`.

### Task 1b: Мультитрек — запись только звучащих частей

**Files:**
- Modify: `src/synth/mod.rs` (route_note_on ~1609; диспетчер note-on ~2027-2043)
- Modify: `src/synth/looper.rs` (record_event ~541)

- [ ] **Step 1: route_note_on -> bool.** Сигнатура `fn route_note_on(&mut self, i: usize, raw: u8, velocity: u8) -> bool`; ранние выходы `return false` (диапазон/velocity/root-mute), после фактического триггера (обе ветки sf2/DSP) `true`. Прочие вызывающие (arp, looper replay root_only) — игнорируют возвращаемое значение (`let _ =` не нужен, bool можно игнорить без warning только в let — вернуть и не использовать = ок, Rust не ругается на неиспользуемый результат обычной функции).

- [ ] **Step 2: Диспетчер note-on.** В цикле `for i in 0..self.parts.len()` заменить:

```rust
                    for i in 0..self.parts.len() {
                        if !self.parts[i].enabled || self.parts[i].mute { continue; }
                        let sounded = self.route_note_on(i, note, velocity);
                        if sounded {
                            // Записывается ТОЛЬКО реально звучащая часть:
                            // реплей honoured range/transpose каждого парта
                            self.record_for_looper(note, velocity, i as u8);
                        }
                    }
```

  (старый комментарий про accepted effects 1/2 — заменить: off-мусор исчезает благодаря фильтру Step 3; split overlap активных нот двух частей — looper.active_notes один слот на ноту, оверрайд остаётся accepted.)

- [ ] **Step 2b: Arp-путь — тот же гейт.** В `tick_block` arp-генерация (~3175-3180): `self.route_note_on(i, note, vel); self.record_for_looper(note, vel, i as u8);` → заменить на гейт `if self.route_note_on(i, note, vel) { self.record_for_looper(note, vel, i as u8); }` (и для off-ветки arp: off-запись остаётся — looper-фильтр сам отсечёт чужих).

- [ ] **Step 3: looper.record_event — фильтр offs.** После гейтов state/len/note добавить:

```rust
        // Multitrack hygiene: off принадлежит части, ДЕРЖАЩЕЙ ноту (engine
        // broadcasts offs всем частям — иначе 15 мусорных off-событий на ноту).
        // On трекает держателя; на записанном off слот чистится (иначе wrap
        // Overdubbing выпустит forced-off по несуществующей ноте).
        if velocity == 0 {
            if self.active_notes[note as usize] != part { return; }
            self.active_notes[note as usize] = 255;
        } else {
            self.active_notes[note as usize] = part;
        }
```

  (перед формированием event; на off НЕ чистим слот здесь — чистит replay/wrap путь как раньше; двойной off на ту же часть — второй запишется как off без держателя... для чистоты: после записи off-события 清ить нельзя (replay зависит) — ОСТАВИТЬ как в сниппете: фильтр только на входе, слот перезаписывается следующим on. Второй одинаковый off = безвредный no-op при реплее.)

- [ ] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test 2>&1 | tail -3
```

### Task 2: Тесты — мультицикл + мультитрек

**Files:**
- Modify: `tests/master_rec_sync.rs`

- [ ] **Step 1:** tc1/tc8 ОСТАВИТЬ (валидны); удалить `tc3_loop_is_one_pattern_cycle` и старый tc2; добавить tc2/tc3/tc9:

```rust
#[test]
fn tc2_multi_cycle_capture() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // до границы: старт (играется цикл 1)
    assert!(matches!(e.looper.state, LooperState::Recording));
    // цикл 1: нота 60
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = e.handle_control(ControlEvent::MasterRecord); // попросили закрыть
    // цикл 2 (закрывающий): нота 67 в ПОСЛЕДНЕЙ трети петли
    run(&mut e, steps(10));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 67 });
    run(&mut e, steps(6)); // граница: закрылось
    assert!(matches!(e.looper.state, LooperState::Playing), "closed at the boundary");
    // Петля = ровно 2 цикла
    assert!(
        (e.looper.loop_len_ticks() as i64 - 2 * 96000).abs() <= 64,
        "loop = 2 cycles: got {}", e.looper.loop_len_ticks()
    );
    assert!(e.looper.len() >= 4, "notes from both cycles captured");
    // TC-B: нота цикла 2 живёт во второй половине петли
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let second_half = disp.events.iter()
            .filter(|ev| ev.velocity > 0 && ev.position > 0.4)
            .count();
        assert!(second_half >= 2, "cycle-2 notes live in the second half");
        let _ = disp; // держим guard до конца блока
    }
}

#[test]
fn tc3_no_autoclose_without_second_press() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(13)); // старт
    run(&mut e, steps(16) * 3); // 3 цикла без нажатий
    assert!(
        matches!(e.looper.state, LooperState::Recording | LooperState::Overdubbing),
        "no second press = still capturing"
    );
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16)); // закрывающая граница
    assert!(matches!(e.looper.state, LooperState::Playing));
    // 4 цикла сыграно (стартовый + 3): total = cycles+1
    assert!(
        (e.looper.loop_len_ticks() as i64 - 4 * 96000).abs() <= 64,
        "4 cycles captured: got {}", e.looper.loop_len_ticks()
    );
}

#[test]
fn tc9_multitrack_records_only_sounding_parts() {
    let mut e = engine();
    // Сплит: part 0 = 0..59 (transpose +12), part 1 = 60..127
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват идёт
    // Ноты с обеих рук
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 }); // part 0
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 }); // part 1
    run(&mut e, 94);
    for n in [40u8, 72u8] {
        e.handle_event(MidiEvent::NoteOff { channel: 0, note: n });
    }
    run(&mut e, 94);
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 40 && ev.part_id == 0), "40 -> part 0");
        assert!(ons.iter().any(|ev| ev.note == 72 && ev.part_id == 1), "72 -> part 1");
        assert!(!ons.iter().any(|ev| ev.note == 40 && ev.part_id == 1), "40 must NOT tag part 1");
        let offs = disp.events.iter().filter(|ev| ev.velocity == 0).count();
        assert!(offs <= 4, "no 15-per-note garbage offs: got {offs}");
    }
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Playing));
}
```

  (MidiEvent в use провернуть; tc5/tc4 не трогать; tc2 Multi-cycle note 67 играет В закрывающем цикле — position > 0.4 валиден для 2-цикловой петли.)

- [ ] **Step 3: Гейты + Commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety && cargo build --release
git add src/synth/mod.rs tests/master_rec_sync.rs
git commit -m "feat: multi-cycle quantized capture — close at boundary, loop = N whole cycles"
```

### Task 3: TECHDEBT + финал

- [ ] **Step 1:** В #32 после Phase sync-заметки: «Multi-cycle (2026-10-31): захват длится до второго Rec (диагноз: авто-закрытие после 1 цикла + игнор нажатия = «не пишет»); закрытие на границе, петля = N целых циклов; Rec после закрытия = overdub».

- [ ] **Step 2:** Полные гейты, `cargo build --release` (рестарт — главная сессия), commit docs.

- [ ] **Step 3: Ручной чеклист (юзер):** Play → Rec → играй сколько надо (ноты+ударка в кубики) → Rec ещё раз → после границы петля = сколько сыграл; Rec после закрытия = overdub.
