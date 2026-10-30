# S5: Looper/transport clock sync (REFACTOR_AUDIT S5, предпосылки TECHDEBT #32)

anchor-checked: 2026-10-30 — все якоря сняты чтением кода на master @ a4426fc в этой сессии.
Номера строк в REFACTOR_AUDIT.md частично совпадают, но всё ниже перепроверено по текущим исходникам.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Каждый таск исполняется СВЕЖИМ субагентом, получающим только этот документ.

**Goal:** один канонический источник темпа — `drum_engine.sequencer.bpm` (как уже для arp/LFO/pitch_seq). Лупер получает BPM + `beats_per_bar` синхронно каждый блок из `tick_block`; `LooperSetBpm` и GUI-флаг `looper_sync_bpm` удаляются; смена BPM mid-loop пропорционально пересчитывает записанный луп (regrid) вместо «замороженной в сэмплах позиции»; длина такта лупера перестаёт хардкодить 4/4. Дизайн-решение пользователя (TECHDEBT #32 phase 1 / #30): ОДНО поле BPM всегда (Sync всегда on), отдельное looper-BPM поле и флаг — убрать.

## Координация с S0-планом (важно)

`docs/superpowers/plans/2026-10-30-refactor-s0-characterization.md` Task 3 добавляет
looper-timing тесты (`#[cfg(test)] mod timing_tests` в `src/synth/looper.rs` +
`tests/looper_timing.rs`). Аудит S5: «После looper-таймингов из S0».

- **Если S0 уже выполнен** — timing_tests/looper_timing.rs существуют; ничего делать не надо,
  тесты этого плана (`mod clock_sync_tests`) не пересекаются по именам.
- **Если S0 ещё НЕ выполнен** — этот план можно исполнять и без него (sync_clock с тем же BPM —
  strict no-op, поведение лупера на фикс. темпе не меняется, S0-тайминги потом лягут поверх).
  Не создавать `timing_tests`/`tests/looper_timing.rs` в этом плане — это работа S0.

Проверка состояния: `grep -n "mod timing_tests" src/synth/looper.rs`.

## Фактическая математика позиций (прочитать перед стартом)

Всё в `src/synth/looper.rs` (файл 636 строк, @ a4426fc):

- **Единица хранения события** — `tick: u32`, «сэмпл-смещение от начала лупа»
  (`LoopEvent`, looper.rs:91-97). События хранятся отсортированными по tick
  (вставка `partition_point` + `copy_within`, looper.rs:434-441).
- **Длина лупа** — `loop_length_ticks = bar_length_samples() * bars`
  (looper.rs:237); `bar_length_samples = (60/bpm) * 4.0 * sr` — **4.0 это хардкод 4/4**
  (looper.rs:211-213). Устанавливается ОДИН раз в `start_record` из Idle.
- **Позиция воспроизведения** — `playback_tick: u32`, +1 за сэмпл, wrap на
  `>= loop_length_ticks` → 0 (looper.rs:528-540). Т.е. позиция живёт в сэмплах
  старого темпа — это и есть «заморожена» из аудита.
- **Quantize-сетка** — `quantize_tick` (looper.rs:215-229):
  `beat_samples = (60/bpm) * sr`; `grid = beat_samples / subdivs` (subdivs: 1/4→1.0,
  1/8→2.0, 1/16→4.0); `nearest = ((tick + grid/2) / grid) * grid`, модulo длины лупа.
  Сетка считается от **текущего** `self.bpm` и якорится в tick 0 (начало лупа).
  При смене BPM mid-loop сетка едет относительно уже записанных тиков — баг из аудита.
- **Курсор** — `playback_cursor: usize` (индекс в отсортированном массиве),
  пересчитывается через `partition_point(|e| e.tick < playback_tick)` — паттерн уже
  есть в `undo()` (looper.rs:304-305), переиспользуем.
- **Тик лупера** — per-sample в `tick_block` (`synth/mod.rs:3002`); arp-прецедент
  per-block sync — `synth/mod.rs:2714-2715` (`let seq_bpm = self.drum_engine.sequencer.bpm; self.arpeggiator.set_bpm(seq_bpm);`).
- `recording_start_tick` (looper.rs:162) — write-only (пишется :238, нигде не читается);
  вне скоупа, не трогать.
- **Восстановление после reconnect** — `LooperRestore` (synth/mod.rs:232-235, handler
  :2363-2366), пушится из `gui/settings.rs:341-348` с рескейлом тиков под новую SR.
  BPM-провениенс восстановленного лупа неизвестен → нужен sentinel (см. дизайн).

## Дизайн-решение: regrid при смене BPM (выбрано и обосновано)

**Выбрано: пропорциональный рескейл лупа в момент смены BPM (`regrid`).**

При `sync_clock(new_bpm)`, если луп существует и его BPM-провениенс известен:
`new_len = old_len * old_bpm / new_bpm`; каждый тик события и playback-позиция
масштабируются `t * new_len / old_len` (u64-промежуточное, monotone → сортировка
сохраняется), курсор пересчитывается `partition_point`-ом, `update_display()`.
Музыкальные позиции переживают смену темпа; луп играет быстрее/медленнее как
DAW-клип (это нотные события, не аудио — тайм-стретч не нужен и не делается).

Отвергнутые альтернативы:

1. **Хранить позицию в долях/битах** (f32 beats, конвертация на лету) — «правильное»
   решение, но ломает контракты `LoopEvent.tick`/`LooperEventSnapshot.tick` (сэмплы),
   `LooperDisplay.loop_length_ticks`, атом позиции, путь restore с SR-рескейлом
   (settings.rs:329-340) и S0-тайминги. Не минимально.
2. **Пересчитать сетку только на следующей границе лупа** (defer) — между сменой BPM
   и границей overdub-запись квантуется на сетку, не согласованную с записанными
   тиками (тот же audited-баг на время перехода), позиция всё ещё заморожена.
3. **Ничего не рескейлить, только сетка от нового BPM** — status quo, сетка и луп
   навсегда расходятся. Отвергнуто.

**Погрешность рескейла:** ±1 сэмпл на событие за regrid (floor деления). Драг BPM
(speed 0.5) даёт regrid на каждый кадр; worst case ~100 шагов × 1 сэмпл ≈ 2 мс @ 48 кГц
дрейфа нотных стартов — неслышимо. Принято, документировано в doc-comment `sync_clock`.

**Sentinel для restore:** новое приватное поле `loop_bpm: f32` — BPM, на котором
записан текущий луп. `0.0` = «неизвестен» (только `restore()`); первый `sync_clock`
после restore просто принимает новый темп БЕЗ рескейла (restore уже рескейлил тики
под SR, повторный рескейл был бы регрессией — сегодня reconnect позицию не трогает).
`start_record` ставит `loop_bpm = self.bpm`; `clear()` обнуляет.

**beats_per_bar:** синхронизируется из `patterns[current_pattern].beats_per_bar`
(drum.rs:710-720, фича TECHDEBT #21 уже влита) каждый блок вместе с BPM. Влияет
только на длину лупа при следующем `start_record` (`bar_length_samples`);
существующий луп не перетактовывается по подписи (редкая операция, вне скоупа).

**Удаление `LooperSetBpm`:** событие удаляется полностью (вариант mod.rs:229, handler
:2362, все 6 пуш-сайтов). Держать deprecated no-op = мёртвый код + ложный сигнал, что
у лупера свой темп. Внешних (не-GUI) вызовов нет — проверено grep по `src/` (только
gui/drums.rs ×5, gui/mod.rs ×1).

**MIDI-clock (0xF8/0xFA) — вне скоупа, документируем:** grep по `src/` — ноль
вхождений real-time MIDI-сообщений (0xF8 timing clock, 0xFA start). Внешний
MIDI-clock-slave ввёл бы ВТОРОЙ источник темпа — это дизайн-решение #32, не S5.
Записать заметку в TECHDEBT #32 (Task 6).

## Полная инвентаризация сайтов (все, трогаем каждый)

| # | Сайт | Что сейчас | Действие |
|---|------|-----------|----------|
| A1 | `looper.rs:211-213` `bar_length_samples` | хардкод `4.0` | `self.beats_per_bar as f32` |
| A2 | `looper.rs:153` `pub bpm: f32` | пишется из `LooperSetBpm` (mod.rs:2362) | пишется из `sync_clock` per-block; + поля `loop_bpm`, `beats_per_bar` |
| A3 | `looper.rs:232-247` `start_record` | длина из bar_length × bars | + `loop_bpm = self.bpm` (:237 рядом) |
| A4 | `looper.rs:310-323` `clear` | — | + `loop_bpm = 0.0` |
| A5 | `looper.rs:332-360` `restore` | — | + `loop_bpm = 0.0` (sentinel) |
| A6 | `looper.rs` (новое) | нет | `sync_clock` + `regrid` |
| A7 | `synth/mod.rs:2714-2715` | только arp | + `sync_clock(seq_bpm, bpb)` |
| A8 | `synth/mod.rs:229` `LooperSetBpm` вариант | — | удалить |
| A9 | `synth/mod.rs:2362` handler | `self.looper.bpm = bpm` | удалить |
| A10 | `gui/drums.rs:277-285` mix row BPM | DragValue + DrumSeqBpm + sync-push | перенести в docked (A14), sync-push исчезает |
| A11 | `gui/drums.rs:381-384` kit load | `looper_sync_bpm` push | удалить блок (`self.drum_bpm = kit.bpm` :370 остаётся) |
| A12 | `gui/drums.rs:416-419` MIDI import | `looper_sync_bpm` push | удалить блок (`self.drum_bpm = bpm` :404 остаётся) |
| A13 | `gui/drums.rs:896-907` looper row 2 | Sync-тоггл + looper BPM Drag | удалить целиком |
| A14 | `gui/drums.rs:24-60` docked transport row | нет BPM | добавить единое BPM-поле → `DrumSeqBpm` |
| A15 | `gui/mod.rs:362-363` поля App | `looper_sync_bpm`, `looper_bpm` | удалить оба |
| A16 | `gui/mod.rs:1512` `save_global_to_config` | пишет флаг в конфиг | удалить строку |
| A17 | `gui/mod.rs:1842` `send_drum_state` | push `LooperSetBpm` | удалить строку (per-block sync накроет reconnect) |
| A18 | `lib.rs:813-814` App literal | init полей | удалить обе строки |
| A19 | `config.rs:142-144` поле UiSettings + `:150` default fn + `:209` Default literal | `looper_sync_bpm` | удалить (serde без `deny_unknown_fields` — старые конфиги с ключом парсятся, поле игнорится) |
| A20 | `tests/rt_safety.rs` (конец файла, :113) | 5 тестов | + `looper_bpm_regrid_does_not_allocate` |

## Test Cases

### Юнит-тесты `src/synth/looper.rs` — `#[cfg(test)] mod clock_sync_tests` (Task 1)

Константа `SR = 48_000.0`. Хелпер (по образцу S0, но локальный):

```rust
fn loop_at(bpm: f32, loop_ticks: u32) -> MidiLooper {
    let mut l = MidiLooper::new(SR);
    l.bpm = bpm;
    l.loop_bpm = bpm;
    l.loop_length_ticks = loop_ticks;
    l
}
```

- **TC1** `sync_updates_bpm_and_beats_per_bar` — `sync_clock(90.0, 3)` → `bpm == 90.0`, `beats_per_bar == 3`.
- **TC2** `regrid_halves_loop_at_double_bpm` — луп 96_000 @ 120 (1 такт 4/4 = 0.5 с × 4 × 48k),
  events restore-путём нельзя (sentinel) → поля напрямую: `len`/`events` приватные, тест в
  том же модуле — положить 2 события через `record_event` в Overdubbing НЕЛЬЗЯ детерминированно,
  поэтому: `l.events[0].tick = 24_000; l.events[1].tick = 48_000; l.len = 2;` (модульные тесты
  видят приватные поля). `playback_tick = 24_000`. `sync_clock(240.0, 4)` → `loop_length_ticks
  == 48_000`, `events[0].tick == 12_000`, `events[1].tick == 24_000`, `playback_tick == 12_000`,
  `loop_bpm == 240.0`.
- **TC3** `regrid_doubles_loop_at_half_bpm` — симметрично: 48_000 @ 120 → 90.0 →
  `loop_length_ticks == 64_000` (48_000 × 120/90), позиция 12_000 → 16_000.
- **TC4** `regrid_preserves_position_fraction_mid_playback` — playback_tick = 25% лупа до и
  после: `playback_tick / loop_length_ticks` в обоих случаях == 1/4 (±1 сэмпл округления).
- **TC5** `events_stay_sorted_after_regrid` — 3 события с близкими тиками (10_000, 10_001,
  10_002) → после `sync_clock(240, 4)` тики неубывающие и ≤ `loop_length_ticks`.
- **TC6** `restore_sentinel_adopts_bpm_without_regrid` — `restore(&[..], 96_000)` (ставит
  `loop_bpm = 0.0`) → `sync_clock(90.0, 4)` → `loop_length_ticks == 96_000` (НЕ изменился),
  тики событий не изменились, `bpm == 90.0`.
- **TC7** `same_bpm_sync_is_noop` — луп 96_000 @ 120, `sync_clock(120.0, 4)` повторно ×10 →
  `loop_length_ticks`, тики, `playback_tick` бит-в-бит прежние.
- **TC8** `bar_length_uses_beats_per_bar` — `MidiLooper::new(SR)`, `bpm = 120`, `beats_per_bar
  = 3`, `bars = 1`, `start_record()` → `loop_length_ticks == 72_000` (0.5 c × 3 × 48k);
  при `beats_per_bar = 4` → 96_000 (регресс-проверка удаления хардкода).
- **TC9** `non_positive_or_nan_bpm_is_ignored` — `sync_clock(0.0, 4)`, `sync_clock(-10.0, 4)`,
  `sync_clock(f32::NAN, 4)` → `bpm` не изменился, луп не тронут, `beats_per_bar` не изменён
  NaN-вызовом (guard стоит ДО присвоения bpb — см. код Task 1: finite-чек первым).
- **TC10** `degenerate_regrid_skips_but_adopts_bpm` — луп 8 тиков @ 120 → `sync_clock(1e7, 4)`:
  `new_len == 0` → тики не тронуты, но `bpm == 1e7` (больше не делит на старый темп).
- **TC11** `quantize_grid_follows_synced_bpm` — `quantize = Sixteenth`, `bpm` 120 →
  `sync_clock(60.0, 4)` → тик 3_000 (при 120 попадал бы в grid 7_500/4≈1875, при 60 grid 3750)
  квантуется в 3_750 (`quantize_tick(3_000)` == 3_750: |3000−0|=3000 vs |3000−3750|=750 → 3750).
  Сверить руками: beat_samples @60bpm/48k = 48_000, grid 1/16 = 12_000... см. точные числа в
  Step 1 теста — числа в утверждении пересчитать по формуле `beat_samples = 60/bpm*sr`.

### Интеграционные `tests/looper_clock_sync.rs` (Task 2)

- **TC12** `tick_block_syncs_looper_clock_from_sequencer` — `DrumSeqBpm { bpm: 90.0 }` → один
  `tick_block` → `engine.looper.bpm == 90.0`.
- **TC13** `beats_per_bar_plumbed_from_current_pattern` — `DrumSeqTimeSig { beats_per_bar: 3 }`
  → `tick_block` → `engine.looper.beats_per_bar == 3`.
- **TC14** `bpm_change_mid_loop_regrids_engine_looper` — `DrumSeqBpm 120` → `LooperRecord` →
  `MidiEvent::NoteOn/Off` на протяжении ~1 блока → `LooperStopRecord` (луп создан через
  `start_record`, `loop_bpm` валиден) → `DrumSeqBpm 240` → `tick_block` →
  `engine.looper.loop_length_ticks` уменьшился вдвое vs до смены.
- **TC15** `record_at_new_bpm_quantizes_to_new_grid` — продолжение TC14: `LooperRecord`
  (overdub), нота на ~середине лупа при `Quantize::Quarter` → записанный тик кратен новой
  четверти (samples_per_beat = 60/240·48_000 = 12_000).

### RT-safety `tests/rt_safety.rs` (Task 2)

- **TC16** `looper_bpm_regrid_does_not_allocate` — луп записан (как TC14), затем ВНУТРИ
  `assert_no_alloc`: `handle_control(DrumSeqBpm { bpm: 90.0 })` + 4 × `tick_block` (первый
  блок делает regrid). Feature-gated `#![cfg(feature = "rt-assert")]` — файл уже такой.

### Конфиг `src/config.rs` inline `#[cfg(test)] mod tests` (Task 4)

- **TC17** `legacy_config_with_looper_sync_bpm_key_parses` — JSON `UiSettings` содержащий
  `"looper_sync_bpm": false` десериализуется (serde по умолчанию игнорит неизвестные поля;
  у `Config`/`UiSettings` нет `deny_unknown_fields` — проверено config.rs:16-27, 100-148).

### GUI (Task 3)

- Тестов нет (headless-инфраструктуры для egui нет; `tests/ui_layout_contract.rs` лупер не
  покрывает — проверено grep). Проверка: `cargo clippy --all-targets -- -D warnings` +
  `cargo build`. Наличие РОВНО одного BPM-поля — код-ревью по diff Task 3.

## Negative matrix

| Случай | Покрытие | Ожидание |
|---|---|---|
| BPM 0 / отрицательный / NaN / inf из кольца | TC9 | guard первым делом: return, ничего не менять |
| regrid даёт `new_len == 0` (экстремальный ratio) | TC10 | скип рескейла, bpm принимается |
| Пустой луп (`loop_length_ticks == 0`) | TC6-вариант, engine по умолчанию | только bpm/loop_bpm, без regrid |
| Restore-провениенс неизвестен (`loop_bpm == 0.0`) | TC6 | первый sync принимает bpm без рескейла |
| Тот же BPM каждый блок (постоянный вызов sync) | TC7 | strict no-op, `update_display` не зовётся |
| `beats_per_bar` 0 / 255 | TC1 + engine clamp (drum_time_sig tc2) | `clamp(1, 16)` в `sync_clock`; 2..=4 уже клампит `DrumSeqTimeSig` (mod.rs:2320) |
| Драг BPM: дубли/шторм пушей каждый кадр | TC5 + design note | last-wins; regrid monotone; дрейф ≤ ±1 сэмпл/событие/regrid, bound ~2 мс после 100 шагов |
| Playback на границе wrap во время regrid | код: `.min(new_len - 1)` | playback_tick не вылезает за новую длину |
| Дубликаты тиков после рескейла (2 события → один тик) | TC5 | легальны: массив остаётся неубывающим, `partition_point` корректен, оба сыграют на одном тике (как `record_event` уже делает) |
| Kit load / MIDI import во время записи лупера | A11/A12 + TC12 | BPM доезжает per-block без GUI-пушей; луп уже записанный — рескейлится как любая смена BPM |
| Старый конфиг с ключом `looper_sync_bpm` | TC17 | парсится, ключ игнорится |
| Reconnect (settings.rs restore) | TC6 + design note | sentinel: без ложного рескейла; старый push `LooperSetBpm` удалён (A17) |
| Удаление `LooperSetBpm` с забытой ссылкой | Gates | `cargo clippy --all-targets -- -D warnings` красный — все 6 сайтов в инвентаре A8/A10-A13/A15/A17/A18 |

---

## Task 1: looper.rs — `sync_clock` + `regrid` + `beats_per_bar`/`loop_bpm` (TDD)

- [ ] **Step 0: прочитать контекст.** `src/synth/looper.rs` целиком (636 строк); якоря:
  struct `MidiLooper` :145-169, `new()` :172-195, `bar_length_samples` :211-213,
  `quantize_tick` :215-229, `start_record` :232-247, `clear` :310-323, `restore` :332-360,
  `undo` (cursor-паттерн) :291-307, `update_display` :552-575, `mod restore_tests` :589-635.

- [ ] **Step 1: тесты ПЕРВЫМИ.** В конец `src/synth/looper.rs` (после `mod restore_tests`)
  добавить `#[cfg(test)] mod clock_sync_tests` с TC1-TC11 из раздела Test Cases. Тесты видят
  приватные поля (`loop_bpm`, `events`, `len`) — модуль тот же. Полные тела:

```rust
#[cfg(test)]
mod clock_sync_tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn loop_at(bpm: f32, loop_ticks: u32) -> MidiLooper {
        let mut l = MidiLooper::new(SR);
        l.bpm = bpm;
        l.loop_bpm = bpm;
        l.loop_length_ticks = loop_ticks;
        l
    }

    #[test]
    fn sync_updates_bpm_and_beats_per_bar() {
        let mut l = loop_at(120.0, 96_000);
        l.sync_clock(90.0, 3);
        assert_eq!(l.bpm, 90.0);
        assert_eq!(l.beats_per_bar, 3);
        assert_eq!(l.loop_bpm, 90.0);
    }

    #[test]
    fn regrid_halves_loop_at_double_bpm() {
        let mut l = loop_at(120.0, 96_000);
        l.events[0].tick = 24_000;
        l.events[1].tick = 48_000;
        l.len = 2;
        l.playback_tick = 24_000;
        l.sync_clock(240.0, 4);
        assert_eq!(l.loop_length_ticks, 48_000);
        assert_eq!(l.events[0].tick, 12_000);
        assert_eq!(l.events[1].tick, 24_000);
        assert_eq!(l.playback_tick, 12_000);
        assert_eq!(l.loop_bpm, 240.0);
    }

    #[test]
    fn regrid_doubles_loop_at_half_bpm() {
        let mut l = loop_at(120.0, 48_000);
        l.playback_tick = 12_000;
        l.sync_clock(90.0, 4);
        assert_eq!(l.loop_length_ticks, 64_000); // 48_000 * 120/90
        assert_eq!(l.playback_tick, 16_000);
    }

    #[test]
    fn regrid_preserves_position_fraction_mid_playback() {
        let mut l = loop_at(120.0, 96_000);
        l.playback_tick = 24_000; // ровно 1/4
        l.sync_clock(60.0, 4);    // x2 длина
        let frac = l.playback_tick as f64 / l.loop_length_ticks as f64;
        assert!((frac - 0.25).abs() < 0.001, "fraction drifted: {frac}");
    }

    #[test]
    fn events_stay_sorted_after_regrid() {
        let mut l = loop_at(120.0, 96_000);
        l.events[0].tick = 10_000;
        l.events[1].tick = 10_001;
        l.events[2].tick = 10_002;
        l.len = 3;
        l.sync_clock(240.0, 4);
        for w in l.events[..3].windows(2) {
            assert!(w[0].tick <= w[1].tick, "sort broken: {} > {}", w[0].tick, w[1].tick);
            assert!(w[1].tick <= l.loop_length_ticks);
        }
    }

    #[test]
    fn restore_sentinel_adopts_bpm_without_regrid() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[LooperEventSnapshot { tick: 24_000, note: 60, velocity: 127, layer_id: 0, part_id: 0 }], 96_000);
        l.sync_clock(90.0, 4);
        assert_eq!(l.loop_length_ticks, 96_000, "sentinel: no regrid after restore");
        assert_eq!(l.events[0].tick, 24_000);
        assert_eq!(l.bpm, 90.0);
    }

    #[test]
    fn same_bpm_sync_is_noop() {
        let mut l = loop_at(120.0, 96_000);
        l.events[0].tick = 24_000;
        l.len = 1;
        l.playback_tick = 48_000;
        let before = (l.loop_length_ticks, l.events[0].tick, l.playback_tick);
        for _ in 0..10 { l.sync_clock(120.0, 4); }
        assert_eq!(before, (l.loop_length_ticks, l.events[0].tick, l.playback_tick));
    }

    #[test]
    fn bar_length_uses_beats_per_bar() {
        let mut l = MidiLooper::new(SR); // bpm 120
        l.beats_per_bar = 3;
        l.bars = 1;
        l.start_record();
        assert_eq!(l.loop_length_ticks, 72_000); // 0.5s * 3 * 48k
        let mut q = MidiLooper::new(SR);
        q.beats_per_bar = 4;
        q.bars = 1;
        q.start_record();
        assert_eq!(q.loop_length_ticks, 96_000);
    }

    #[test]
    fn non_positive_or_nan_bpm_is_ignored() {
        let mut l = loop_at(120.0, 96_000);
        l.sync_clock(0.0, 4);
        l.sync_clock(-10.0, 4);
        l.sync_clock(f32::NAN, 4);
        l.sync_clock(f32::INFINITY, 4);
        assert_eq!(l.bpm, 120.0);
        assert_eq!(l.loop_length_ticks, 96_000);
        assert_eq!(l.beats_per_bar, 4); // не изменён NaN-вызовом
    }

    #[test]
    fn degenerate_regrid_skips_but_adopts_bpm() {
        let mut l = loop_at(120.0, 8);
        l.events[0].tick = 4;
        l.len = 1;
        l.sync_clock(1.0e7, 4); // new_len == 0
        assert_eq!(l.loop_length_ticks, 8);
        assert_eq!(l.events[0].tick, 4);
        assert_eq!(l.bpm, 1.0e7);
    }

    #[test]
    fn quantize_grid_follows_synced_bpm() {
        // beat_samples @60bpm/48k = 48_000; grid 1/16 = 12_000 (было @120: 6_000).
        // tick 13_000: до sync квантовался бы в 12_000 (@120 grid 6_000: |13000-12000|<3000),
        // после sync (grid 12_000) 13_000 → 12_000 тоже; возьмём 17_000:
        // @120 (grid 6_000): 18_000. @60 (grid 12_000): 12_000.
        let mut l = loop_at(120.0, 192_000);
        l.quantize = Quantize::Sixteenth;
        assert_eq!(l.quantize_tick(17_000), 18_000);
        l.sync_clock(60.0, 4); // regrid x2, grid теперь 12_000
        assert_eq!(l.quantize_tick(17_000), 12_000);
    }
}
```

  Запуск: `cargo test --lib looper` — ВСЕ новые тесты КРАСНЫЕ (нет `sync_clock`, нет полей),
  кроме `quantize_grid_follows_synced_bpm` может частично пройти — это норма для TDD-пина;
  фиксируем красный список.

- [ ] **Step 2: реализация.** Три правки в `src/synth/looper.rs`:

  (a) Поля struct (после `pub bpm: f32,` :153):

```rust
    pub bpm: f32,
    /// BPM the current loop was established at; drives the regrid ratio on
    /// tempo changes. `0.0` = provenance unknown (post-restore): the next
    /// sync_clock adopts the new tempo WITHOUT rescaling the restored loop.
    loop_bpm: f32,
    /// Beats per bar, synced per block from the drum sequencer's current
    /// pattern (REFACTOR_AUDIT S5). Affects loop length only at the next
    /// start_record — an existing loop is never re-bared.
    pub beats_per_bar: u8,
```

  И в `new()` (рядом с `bpm: 120.0,` :181): `loop_bpm: 120.0,` + `beats_per_bar: 4,`.

  (b) `bar_length_samples` (:211-213) — заменить тело:

```rust
    fn bar_length_samples(&self) -> u32 {
        ((60.0 / self.bpm) * self.beats_per_bar as f32 * self.sample_rate) as u32
    }
```

  (c) Новые методы — вставить после `quantize_tick` (после :229), до `start_record`:

```rust
    /// Per-block clock sync (REFACTOR_AUDIT S5): the drum sequencer BPM is the
    /// single canonical clock (same precedent as arp/pitch_seq in tick_block).
    /// Called once per block, before the per-sample tick() loop.
    ///
    /// A mid-loop tempo change rescales the recorded loop proportionally
    /// (event ticks, loop length, playback position): musical positions
    /// survive and the loop plays back at the new tempo like a DAW clip.
    /// Events are note on/offs, not audio — no time-stretch needed. Rounding
    /// error is bounded by ±1 sample per event per regrid; a BPM drag regrids
    /// every frame, worst case ~2 ms drift at 48 kHz after ~100 steps.
    /// RT-safe: integer math, no allocation.
    pub fn sync_clock(&mut self, bpm: f32, beats_per_bar: u8) {
        if !bpm.is_finite() || bpm <= 0.0 { return; }
        self.beats_per_bar = beats_per_bar.clamp(1, 16);
        if (self.bpm - bpm).abs() < 0.01 { return; }
        if self.loop_length_ticks > 0 && self.loop_bpm > 0.0 {
            self.regrid(bpm);
        }
        self.bpm = bpm;
        self.loop_bpm = bpm;
    }

    /// Rescale loop content and playback position from self.bpm to `new_bpm`.
    /// Per-event scale `t * new_len / old_len` is monotone non-decreasing in
    /// `t`, so the sorted-by-tick invariant is preserved (duplicates legal —
    /// record_event already inserts those). Cursor re-derivation mirrors
    /// undo() (partition_point below).
    fn regrid(&mut self, new_bpm: f32) {
        let old_len = self.loop_length_ticks as u64;
        let new_len = ((old_len as f64) * (self.bpm as f64 / new_bpm as f64)) as u64;
        if new_len == 0 || new_len > u32::MAX as u64 { return; }
        let new_len = new_len as u32;
        let scale = |t: u32| ((t as u64 * new_len as u64) / old_len) as u32;
        for i in 0..self.len as usize {
            self.events[i].tick = scale(self.events[i].tick);
        }
        let new_pos = scale(self.playback_tick);
        self.loop_length_ticks = new_len;
        self.playback_tick = new_pos.min(new_len - 1);
        self.playback_cursor = self.events[..self.len as usize]
            .partition_point(|e| e.tick < self.playback_tick);
        self.update_display();
    }
```

  (d) `start_record` Idle-ветка (:237 сразу после присвоения `loop_length_ticks`):
  добавить `self.loop_bpm = self.bpm;`.
  (e) `clear()` (:312 рядом с `self.loop_length_ticks = 0;`): добавить `self.loop_bpm = 0.0;`.
  (f) `restore()` (:339 рядом с `self.loop_length_ticks = loop_length_ticks;`):
  добавить `self.loop_bpm = 0.0;` + комментарий
  `// Provenance unknown (ticks were rescaled to the new SR by the GUI): first sync_clock must NOT regrid.`

- [ ] **Step 3: прогнать.** `cargo test --lib looper` — все TC1-TC11 зелёные +
  существующие `restore_tests` зелёные (restore_tests не зависят от loop_bpm).
  Затем `cargo test` (весь suite) — ничего другого не сломано.

- [ ] **Step 4: gates + commit.**
  `cargo clippy --all-targets -- -D warnings` (чисто; поля `loop_bpm` читаются, dead_code нет).
  `git add src/synth/looper.rs && git commit -m "feat(looper): per-block clock sync — sync_clock/regrid, beats_per_bar-aware bar length, restore sentinel (audit S5)"`

## Task 2: tick_block wiring + удалить `LooperSetBpm` + engine/RT тесты

- [ ] **Step 1: wiring в `src/synth/mod.rs`.** В `tick_block` сразу после
  `self.arpeggiator.set_bpm(seq_bpm);` (:2715) добавить:

```rust
        // Looper follows the same canonical clock (REFACTOR_AUDIT S5): bpm +
        // current pattern's beats_per_bar, synced every block like arp/LFO.
        let cur_pat = self.drum_engine.sequencer.current_pattern as usize;
        let beats_per_bar = self.drum_engine.sequencer.patterns[cur_pat].beats_per_bar;
        self.looper.sync_clock(seq_bpm, beats_per_bar);
```

- [ ] **Step 2: удалить событие.** `src/synth/mod.rs`:
  - строку :229 `LooperSetBpm { bpm: f32 },` — удалить;
  - строку :2362 `ControlEvent::LooperSetBpm { bpm } => { self.looper.bpm = bpm; }` — удалить.
  Проверка отсутствия остальных ссылок: `grep -rn "LooperSetBpm" src/` → пусто
  (GUI-сайты ещё живы — они удаляются в Task 3; на этом шаге `cargo check` будет красный
  из-за gui — допустимо, если Task 3 идёт сразу; иначе сначала убрать только
  `src/gui`-пуши минимально. Рекомендуемый порядок: Step 2 и Step 3 в одном коммите
  НЕ делать — коммит только после Task 3; локально держать оба таска в рабочем дереве).
  **Практика:** выполнить Step 1 + Step 2 + Task 3 целиком, gates, один коммит на
  Task 2+3 вместе — см. коммит-план ниже.

- [ ] **Step 3: engine-тесты.** Создать `tests/looper_clock_sync.rs` (новый файл —
  `rustfmt.toml` применим):

```rust
//! Looper/transport clock sync (REFACTOR_AUDIT S5).
//! Plan: docs/superpowers/plans/2026-10-30-refactor-s5-clock-sync.md
//!
//! Drum sequencer BPM is the canonical clock: looper bpm/beats_per_bar are
//! synced per block in tick_block; LooperSetBpm no longer exists.

use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn block(e: &mut SynthEngine) {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    e.tick_block(&mut l, &mut r);
}

#[test]
fn tc12_tick_block_syncs_looper_clock_from_sequencer() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 90.0 });
    block(&mut e);
    assert_eq!(e.looper.bpm, 90.0);
}

#[test]
fn tc13_beats_per_bar_plumbed_from_current_pattern() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 3 });
    block(&mut e);
    assert_eq!(e.looper.beats_per_bar, 3);
}

#[test]
fn tc14_bpm_change_mid_loop_regrids_engine_looper() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 120.0 });
    block(&mut e); // sync: looper.bpm = 120

    let _ = e.handle_control(ControlEvent::LooperRecord); // loop established @120
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    let before = e.looper.loop_length_ticks;
    assert!(before > 0, "record must establish the loop");

    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 240.0 });
    block(&mut e); // first block after the change does the regrid
    assert_eq!(e.looper.loop_length_ticks, before / 2);
}

#[test]
fn tc15_record_at_new_bpm_quantizes_to_new_grid() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 120.0 });
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperRecord);
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    // Regrid to 240, then overdub a note and check the quantized tick.
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 240.0 });
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperSetQuantize { quantize: 1 }); // Quarter
    let _ = e.handle_control(ControlEvent::LooperRecord); // overdub
    // Advance to the middle of the (now halved) loop, then record a note.
    for _ in 0..(BLOCK_SIZE * 30) { block(&mut e); }
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    // samples_per_beat @240bpm = 12_000; quantize Quarter grid == beat == 12_000.
    let beat = e.looper.bpm; // 240.0
    let spb = (60.0 / beat) * SR;
    // Find the recorded note: events are private; assert via looper length and
    // a fresh overdub readout instead — read the display snapshot.
    let display = e.looper.display();
    let d = display.try_lock().unwrap();
    let qnote = d.events.iter().find(|ev| ev.note == 64).expect("overdubbed note recorded");
    let tick = (qnote.position * e.looper.loop_length_ticks as f32).round() as u32;
    let rem = tick % spb as u32;
    assert!(rem < 2 || (spb as u32 - rem) < 2, "tick {tick} not on the 240bpm quarter grid");
}
```

  Примечание к TC15: `LooperDisplayEvent.position` — f32 доля лупа (looper.rs:34), обратное
  преобразование с округлением даёт ±1-2 сэмпла — допуск в assert учитывает это. Если
  flakiness — заменить на unit-уровень (TC11 уже покрывает сетку; TC15 можно упростить до
  проверки `e.looper.bpm == 240.0` + неотрицательного остатка < 4 сэмпла).

- [ ] **Step 4: RT-тест.** В конец `tests/rt_safety.rs` (после :112) добавить:

```rust
#[test]
fn looper_bpm_regrid_does_not_allocate() {
    // Regrid rescales up to MAX_LOOP_EVENTS event ticks and calls
    // update_display inside tick_block — must stay allocation-free.
    let mut engine = SynthEngine::new(SR);
    let _ = engine.handle_control(ControlEvent::DrumSeqBpm { bpm: 120.0 });
    let _ = engine.handle_control(ControlEvent::LooperRecord);
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    engine.tick_block(&mut l, &mut r);
    let _ = engine.handle_control(ControlEvent::LooperStopRecord);

    assert_no_alloc(|| {
        let _ = engine.handle_control(ControlEvent::DrumSeqBpm { bpm: 90.0 });
        for _ in 0..4 { engine.tick_block(&mut l, &mut r); } // first block regrids
    });
}
```

- [ ] **Step 5: прогон.** `cargo test --test looper_clock_sync` (TC12-15 зелёные);
  `cargo test --features rt-assert --test rt_safety` (6 тестов зелёные).

## Task 3: GUI — одно BPM-поле в docked-панели, флаг и looper-BPM убрать

(Выполняется в том же рабочем дереве, что и Step 2 Task 2 — см. коммит-план.)

- [ ] **Step 1: `src/gui/drums.rs` — единое BPM-поле в docked transport row.**
  В `draw_drum_seq_docked`, транспортный `ui.horizontal` (:25-60), после блока Undo/Clear
  (после :59, перед закрывающей `});`) добавить:

```rust
            ui.separator();
            ui.label("BPM:");
            if ui.add(egui::DragValue::new(&mut self.drum_bpm).range(40.0..=300.0).speed(0.5)).changed() {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqBpm { bpm: self.drum_bpm });
            }
```

- [ ] **Step 2: убрать BPM из mix row.** `draw_drum_kit_controls` :277-285 — удалить
  `ui.label("BPM:");` и весь `if ui.add(egui::DragValue::new(&mut self.drum_bpm)...)` блок
  (вместе с внутренним `if self.looper_sync_bpm { ... }`). Остаток строки: Swing/Vol/V127/Metro.
  Обновить doc-комментарий функции (:271-273): «Mix row (Swing/Vol/V127/Metro)» — BPM переехал
  в docked.

- [ ] **Step 3: убрать sync-блоки из kit load и MIDI import.**
  - :381-384 (`if self.looper_sync_bpm { self.looper_bpm = kit.bpm; let _ = ... }`) — удалить
    целиком (присвоение `self.drum_bpm = kit.bpm;` :370 остаётся).
  - :416-419 — удалить целиком (присвоение `self.drum_bpm = bpm;` :404 остаётся).

- [ ] **Step 4: убрать Sync-тоггл + looper BPM из looper row 2.** `draw_looper` :896-907 —
  удалить от `if ui.selectable_label(self.looper_sync_bpm, "Sync")...` до конца
  looper-BPM DragValue-блока включительно. Row 2 остаётся: Bars, Q, separator, Rec-to combo.

- [ ] **Step 5: `src/gui/mod.rs`.**
  - :362-363 — удалить `pub looper_sync_bpm: bool,` и `pub looper_bpm: f32,`;
  - :1512 — удалить `self.config.ui.looper_sync_bpm = self.looper_sync_bpm;`;
  - :1842 (`send_drum_state`) — удалить `let _ = self.ctrl_tx.push(ControlEvent::LooperSetBpm { bpm: self.looper_bpm });`
    (строки :1843-1844 Bars/Quantize остаются; BPM доезжает per-block из sequencer'а,
    который :1834 уже получил через DrumLoadKit).

- [ ] **Step 6: `src/lib.rs`** — :813-814 удалить обе строки (`looper_sync_bpm: ...`,
  `looper_bpm: 120.0,`).

- [ ] **Step 7: `src/config.rs`.**
  - :142-144 — удалить doc-комментарий + поле `pub looper_sync_bpm: bool,`;
  - :150 — удалить `fn default_looper_sync_bpm() -> bool { true }`;
  - :209 — удалить `looper_sync_bpm: true,` из Default literal.
  Добавить в конец файла:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_with_looper_sync_bpm_key_parses() {
        // TC17: pre-S5 config files carry the removed key; serde must ignore it.
        let json = r#"{
            "audio": { "backend": "JACK", "sample_rate": 48000, "buffer_size": 256 },
            "midi": { "port_name": null },
            "ui": { "window_width": 1280.0, "window_height": 800.0, "last_preset": null, "looper_sync_bpm": false }
        }"#;
        let cfg: Config = serde_json::from_str(json).expect("legacy config must parse");
        assert_eq!(cfg.audio.sample_rate, 48000);
    }
}
```

  Если `UiSettings` требует дополнительных обязательных полей — сверить :100-148 (все с
  `#[serde(default)]`/default-fn, кроме audio/midi-структур — они в JSON выше заданы).
  При расхождении — упростить тест до парсинга `UiSettings` из `{"looper_sync_bpm": false}`
  и assert дефолтов.

- [ ] **Step 8: прогон + коммит (вместе с Task 2).**
  `grep -rn "LooperSetBpm\|looper_sync_bpm\|looper_bpm" src/` → пусто
  (`looper_bars`/`looper_quantize` НЕ тронуты — не match'ить по префиксу).
  `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
  `git add -A src/ && git commit -m "refactor: single canonical BPM clock — looper synced per-block from drum sequencer, LooperSetBpm + looper_sync_bpm flag removed, single BPM field in docked panel (audit S5, TECHDEBT #32 phase 1)"`

## Task 4: docs + финальные gates

- [ ] **Step 1: `docs/TECHDEBT.md` #32** (секция «## 32.», :579-607): дописать в конец
  фазы 1 заметку:

```
   Phase 1 (clock): ВЫПОЛНЕНО (S5, 2026-10-30) — один канонический BPM
   (drum sequencer), лупер синхронизируется per-block (bpm + beats_per_bar),
   смена темпа mid-loop рескейлит луп (regrid), LooperSetBpm /
   looper_sync_bpm / второе BPM-поле удалены. MIDI-clock (0xF8/0xFA) не
   реализован вовсе (0 вхождений) — осознанно вне S5: внешний clock-slave
   ввёл бы второй источник темпа; решать внутри #32 при дизайне
   мастер-транспорта.
```

- [ ] **Step 2: `REFACTOR_AUDIT.md`** — в заголовке S5-пункта (:250) пометить:
  `**S5. Предпосылки #32 (looper/transport). — ВЫПОЛНЕНО 2026-10-30 (plan docs/superpowers/plans/2026-10-30-refactor-s5-clock-sync.md).**`

- [ ] **Step 3: полные gates (CLAUDE.md):**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```

  Все зелёные. `cargo fmt` по всей базе НЕ запускать (CLAUDE.md); новые файлы
  (`tests/looper_clock_sync.rs`) уже в rustfmt-стиле.

- [ ] **Step 4:** `git add docs/TECHDEBT.md REFACTOR_AUDIT.md && git commit -m "docs: S5 done — canonical clock, regrid on tempo change, MIDI-clock explicitly deferred to #32"`

## RT-safety резюме

- `sync_clock`/`regrid` вызываются из `tick_block` (audio thread): только целочисленная
  арифметика + f64 mul/div (2 деления на фактическую смену BPM, 0 в steady state) +
  `update_display()` (existing `try_lock` + pre-sized Vec — известный принятый долг).
- Покрыто TC16 (`assert_no_alloc` вокруг regrid-блока).
- `LooperSetBpm`-пуши удалены — минус 6 push-сайтов, кольцо `rtrb` не расширялось.
- GUI-правки — вне RT-зоны.

## Рискованные места (для ревьюера)

1. **Regrid во время Recording (первый проход)** — playback_tick рескейлится вместе с
   длиной; запись продолжается корректно (позиция и длина согласованы), но не покрыто
   отдельным тестом (TC2-TC5 — Playing/Idle; семантика идентична, state не читается в regrid).
2. **Дрейф при драге BPM** — ±1 сэмпл/событие/regrid, bound в doc-comment; если когда-нибудь
   станет проблемой — переход на биты (отвергнутая альтернатива 1) как отдельная работа.
3. **`bars` GUI-поле** — осталось как было (`LooperSetBars`); после смены bpb длина нового
   лупа = `bars * bpb * beat`, что и требуется по #21/#32.
4. **TC15 хрупкость** — обратное преобразование position→tick через f32; при flakiness
   упростить per примечания в Step 3 Task 2.
