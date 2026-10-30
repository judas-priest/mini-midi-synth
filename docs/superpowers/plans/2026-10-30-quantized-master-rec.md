# Quantized Master Recording: looper aligns to sequencer bar boundaries (#32/#30 sync)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Master Rec стартует запись лупера на границе такта секвенсора и сама закрывает петлю после `Bars` тактов (нажал раз — получил целое число тактов, выровненное с паттерном; второй нажатий — досрочное закрытие тоже на границе такта). Длины лупера и секвенсора всегда целотактовые и фазово совпадают.

**Architecture:** Практика (SooperLooper 8ths/cycle, Boss RC drum-sync): старт/стоп петли квантуются к сетке драм-машины, длина — целые такты. Реализация: три поля на `SynthEngine` (`looper_arm_pending`, `looper_finish_pending`, `looper_rec_bars`), детект границы такта в `tick_block` (drum `current_step == 0`), финиширование существующим `looper.toggle_record()`. Free-режим (секвенсор не играет) — прежнее немедленное поведение. GUI: статус «ARMED» через новый атом `master_rec_pending_atom` (пламб через CommonInit — урок detached-atom).

**Tech Stack:** Rust, существующие LooperState/toggle_record, атомики; тесты через tick_block harness (как tests/looper_timing.rs).

**Отвергнутая альтернатива:** авто-равнение Bars = длине паттерна — Bars остаются ручными (1/2/4/8), выравнивание даёт квантование границ. Если юзер потом захочет — отдельный пункт.

---

## Test Cases (from the requirement)

Requirement (юзер, 2026-10-30): «не синхронизированы размеры НИХУЯ у лупера и секвенсора»; best practice — квантование старта/закрытия петли к границе такта.

1. **TC1 Старт на границе:** секвенсор играет (playhead на шаге 5), MasterRecord → лупер остаётся Idle (armed), при переходе playhead на шаг 0 → `Recording`. Ни одного события лупера до границы.
2. **TC2 Авто-закрытие после Bars:** Bars=1, MasterRecord на шаге 5 → на границе старт, на СЛЕДУЮЩЕЙ границе → `Playing` (петля ровно 1 такт, без второго нажатия).
3. **TC3 Досрочное закрытие квантуется:** Bars=4, MasterRecord, старт на границе, играем 1.5 такта, второй MasterRecord → запись закрывается на СЛЕДУЮЩЕЙ границе (петля = 2 целых такта), не сразу.
4. **TC4 Free-режим сохранён:** секвенсор НЕ играет, MasterRecord → запись стартует НЕМЕДЛЕННО (сырая, как было), второй MasterRecord закрывает сразу.
5. **TC5 MasterStop во время armed/recording:** снимает pending-флаги и останавливает как раньше (Recording → finish+stop, лупер Idle).
6. **TC6 GUI «ARMED»:** атом `master_rec_pending` = true между нажатием и границей (визуальный факт + атом в тесте).
7. **TC7 Событ<![CDATA[>]]>я выровнены:** нота, сыгранная на шаге k такта, реплеится лупером на той же позиции внутри такта каждый цикл (позиция в тиках делится нацело на длину такта).

## Negative Matrix

- **Секвенсор играет, но pattern length ≠ 16** (например 12 шагов 3/4): граница такта = current_step==0, цикл паттерна = length шагов — выравнивание по началу ПАТТЕРНА (такт секвенсора = его length). Bars считаются по пересечениям границы. Для 3/4 «бар» = 12 шагов — соответствует sig. Покрыто детектом current_step==0 (он наступает раз в pattern length).
- **Bars=1 и второй нажатие ДО первой границы** (старт ещё не случился): arm_pending → снять arm (отмена), лупер остаётся Idle. Тест tc8.
- **MasterStop между arm и границей** — снять arm, лупер Idle (tc5).
- **Drums остановились ВО ВРЕМЯ записи** (после старта): finish_pending повиснет (границ не будет) — fallback: если `!playing` и finish_pending → закрыть НЕМЕДЛЕННО (без квантования). Тест tc9.
- **Q Off/On** — не влияет: квантование событий (Q) и квантование границ петли — независимые вещи. N/A.
- **Overdub (лупер Playing → MasterRecord)** — прежняя семантика (сразу Overdubbing): диспетчер требует `looper.state == Idle` для can_quant — Playing уходит в free-путь. N/A.
- **Arm во время шага 0** (в т.ч. сразу после MasterPlay) — edge-detect ждёт СЛЕДУЮЩЕЙ границы (полный цикл паттерна). Принято и задокументировано.
- **RT-safety:** pending-флаги — bool-поля, детект границы — чтение existing атомики/поля; аллокаций нет. Гейт rt_safety.
- **Миди-часы/внешний clock** — вне scope (S5 уже отверг внешний clock).

---

### Task 1: Движок — pending-машина + детект границы такта

**Files:**
- Modify: `src/synth/mod.rs` (поля SynthEngine ~1206; master_toggle_record ~1737; tick_block ~2698-2760; master_stop ~1712; atom для GUI ~1486 accessor area)
- Modify: `src/synth/looper.rs` (методы truncate_loop / loop_len_ticks в impl MidiLooper)

- [ ] **Step 1: Поля + атом.** Рядом с `seq_target` добавить:

```rust
    /// Quantized master recording (#30 sync): arm/finish pending until the
    /// drum sequencer crosses its pattern boundary (current_step == 0).
    looper_arm_pending: bool,
    looper_finish_pending: bool,
    /// Как был начат ТЕКУЩИЙ захват: true = квантованный (граница такта),
    /// false = свободный (немедленный). Дискриминатор досрочного нажатия.
    looper_capture_quantized: bool,
    /// Bar boundaries crossed since looper recording started (quantized mode).
    looper_rec_bars_done: u8,
    /// Previous drum step for boundary edge detection.
    looper_prev_step: u8,
    /// GUI mirror of arm_pending (shows "ARMED" in the looper state label).
    master_rec_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,
```

Конструктор: все bool/счётчики false/0, `master_rec_pending: Arc::new(AtomicBool::new(false)),`. Accessor:

```rust
    pub fn master_rec_pending_atom(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.master_rec_pending.clone()
    }
```

В `src/synth/looper.rs` (impl MidiLooper) добавить — досрочное закрытие должно ОБРЕЗАТЬ петлю до фактически записанных тактов (start_record заранее ставит length = bars×bar, stop_record не режет):

```rust
    /// Quantized early-finish (#30 sync): truncate the loop to the whole bars
    /// actually recorded (start_record pre-gridded `bars`; early press closes
    /// sooner — trim the silence tail).
    pub fn truncate_loop(&mut self, bars: u8) {
        let bar = self.bar_length_samples();
        self.loop_length_ticks = bar.saturating_mul(bars.max(1) as u32);
        self.update_display();
        self.sync_atoms();
    }

    /// Loop length in ticks (test/GUI introspection).
    pub fn loop_len_ticks(&self) -> u32 {
        self.loop_length_ticks
    }
```

  (visibility полей/методов сверить по факту: `bar_length_samples`/`loop_length_ticks` приватные, но методы внутри impl MidiLooper — доступ есть.)

- [ ] **Step 2: master_toggle_record — диспетчер.** Текущее тело (~1737-1765) переименовать в `master_toggle_record_free_looper()` (точная копия: `set_recording(false)` + toggle_record лупера) и новый диспетчер. КЛЮЧЕВОЙ принцип ревью: free/quantized решается по тому, КАК был начат текущий захват (`looper_capture_quantized`), а не по drum_playing в момент нажатия:

```rust
    /// Master transport: toggle record arm. Quantized path: drums playing,
    /// pattern >= 2 steps, looper Idle -> capture starts at the NEXT pattern
    /// boundary, auto-closes after `bars` bars; early press closes at the
    /// next boundary. Everything else (stopped sequencer, Steps=1, overdub
    /// from Playing, disarming) — прежнее немедленное поведение.
    pub fn master_toggle_record(&mut self) {
        let looper_rec = matches!(
            self.looper.state,
            crate::synth::looper::LooperState::Recording
                | crate::synth::looper::LooperState::Overdubbing
        );
        // Досрочное закрытие — только для захвата, начатого квантованно
        if looper_rec && self.looper_capture_quantized {
            self.looper_finish_pending = true;
            return;
        }
        // Снять арм (toggle-семантика ожидания границы)
        if self.looper_arm_pending {
            self.looper_arm_pending = false;
            self.master_rec_pending.store(false, std::sync::atomic::Ordering::Relaxed);
            self.drum_engine.set_recording(false);
            return;
        }
        // Только сетка пишется (лупер Idle) — разармировать сетку, лупера
        // НЕ трогать (старый toggle в этой ситуации лупер не запускал)
        if !looper_rec && self.drum_engine.sequencer.recording {
            self.drum_engine.set_recording(false);
            return;
        }
        if !looper_rec {
            // ARM
            let pat = self.drum_engine.sequencer.current_pattern as usize;
            let pattern_len = self.drum_engine.sequencer.patterns[pat].length;
            let can_quant = self.drum_engine.sequencer.playing
                && pattern_len > 1
                && self.looper.state == crate::synth::looper::LooperState::Idle;
            self.drum_engine.set_recording(true);
            if !can_quant {
                // Free: секвенсор стоит (autoplay), Steps=1, или overdub из Playing
                if !self.drum_engine.sequencer.playing {
                    self.drum_engine.set_playing(true);
                }
                self.looper_capture_quantized = false;
                if matches!(
                    self.looper.state,
                    crate::synth::looper::LooperState::Idle
                        | crate::synth::looper::LooperState::Playing
                ) {
                    self.looper.toggle_record();
                }
                return;
            }
            // Armed: ждём границы паттерна
            self.drum_engine.set_playing(true);
            self.looper_arm_pending = true;
            self.looper_capture_quantized = true;
            self.master_rec_pending.store(true, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        // looper_rec && !quantized (free-захват или overdub) — закрыть сразу
        self.master_toggle_record_free_looper();
    }

    /// Former immediate toggle (verbatim прежнее тело): Recording/Overdubbing
    /// -> finish + set_recording(false); Idle/Playing -> record.
    fn master_toggle_record_free_looper(&mut self) {
        self.drum_engine.set_recording(false);
        if matches!(
            self.looper.state,
            crate::synth::looper::LooperState::Recording
                | crate::synth::looper::LooperState::Overdubbing
        ) {
            self.looper.toggle_record();
        } else if matches!(
            self.looper.state,
            crate::synth::looper::LooperState::Idle | crate::synth::looper::LooperState::Playing
        ) {
            self.looper.toggle_record();
        }
    }
```

  Следствие для тестов (ревью подтвердило): с дискриминатором `looper_capture_quantized` все существующие тесты master_transport.rs (tc1/tc2/tc3/tc5 — free-захваты, свежий движок) остаются зелёными БЕЗ правок; tc3 (второй MasterRecord) идёт по free-ветке и закрывается немедленно как раньше.

- [ ] **Step 3: master_stop — снять pending.** В `master_stop()` в начало:

```rust
        self.looper_arm_pending = false;
        self.looper_finish_pending = false;
        self.looper_rec_bars_done = 0;
        self.looper_capture_quantized = false;
        self.master_rec_pending.store(false, std::sync::atomic::Ordering::Relaxed);
```

  (остальное тело без изменений: Recording → toggle_record+toggle_play и т.д.)

- [ ] **Step 4: Детект границы в tick_block.** В `tick_block` ПОСЛЕ `looper.sync_clock(...)` (~2772) и до пер-сэмплового цикла. Анти-повтор (step==0 держится ~93 блока): prev_step читается и обновляется БЕЗУСЛОВНО сразу. Сигнатура tick_block: `tick_block(&mut self, buf_l: &mut [f32], buf_r: &mut [f32])`:

```rust
        // Quantized master recording: pattern boundary = sequencer wrapped to
        // step 0 (edge-detected). Arm -> start capture; finish_pending or
        // bars reached -> close (и снять запись сетки).
        let step = self.drum_engine.sequencer.current_step;
        let prev = self.looper_prev_step;
        self.looper_prev_step = step;
        if self.looper_arm_pending
            || self.looper_finish_pending
            || self.looper_rec_bars_done > 0
            || (self.looper_capture_quantized
                && self.looper.state == crate::synth::looper::LooperState::Recording)
        {
            let playing = self.drum_engine.sequencer.playing;
            let boundary = playing && step == 0 && prev != 0;
            if boundary && self.looper_arm_pending {
                self.looper_arm_pending = false;
                self.master_rec_pending.store(false, std::sync::atomic::Ordering::Relaxed);
                self.looper_rec_bars_done = 0;
                self.looper.toggle_record(); // Idle -> Recording (length = bars * bar)
            } else if boundary
                && self.looper.state == crate::synth::looper::LooperState::Recording
            {
                self.looper_rec_bars_done += 1;
                let bars = self.looper.bars.max(1);
                if self.looper_finish_pending || self.looper_rec_bars_done >= bars {
                    if self.looper_finish_pending {
                        // Досрочное закрытие: обрезать петлю до целых тактов факта
                        self.looper.truncate_loop(self.looper_rec_bars_done);
                    }
                    self.looper_finish_pending = false;
                    self.looper_rec_bars_done = 0;
                    self.drum_engine.set_recording(false); // сетка тоже разармирована
                    self.looper.toggle_record(); // Recording -> Playing
                }
            } else if !playing
                && (self.looper_finish_pending
                    || (self.looper_capture_quantized
                        && self.looper.state == crate::synth::looper::LooperState::Recording))
            {
                // tc9 fallback: часы умерли посреди записи — закрыть сейчас, без квантования
                self.looper_finish_pending = false;
                self.looper_rec_bars_done = 0;
                self.drum_engine.set_recording(false);
                self.looper.toggle_record();
            }
        }
```

  Известные ограничения (приняты, задокументированы): (1) arm во время шага 0 ждёт СЛЕДУЮЩЕЙ границы (edge-detect); (2) Steps=1 паттерны не квантуются вовсе — их отсекает `pattern_len > 1` в диспетчере (free-путь).

- [ ] **Step 5: Гейты (старые тесты упадут — ожидаемо, Task 2 чинит).**

```bash
cargo test --test master_transport 2>&1 | tail -5
```

### Task 2: Тесты — квантованный master-recording

**Files:**
- Create: `tests/master_rec_sync.rs`

- [ ] **Step 1: Харнесс с tick_block.** Сигнатура — два буфера: `tick_block(&mut l, &mut r)` (как в tests/looper_timing.rs:52):

```rust
//! Quantized master recording: looper starts at the pattern boundary,
//! auto-closes after `bars` bars, early stop is boundary-quantized (#30 sync).

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

/// Advance `blocks` audio blocks (tick_block takes two buffers, len = slice).
fn run(e: &mut SynthEngine, blocks: usize) {
    let mut l = [0.0f32; BLOCK];
    let mut r = [0.0f32; BLOCK];
    for _ in 0..blocks {
        e.tick_block(&mut l, &mut r);
    }
}

/// Blocks per 16th step at 120 BPM: 48000*60/(120*4) = 6000 samples/step;
/// 6000/64 = 93.75 -> 94.
fn steps(blocks: usize) -> usize { blocks * 94 }
```

- [ ] **Step 2: Тесты:**

```rust
#[test]
fn tc1_start_at_boundary() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay); // drums playing from step 0
    run(&mut e, steps(5)); // playhead ~step 5
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(e.master_rec_pending_atom().load(Ordering::Relaxed), "armed");
    assert!(matches!(e.looper.state, LooperState::Idle), "not capturing until boundary");
    run(&mut e, steps(12)); // crosses the boundary
    assert!(!e.master_rec_pending_atom().load(Ordering::Relaxed));
    assert!(matches!(e.looper.state, LooperState::Recording), "capture started at boundary");
}

#[test]
fn tc2_auto_close_after_one_bar() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    let _ = e.handle_control(ControlEvent::LooperSetBars { bars: 1 });
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(16)); // to the boundary: start
    assert!(matches!(e.looper.state, LooperState::Recording));
    run(&mut e, steps(16) + steps(16)); // one full bar after start + margin
    assert!(matches!(e.looper.state, LooperState::Playing), "auto-closed after `bars` bars");
}

#[test]
fn tc3_early_stop_quantized() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    let _ = e.handle_control(ControlEvent::LooperSetBars { bars: 4 });
    run(&mut e, steps(2));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(14)); // boundary: recording
    run(&mut e, steps(24)); // 1.5 bars into the capture
    let _ = e.handle_control(ControlEvent::MasterRecord); // early stop request
    assert!(matches!(e.looper.state, LooperState::Recording), "not closed immediately");
    run(&mut e, steps(10)); // next boundary
    assert!(matches!(e.looper.state, LooperState::Playing), "closed at the boundary");
    // Досрочное закрытие ОБРЕЗАЕТ петлю до записанных целых тактов (2 из 4):
    // эталон — полная 4-тактная петля второго движка.
    let mut e2 = engine();
    let _ = e2.handle_control(ControlEvent::MasterPlay);
    let _ = e2.handle_control(ControlEvent::LooperSetBars { bars: 4 });
    run(&mut e2, steps(2));
    let _ = e2.handle_control(ControlEvent::MasterRecord);
    run(&mut e2, steps(14) + steps(16) * 4); // start + full 4 bars auto-close
    assert!(matches!(e2.looper.state, LooperState::Playing));
    assert_eq!(
        e.looper.loop_len_ticks() * 2,
        e2.looper.loop_len_ticks(),
        "early-closed loop must be exactly half the full-bars loop"
    );
}

#[test]
fn tc4_free_mode_immediate() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord); // drums stopped
    assert!(e.drum_engine.sequencer.playing, "auto-play");
    assert!(matches!(e.looper.state, LooperState::Recording), "free mode: raw immediate capture");
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(matches!(e.looper.state, LooperState::Playing), "immediate finish");
}

#[test]
fn tc5_master_stop_clears_pending() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(4));
    let _ = e.handle_control(ControlEvent::MasterRecord); // armed
    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(!e.master_rec_pending_atom().load(Ordering::Relaxed));
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn tc8_rearm_cancel() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(6));
    let _ = e.handle_control(ControlEvent::MasterRecord); // armed
    let _ = e.handle_control(ControlEvent::MasterRecord); // cancel before boundary
    assert!(!e.master_rec_pending_atom().load(Ordering::Relaxed), "disarmed");
    run(&mut e, steps(10));
    assert!(matches!(e.looper.state, LooperState::Idle), "never started");
}
```

  ВАЖНО tc8: второй MasterRecord при arm_pending снимает арм — эта ветка уже есть в диспетчере Task 1 Step 2 («Снять арм»). Сниппет теста выше — финальный.

- [ ] **Step 3: tests/master_transport.rs остаётся без правок.** Все его MasterRecord-тесты (tc1/tc2/tc3/tc5/tc9) стартуют со свежего движка — free-путь (секвенсор стоял в момент нажатия, `looper_capture_quantized=false`) — семантика прежняя. С дискриминатором tc3 (второй MasterRecord при уже играющих драмах) закрывается немедленно по free-ветке — зелёный. Проверить прогоном; при расхождении минимально править и объяснить в отчёте.

- [ ] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 5: Commit**

```bash
git add src/synth/mod.rs src/synth/looper.rs tests/master_rec_sync.rs
git commit -m "feat: quantized master recording — looper aligns to sequencer bar boundaries (#30)"
```

### Task 3: GUI — статус ARMED

**Files:**
- Modify: `src/gui/drums.rs` (draw_looper state label ~832-844: `status_color`/`state_label` в транспортной строке)
- Modify: `src/gui/mod.rs` (App поле), `src/lib.rs` (литерал + CommonInit), `src/gui/settings.rs` (reconnect re-fetch ~263-276)

- [ ] **Step 1: Пламб атома (урок detached-atom).** App поле `pub master_rec_pending: std::sync::Arc<std::sync::atomic::AtomicBool>,`; CommonInit → App-литерал (`c.master_rec_pending_atom()` — сверить, как другие атомы видны в CommonInit); settings.rs `reconnect_audio` — переполучить клон.

- [ ] **Step 2: Статус.** В state-лейбл лупера (drums.rs, строка статуса рядом с `status_color`):

```rust
                let armed = self.master_rec_pending.load(std::sync::atomic::Ordering::Relaxed);
                ui.colored_label(
                    if armed { egui::Color32::YELLOW } else { status_color },
                    if armed { "ARMED" } else { state_label }, // существующие локалы
                );
```

  (имена локалов по факту; суть: если armed — показываем жёлтое «ARMED» вместо состояния.)

- [ ] **Step 3: Гейты + Commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo build --release
git add src/gui/drums.rs src/gui/mod.rs src/lib.rs src/gui/settings.rs
git commit -m "feat: ARMED status while master recording waits for the bar boundary"
```

### Task 4: TECHDEBT + финал

- [ ] **Step 1:** В #32 (после Phase 4-заметки):

```markdown
Phase sync (квантованная запись лупера, best practice SooperLooper/Boss-RC):
ВЫПОЛНЕНО (2026-10-30, план 2026-10-30-quantized-master-rec.md) — MasterRecord
при играющем секвенсоре: старт захвата лупера на следующей границе паттерна
(current_step==0), авто-закрытие после Bars тактов, досрочное второе нажатие
закрывает тоже на границе. Free-режим (секвенсор стоит) — прежнее немедленное
поведение. GUI: ARMED-статус. Отвергнутая альтернатива: Bars = длина паттерна
(Bars остались ручными).
```

- [ ] **Step 2:** Полные гейты + restart (главная сессия), commit docs.

- [ ] **Step 3: Ручной чеклист (юзер):**
1. Секвенсор играет, жмёшь Rec — статус ARMED (жёлтый), запись стартует с начала такта
2. Bars=1 — петля сама закрылась через такт (OVR→Play без второго нажатия)
3. Bars=4 — второй Rec закрывает на ближайшей границе, длина = целые такты
4. Секвенсор стоит — Rec пишет сразу как раньше
