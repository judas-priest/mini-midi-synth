# Time Signatures (3/4 etc.) for the Drum Sequencer — Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ ecf67e7 чтением кода в этой сессии)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** TECHDEBT #21 (docs/TECHDEBT.md:358) — секвенсор ударных должен уметь
размерности вида 3/4. Минимальный дизайн: такт секвенсора и так определён
`DrumPattern.length` (уже работает: tick/record/grid/metronome длину уважают),
добавляется персистентное `beats_per_bar` (2..=6, default 4) + GUI-кнопки
быстрого выбора размерности. Лупер НЕ трогаем (задокументировать почему).

**Проверенное текущее состояние (важно — TECHDEBT #21 частично устарел):**

| Заявление #21 | Факт (master @ ecf67e7) |
|---|---|
| «GUI-сетка жёстко 4×4» | УСТАРЕЛО: сетка рендерит `0..pattern_length` (gui/drums.rs:232,251) |
| «метроном акцентирует `step % 4 == 0` (усечёт 3/4 неверно)» | НЕ ВЕРНО для length=12: доли на шагах 0,4,8 ✓, акцент `fired == 0` (drum.rs:1180-1181) — это и есть даунбит, т.к. такт = `length` шагов. Метроном уже корректен для ЛЮБОЙ длины → меняем НЕ двигок, а пишем верификационные тесты |
| «record-путь привязан к 16» | УСТАРЕЛО: `record_hit` полностью length-aware (`% len`, drum.rs:834-845) |
| swing привязан к 16 | НЕ проблема: swing только `current_step % 2` (drum.rs:800), от длины не зависит |
| лупер считает bars в 4/4 | `loop_length_ticks` — свободная длина в тиках (looper.rs:55), синк не ломается; `bars` (1,2,4,8) — это лишь запрос длины при записи и сетка-разметка таймлайна `looper_bars * 4` битовых линий (gui/drums.rs:1006) — чисто визуальный 4/4. Вне скоупа, документируем |

**Architecture:**
- `DrumPattern` (src/synth/drum.rs:709-722, Copy + serde derive) получает
  `beats_per_bar: u8` с `#[serde(default)]` — старые kit-файлы (preset.rs:621-628
  `DrumKit`, сохранение/загрузка preset.rs:635-641) подхватятся без миграции.
  Двигок это поле СЕГОДНЯ НЕ ЧИТАЕТ (метроном/акцент корректны без него):
  поле существует, чтобы размерность переживала save/load kit'а и правки
  свободного Steps-ножа, и чтобы GUI знал, какую кнопку подсветить.
- Новый `ControlEvent::DrumSeqTimeSig { beats_per_bar }` (по образцу
  `DrumSeqLength`, src/synth/mod.rs:211): обработчик (mod.rs:2304-2307 рядом)
  клампит 2..=6 и пишет ОБА поля: `beats_per_bar` и `length = bpb * 4`
  (сетка — шестнадцатые, 4 шага на долю при любом размере).
- Попутное упрочнение: `DrumSeqLength` в обработчике НЕ клампится
  (mod.rs:2304-2307), а `record_hit` делит на `len` (drum.rs:839) —
  нулевая длина из кольца = div-by-zero panic. Добавляем `clamp(1, 16)`.
- GUI (src/gui/drums.rs:105-123): перед существующим DragValue «Steps»
  вставляем кнопки `2/4 3/4 4/4 5/4 6/4` (selectable_label, как pattern-ряд на
  drums.rs:108-115). Кнопка подсвечена, когда `length == beats_per_bar * 4`;
  свободная длина (DragValue 1..=16, уже есть) = ни одна не подсвечена —
  это и есть «[free]» из заявки, отдельная кнопка не нужна.
- Undo/console: `undo_snapshot: Option<Box<[DrumPattern; 8]>>` (drum.rs:733,
  save/undo 767-787) копирует массив паттернов ЦЕЛИКМОВО — `length` и новое
  поле переносятся wholesale, менять нечего. LED-консоль шлёт только
  `DrumSeqUndo`/`DrumSeqClear` (gui/mod.rs:1893, gui/drums.rs:51-54),
  clear_pattern (drum.rs:768-773) стирает только steps — тоже без правок.
  Покрывается тестом (TC5).
- Looper: НЕ ТРОГАЕМ. `loop_length_ticks` — свободная длина; `bars` — параметр
  запроса длины при записи и визуальная сетка. Синк `looper_sync_bpm`
  гоняет только BPM — размер не участвует. Пишем в TECHDEBT.
- Импорт MIDI (preset.rs:729-747): бьёт на такты по 16 шагов с
  `DrumPattern::default()` — остаётся 4/4. Вне скоупа (документируем).

**Tech Stack:** Rust, egui, rtrb/ControlEvent, serde — без новых зависимостей.

**Spec context:** docs/TECHDEBT.md «21. Размерность: лупер и секвенсор…» (строки
358-373); заявка пользователя 2026-10-29 «лупер и секвенсор не должны быть
обделены размерностью». Шаги секвенсора = шестнадцатые (`steps_per_beat = 4.0`,
drum.rs:796), поэтому «доля» всегда 4 шага; размер меняет только число долей
в такте = `length / 4`. Прецедент тест-харнесса метронома — tests/metronome.rs.

**RT-safety:** обработчик `DrumSeqTimeSig` пишет два u8 в уже существующий
массив — ни аллокаций, ни локов; событие — два маленьких Copy-поля в кольцо
512 слотов. После правок в `src/synth/` гонять
`cargo test --features rt-assert --test rt_safety`.

**Проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

Полные гейты перед финальным коммитом группы: добавить `cargo deny check`.

---

## Test Cases (from the requirement)

Требование: «секвенсор должен уметь 3/4 и другие размеры; метроном должен
отбивать такт правильного размера».

1. **TC1 (метроном, 3/4, верификация — прод-код НЕ меняется):** pattern
   length=12, metronome on, 120 BPM → клики каждые ~750 блоков (доли 0,4,8),
   каждый ТРЕТИЙ клик (шаги 0,12,24 — начала тактов) громче соседних
   (акцент 0.9 против 0.55, drum.rs:40-43,1181-1186).
2. **TC2 (2/4):** length=8 → клики каждые ~750 блоков, каждый ВТОРОЙ — акцент.
3. **TC3 (free 5 шагов):** length=5 → клики на локальных шагах 0 и 4: интервалы
   чередуются ~750 / ~187.5 блоков; акцент на каждом такте (шаг 0), т.е. на
   каждом ЧЁТНОМ событии.
4. **TC4:** `DrumSeqTimeSig{3}` → в текущем паттерне `beats_per_bar == 3` И
   `length == 12`; `DrumSeqTimeSig{4}` → 16.
5. **TC5 (undo wholesale):** 3/4 → `DrumSeqRecord{true}` (снапшот) → 4/4 →
   `DrumSeqUndo` → снова length=12, beats_per_bar=3. Snapshot-механика
   (drum.rs:767-787) уже копирует паттерны целиком — тест фиксирует это.
6. **TC6 (serde legacy):** kit-JSON БЕЗ `beats_per_bar` десериализуется в
   `beats_per_bar == 4`, `length` сохраняется (12). Roundtrip: bpb=3 → 3.
7. **TC7 (клампы, negative):** `DrumSeqLength{0}` → length==1, tick_block и
   `record_hit` не паникуют; `DrumSeqTimeSig{0}` → bpb==2/length==8;
   `DrumSeqTimeSig{255}` → bpb==6/length==24.
8. **GUI (ручная проверка, egui не тестируем):** кнопка 3/4 подсвечивается,
   Steps=12, сетка сжимается до 12 колонок, биты подсвечены на 0/4/8; drag
   Steps на 7 → подсветка снята; Save kit → Load kit → 3/4 живо.

## Negative Matrix

- `DrumSeqLength{0}` из кольца → деление на ноль в `record_hit` (drum.rs:839) —
  закрывается клампом `clamp(1,16)` в обработчике (Task 2, TC7).
- `DrumSeqTimeSig` вне 2..=6 (0, 1, 7, 255) → кламп к 2..=6, length = bpb*4 (TC7).
- Легаси kit-JSON без нового поля → serde default 4, старые файлы читаются (TC6).
- length=1 → `current_step` не меняется (0 % 1), prev==cur → кликов нет
  (drum.rs:1177 guard) — допустимый edge, 1-шаговый такт долей не имеет.
- length=5 → клики только на шагах 0 и 4 такта; интервал 4→0 = 1 шаг (~187.5
  блоков) — «догоняющий» клик корректен (нецелое число долей в free-режиме).
- Уменьшение length на лету (16→12) при игре: `current_step % length`
  (drum.rs:819) — возможен один переход без клика на некратном шаге;
  задокументированное поведение, не баг.
- Undo/Clear/переключение паттерна/LED-консоль PAGE — переносят паттерны
  целиком (drum.rs:733,768-787), нового кода не требуют (TC5 фиксирует).
- Метроном off / transport стоит → 0 кликов (уже покрыто tests/metronome.rs
  tc3/tc4, не дублируем).
- Потеря события в кольце (`CountingProducer`) → недостающий клик одной доли,
  самовосстановление на следующей границе; существующая модель потерь.

---

## Task 1 — Верификационные тесты метронома для 12/8/5-шаговых тактов (zero prod changes)

Метроном уже корректен при любой длине (см. таблицу выше). Пишем тесты,
запускаем, убеждаемся что зелёные БЕЗ правок двигка. Если красные —
предположение плана неверно: СТОП, эскалация пользователю.

- [ ] Добавить в конец `tests/metronome.rs` (после tc5_config_missing…:93)
      три теста. Харнесс переиспользуем: `metronome_engine`, `click_events`
      (tests/metronome.rs:9-40). Полный код:

```rust
// --- Time signatures (TECHDEBT #21): метроном уже корректен при любой длине.
// 120 BPM, 48 kHz, BLOCK_SIZE=32 → шаг = 6000 сэмплов = 187.5 блоков, доля
// (4 шага) = 750 блоков. DrumSeqPlay сбрасывает step в 0 (mod.rs:2295-2297),
// первый клик приходит рано (~блок 187) — интервалы меряем windows().

fn engine_with_length(bpm: f32, length: u8) -> SynthEngine {
    let mut e = metronome_engine(bpm, true);
    let _ = e.handle_control(ControlEvent::DrumSeqLength { length });
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    e
}

/// Акценты (пики > 1.2× соседнего бита) идут с периодом `bar_events` событий.
fn assert_bar_period(events: &[(usize, f32)], bar_events: usize) {
    assert!(events.len() >= 2 * bar_events, "too few events: {events:?}");
    for i in 0..events.len() {
        let is_bar_start = i % bar_events == 0;
        let next = events[(i + 1) % events.len()].1;
        let louder = events[i].1 > next * 1.2;
        if is_bar_start {
            assert!(louder, "event {i} (bar start) not accented: {events:?}");
        } else {
            assert!(!louder, "event {i} unexpectedly accented: {events:?}");
        }
    }
}

#[test]
fn tc6_three_quarter_length_12_clicks_and_accents() {
    // 3/4: доли 0,4,8; такт = 12 шагов = 3 события. ~6000 блоков ≈ 3 такта.
    let mut e = engine_with_length(120.0, 12);
    let events = click_events(&mut e, 6000);
    assert!(events.len() >= 6, "expected >= 6 beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750");
    }
    assert_bar_period(&events, 3);
}

#[test]
fn tc7_two_quarter_length_8_clicks_and_accents() {
    let mut e = engine_with_length(120.0, 8);
    let events = click_events(&mut e, 6000);
    assert!(events.len() >= 6, "expected >= 6 beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750");
    }
    assert_bar_period(&events, 2);
}

#[test]
fn tc8_free_length_5_clicks_at_0_and_4() {
    // Такт 5 шагов: клики на шагах 0 и 4 → интервалы чередуются 750 / 187.5.
    // Акцент на шаге 0 = каждое ЧЁТНОЕ событие.
    let mut e = engine_with_length(120.0, 5);
    let events = click_events(&mut e, 6000);
    assert!(events.len() >= 6, "expected >= 6 clicks, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        let expect = if w[0].0 % 2 == 0 { 750.0 } else { 187.5 };
        assert!((dt - expect).abs() < 4.0, "interval {dt} blocks, expected ~{expect}: {events:?}");
    }
    assert_bar_period(&events, 2);
}
```

- [ ] `cargo test --test metronome` — все 8 тестов зелёные, прод-код не менялся.
      **Красные = план неверен: остановиться и доложить.**
- [ ] Commit: `test: metronome already correct for 3/4, 2/4, free-length bars (TECHDEBT #21 verify)`

## Task 2 — `beats_per_bar` в DrumPattern + `DrumSeqTimeSig` + кламп length (TDD)

- [ ] **Тесты сначала** (красные: поля/события ещё нет). Новый файл
      `tests/drum_time_sig.rs`:

```rust
//! Time signatures для драм-секвенсора (TECHDEBT #21).
//! Plan: docs/superpowers/plans/2026-10-29-time-signatures.md

use mini_midi_synth::preset::DrumKit;
use mini_midi_synth::synth::{ControlEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

#[test]
fn tc1_time_sig_sets_beats_and_length() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 3 });
    let seq = &e.drum_engine.sequencer;
    assert_eq!(seq.patterns[0].beats_per_bar, 3);
    assert_eq!(seq.patterns[0].length, 12);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 4 });
    assert_eq!(seq.patterns[0].beats_per_bar, 4);
    assert_eq!(seq.patterns[0].length, 16);
}

#[test]
fn tc2_time_sig_clamped_to_2_6() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 0 });
    assert_eq!(e.drum_engine.sequencer.patterns[0].beats_per_bar, 2);
    assert_eq!(e.drum_engine.sequencer.patterns[0].length, 8);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 255 });
    assert_eq!(e.drum_engine.sequencer.patterns[0].beats_per_bar, 6);
    assert_eq!(e.drum_engine.sequencer.patterns[0].length, 24);
}

#[test]
fn tc3_length_zero_from_ring_is_clamped_and_rt_safe() {
    // reg: record_hit делит на len (drum.rs:839) — length=0 из кольца = panic.
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqLength { length: 0 });
    assert_eq!(e.drum_engine.sequencer.patterns[0].length, 1);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: true });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..200 { e.tick_block(&mut l, &mut r); }
    // record_hit при length=1 (playing+recording) не должен паниковать
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    for _ in 0..200 { e.tick_block(&mut l, &mut r); }
}

#[test]
fn tc4_undo_restores_time_sig_wholesale() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 3 });
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true }); // snapshot
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 4 });
    let _ = e.handle_control(ControlEvent::DrumSeqUndo);
    let seq = &e.drum_engine.sequencer;
    assert_eq!(seq.patterns[0].length, 12);
    assert_eq!(seq.patterns[0].beats_per_bar, 3);
}

#[test]
fn tc5_legacy_kit_json_without_field_defaults_to_4() {
    let mut pat = mini_midi_synth::synth::drum::DrumPattern::default();
    pat.length = 12;
    let kit = DrumKit {
        name: "legacy".into(),
        patterns: vec![pat],
        params: vec![],
        bpm: 120.0,
        swing: 0.0,
        volume: 0.8,
    };
    let mut json = serde_json::to_string(&kit).unwrap();
    // стереть новое поле — симуляция старого kit-файла
    let needle = format!(r#","beats_per_bar":{}"#, pat.beats_per_bar);
    let pos = json.find(&needle).expect("field must be serialized");
    json.replace_range(pos..pos + needle.len(), "");
    let back: DrumKit = serde_json::from_str(&json).expect("legacy kit must parse");
    assert_eq!(back.patterns[0].length, 12);
    assert_eq!(back.patterns[0].beats_per_bar, 4);
}

#[test]
fn tc6_kit_roundtrip_keeps_time_sig() {
    let mut pat = mini_midi_synth::synth::drum::DrumPattern::default();
    pat.length = 12;
    pat.beats_per_bar = 3;
    let kit = DrumKit {
        name: "waltz".into(),
        patterns: vec![pat],
        params: vec![],
        bpm: 120.0,
        swing: 0.0,
        volume: 0.8,
    };
    let json = serde_json::to_string(&kit).unwrap();
    let back: DrumKit = serde_json::from_str(&json).unwrap();
    assert_eq!(back.patterns[0].beats_per_bar, 3);
    assert_eq!(back.patterns[0].length, 12);
}
```

- [ ] `cargo test --test drum_time_sig` — красные (не компилируется: нет поля
      и события). Это ожидаемо.
- [ ] **Поле** в `src/synth/drum.rs` — заменить struct `DrumPattern`
      (drum.rs:709-722) и Default:

```rust
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DrumPattern {
    pub steps: [[StepData; MAX_STEPS]; NUM_DRUM_SLOTS],
    pub length: u8, // 1..16
    /// Beats per bar of the intended signature (2..=6, default 4). The bar
    /// the sequencer loops IS `length` steps, so the engine never reads this
    /// today: metronome accent stays at step 0, which is always the downbeat.
    /// Persists the signature for kit files and GUI highlighting; must equal
    /// `length / 4` whenever the GUI signature buttons are used.
    #[serde(default = "default_beats_per_bar")]
    pub beats_per_bar: u8,
}

fn default_beats_per_bar() -> u8 { 4 }

impl Default for DrumPattern {
    fn default() -> Self {
        Self {
            steps: [[StepData::default(); MAX_STEPS]; NUM_DRUM_SLOTS],
            length: 16,
            beats_per_bar: 4,
        }
    }
}
```

- [ ] **Событие** в `src/synth/mod.rs` — после `DrumSeqLength { length: u8 }`
      (mod.rs:211) добавить:

```rust
    DrumSeqTimeSig { beats_per_bar: u8 },
```

- [ ] **Обработчики** в `src/synth/mod.rs` — заменить handler `DrumSeqLength`
      (mod.rs:2304-2307) на клампящий и добавить соседний:

```rust
            ControlEvent::DrumSeqLength { length } => {
                // Clamp: record_hit делит на len (drum.rs:839) — ноль из кольца
                // это div-by-zero; 0..=255 → 1..=16.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                self.drum_engine.sequencer.patterns[pat].length = length.clamp(1, 16);
            }
            ControlEvent::DrumSeqTimeSig { beats_per_bar } => {
                // Signature = beats_per_bar; the bar IS `length` steps of 16ths,
                // so length follows: 4 steps per beat at any signature.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                let bpb = beats_per_bar.clamp(2, 6);
                self.drum_engine.sequencer.patterns[pat].beats_per_bar = bpb;
                self.drum_engine.sequencer.patterns[pat].length = bpb * 4;
            }
```

- [ ] `cargo test --test drum_time_sig --test metronome` — зелёные.
- [ ] Полные гейты (clippy / test / rt_safety — тронут `src/synth/`).
- [ ] Commit: `feat: drum pattern beats_per_bar + DrumSeqTimeSig event, clamp DrumSeqLength (TECHDEBT #21)`

## Task 3 — GUI: кнопки размерности перед Steps-DragValue

- [ ] `src/gui/drums.rs` — в horizontal «Pattern selector + length»
      (drums.rs:105-123) вставить между `ui.separator();` (drums.rs:116) и
      `let mut length = …` (drums.rs:117):

```rust
            // Time-signature quick buttons (TECHDEBT #21). The sequencer bar IS
            // `length` steps of 16ths, so a signature sets length = beats * 4.
            // "Free" = drag the Steps DragValue to any 1..=16 — no button
            // matches then (length != beats_per_bar * 4).
            const SIGS: [(u8, &str); 5] =
                [(2, "2/4"), (3, "3/4"), (4, "4/4"), (5, "5/4"), (6, "6/4")];
            let p = &self.drum_patterns[pat_idx];
            let active_sig = if p.length == p.beats_per_bar.saturating_mul(4) {
                Some(p.beats_per_bar)
            } else {
                None
            };
            for (bpb, label) in SIGS {
                let selected = active_sig == Some(bpb);
                if ui.selectable_label(selected, label)
                    .on_hover_text(format!(
                        "{label}: {} steps ({} beats of 16ths)", bpb * 4, bpb
                    ))
                    .clicked() && !selected
                {
                    let pat = &mut self.drum_patterns[pat_idx];
                    pat.beats_per_bar = bpb;
                    pat.length = bpb * 4;
                    let _ = self.ctrl_tx.push(ControlEvent::DrumSeqTimeSig { beats_per_bar: bpb });
                }
            }
            ui.separator();
```

  (Замыкание в `ui.horizontal` захватывает `&mut self` целиком — тот же
  приём, что у pattern-ряда drums.rs:108-115; заимствование `p` закончить
  до `let pat = &mut …`, clippy/borrow-check проконтролируют.)
- [ ] GUI-зеркало обновляется локально + событие в двигок — оба поля сразу,
      как в Task 2 handler. Зеркало уже несёт поле: тип
      `drum_patterns: [DrumPattern; 8]` (gui/mod.rs:305), kit load копирует
      `DrumPattern` wholesale (gui/drums.rs:162-168), дефолт — из
      `DrumPattern::default()` (lib.rs:756). Отдельного поля в `App` не нужно.
- [ ] `cargo clippy --all-targets -- -D warnings && cargo test` — зелёные.
- [ ] Ручная проверка (TC8): запустить GUI (`cargo run`), вкладка Drums:
      3/4 подсвечивается, Steps=12, сетка 12 колонок с битовыми полосами
      0/4/8 (bg `step % 4 == 0`, drums.rs:262 — не меняется), плейхед-даунбит
      на колонке 0 (drums.rs:290 — не меняется); Steps→7: подсветка снята;
      Save kit → Load kit → 3/4 восстановлено; Clear/Undo/переключение
      паттернов 1-8 ведут себя как раньше.
- [ ] Commit: `feat: GUI time-signature quick buttons for drum sequencer (TECHDEBT #21)`

## Task 4 — Документация: TECHDEBT #21 и границы скоупа

- [ ] `docs/TECHDEBT.md` §21 (строки 358-373) — обновить «Текущее состояние»:
      секвенсор СДЕЛАН (beats_per_bar + кнопки; сетка/record/undo/метроном
      уже были length-aware — таблица фактов из шапки этого плана); вынести
      в остаток ТОЛЬКО лупер: bars-индикация и (опционально) bars-запрос в
      не-4/4 — `loop_length_ticks` свободен, синк не ломается, это UX-вопрос;
      импорт MIDI бьёт по 16 шагов 4/4 (preset.rs:729-747) — известное
      ограничение, вне скоупа.
- [ ] Полные гейты: `cargo clippy --all-targets -- -D warnings && cargo test
      && cargo test --features rt-assert --test rt_safety && cargo deny check`.
- [ ] Commit: `docs: TECHDEBT #21 — sequencer time signatures done, looper scope note`
