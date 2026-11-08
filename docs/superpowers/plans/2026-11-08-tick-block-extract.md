# tick_block extract: looper-capture машина → отдельный метод Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 8411332; поля SynthEngine 246-278; tick_block
с 2138; capture-машина ~2150-2290; post-QW3 tc13/tc14 — characterization уже
начат)

**Goal:** ~130-строчная looper-capture машина вырезается из tick_block-монолита
в отдельный метод (`looper_capture_tick`) в новом файле
`src/synth/looper_capture.rs` — behavior-preserving, чистый move.

**Architecture:** ПОЛЯ НЕ переносятся (10 looper_* остаются в SynthEngine —
их трогают master_stop/disarm/toggle_record вне tick_block; перенос в
субструкт = большой blast без выгоды). Метод-экстракция: цельный блок
2168-2298 (edge-detect → весь if-chain ВКЛЮЧАЯ clock-death fallback 2285-2297,
закрывающая скобка цепи 2298) вырезается verbatim в
`impl SynthEngine { fn looper_capture_tick(&mut self, step: u8) }` — единственный
внешний локал step; остальные (prev/playing/boundary/quantized_looping/
pat_switched/cur_pat) выводятся внутри блока из self (review #3: cur_pat
наружный — usize, не нужен). Пререквизит — characterization: tc10/tc11/tc12/tc13/
tc14 УЖЕ покрывают fresh-clip/overdub/cycle-remove/mid-stop/close; РЕАЛЬНЫЕ
пробелы только 3 (см. Task 1): clock-death fallback, cycle-ADD, source_mix.

**Tech Stack:** Rust; characterization-тесты в tests/master_rec_sync.rs
(атом- driven harness уже есть); goldens не задеты (looper-рендер вне
preset-golden; behavior lock = characterization).

**Проверенные факты (8411332):**
- Поля looper_* (10): 246 arm_pending, 249 capture_quantized, 251 prev_step,
  255 prev_pattern, 258 overdub_pending, 259 overdub_active, 262 cycle_ticks,
  264 finish_pending, 266 rec_cycles, 278 source_mix (source_mix вне машины —
  NOT-RT-машина, проверить при вырезке).
- tick_block: 2138+; машина = связный блок 2168-2298 (edge-detect 2168-2169;
  fallback 2285-2297 — ПОСЛЕДНЯЯ arm той же if-цепи 2208-2298, вырезать
  отдельно нельзя; конец = закрывающая скобка цепи 2298). Единственный
  внешний локал — step (2168); keep_capture_capacity (404) и
  set_master_rec_state (397) — private методы того же impl, видны из
  child-модуля; приватные поля видны потомку (Rust-privacy наследуется вниз).
- QW3 добавил tc13/tc14 (tests/master_rec_sync.rs) — harness атом-driven,
  повторяем для новых сценариев.
- rt_safety: extraction — чистый move, аллокаций не добавляет; suite обязателен.

---

## Test Cases (from the requirement)

- TC-A (новые characterization, до рефактора, все зелёные на СТАРОМ коде):
  1. overdub_pending: armed capture → overdub_pending arm → на boundary
     overdub активируется (state Overdubbing).
  2. clock-death fallback: активная quantized capture, клок умирает
     (симуляция = вызов fallback-пути/флага) → capture закрывается,
     rec_cycles=0, atom=0 (QW3-8 добавил reset).
  3. fresh-clip wipe: после закрытия capture новый MasterRecord (armed→close)
     стирает старый клип и пишет новый (looper_capture_quantized остаётся
     true, rec_cycles перезапускается).
  4. CaptureCycleAdd/Remove: во время quantized capture +N/−N тактов —
     rec_cycles и итоговая длина соответствуют.
  5. source_mix: toggle в процессе capture — контент пишется с источником/
     без (behavior lock — assert по display()-контенту).
- TC-B: ВСЕ существующие тесты master_rec_sync (14) + master_transport (10)
  зелёные ДО и ПОСЛЕ рефактора — бит-в-бит behavior lock.
- Негатив: ~~~timeout~~ n/a; переполнение 64-cycle — существующее поведение
  (пин TECHDEBT, не трогаем).

---

### Task 1: characterization — 3 реальных пробела (tc10/tc11 УЖЕ покрывают
fresh-clip и overdub — НЕ дублировать, review #1)

**Files:** Modify `tests/master_rec_sync.rs` (по образцу tc13/tc14; harness
атом-driven + прямые field pokes где атомов нет).

- [ ] Step 1 (TC-A-2 clock-death): СЦЕНАРИЙ НЕ driven атомами (review #4):
  driver = прямой field poke `e.drum_engine.sequencer.playing = false;`
  (visibility pub-цепь: drum_engine mod.rs:180 → sequencer drum.rs:1041 →
  playing :736) в момент активной quantized capture, затем ≥1 блок —
  fallback (2285) должен закрыть capture. АССЕРТЫ ЧЕРЕЗ ПРОКСИ (review #5 —
  looper_rec_cycles приватен): master_rec_state_atom()==0, state Playing,
  длина лупа = сохранённый кусок. Негатив (дешёвый, добавить): clock-death
  при НЕ-capturing → no-op.
- [ ] Step 2 (TC-A-4-ADD): cycle-ADD половина — tc12 покрывает только Remove
  (review #1); CaptureCycleAdd/Remove хендлеры живут в handle_control
  (~1821-1836, вне машины) — тест фиксирует семантику rec_cycles, охраняя
  move косвенно (review #6 — отметить в комментарии теста).
- [ ] Step 3 (TC-A-5 source_mix): toggle LooperSetSource (~1810) в процессе
  capture → контент display() с/без источника (behavior lock).
- [ ] Step 4: все ЗЕЛЁНЫЕ на текущем master (падение = ошибка теста, не
  движка; расхождения — в отчёт). Gates: clippy; cargo test.
- [ ] Step 5: коммит `test: characterize looper capture — clock-death fallback, cycle-add, source_mix (prereq for extraction)`.

### Task 2: extract — move машины в looper_capture.rs

**Files:** Create `src/synth/looper_capture.rs` (impl SynthEngine);
Modify `src/synth/mod.rs` (вырезанный блок → вызов; mod-декларация файла).

- [ ] Step 1: вырезать цельный блок машины из tick_block (от
  `let prev = self.looper_prev_step;` до конца закрытия capture, ГРАНИЦЫ
  по фактическим скобкам — реализатор проверяет, что блок не использует
  внешние let'ы кроме step/cur_pat; если использует — передать параметрами).
  Вставить в новый файл:
```rust
//! Looper-capture машина: quantized arm→record→close, overdub, cycle
//! add/remove, clock-death fallback. Вынесено из tick_block verbatim
//! (techdebt Strategic 3) — behavior-preserving move.
use super::*;

impl SynthEngine {
    pub(super) fn looper_capture_tick(&mut self, step: u8) {
        // VERBATIM из tick_block 2168-2298 (см. базовый коммит)
    }
}
```
  в tick_block на месте блока: `self.looper_capture_tick(step);`
- [ ] Step 2: `mod looper_capture;` в src/synth/mod.rs (рядом с mod-списком).
- [ ] Step 3: gates: clippy; cargo test ПОЛНЫЙ (все characterization зелёные —
  behavior lock сработал); rt_safety (чистый move); golden_fx.
- [ ] Step 4: коммит `refactor: extract looper-capture machine from tick_block — verbatim move to looper_capture.rs (Strategic 3)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens.
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.
- [ ] Step 3: метрика в PR: строки tick_block до/после (~540 → ~410).

## Out of scope
- Перенос 10 looper_* полей в субструкт — blast по внешним хендлерам без
  выгоды; возможно позже отдельным планом.
- Исправление known bugs машины (64-cycle wrap #6, arbitration #5, boundary
  block-rate #4) — отдельные планы/юзер-решения; characterization их
  ЗАФИКСИРУЕТ как текущее поведение (тесты пишут то, что ЕСТЬ).
