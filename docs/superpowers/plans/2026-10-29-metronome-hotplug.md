# Metronome-dies-on-Rec (#22a) + Idle Guard (#19 quick win) + Audio Hotplug (#18) — Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ ecf67e7 чтением кода в этой сессии; выводы по #22a и #19 подтверждены ЗАПУСКОМ тестов в этой сессии)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (A) TECHDEBT #22a — выяснить фактический механизм «метроном затихает при
старте записи с клавиатуры», закрыть регрессионными тестами (или починить, если
тест красный). (B) TECHDEBT #19 quick win — guard на простой метронома.
(C) TECHDEBT #18 — автореконнект при hotplug аудиоустройств (наушники ↔ колонки):
poller смены default-device + retry-машина неудавшегося реконнекта.

**ГЛАВНЫЕ НАХОДКИ РАССЛЕДОВАНИЯ (сессия 2026-10-29, master @ ecf67e7):**

1. **#22a: движок НЕ виноват — бага в движке НЕТ.** Прочитан весь клик-путь и
   Rec-путь; гипотезы (a)/(b)/(c) из techdebt #22 проверены по коду и
   эмпирически (scratch-тест с SynthEngine, запущен и зелёный):
   - (a) клик масштабируется `volume²` (drum.rs:1228-1229), но **ничто** в
     Rec-путях не меняет volume: `DrumSeqRecord` (mod.rs:2308-2316) и
     клавиатурный note-83 (mod.rs:1762-1769) трогают только `recording`/`playing`;
     `DrumLoadKit` (mod.rs:2323-2332) метроном не сбрасывает.
   - (b) auto-play (`set_playing(true)`, drum.rs:994-997) НЕ сбрасывает
     `phase_acc`/`current_step` (reset делает только `DrumSeqPlay { playing: true }`,
     mod.rs:2295-2297). Разрыва клик-тайминга нет.
   - (c) `set_recording` → `save_snapshot` (drum.rs:986-992, 784-787) не трогает
     `click_*`-поля; `click_env` зануляется только в `set_sample_rate` (SR-change).
   - Триггер клика (drum.rs:1176-1189) зависит только от `metronome && playing &&
     step сменился` — `recording` в условии отсутствует.
   - Эмпирика (запуск в этой сессии): rec во время игры → клики продолжаются с
     тем же интервалом 750 блоков (peak 0.349 до, 0.216 после — это просто
     акцент step-0 vs обычный бит); rec из стопа → auto-play → клики НАЧИНАЮТСЯ.
   - Вероятные реальные причины репорта: (1) **соседние пэды** 81 (stop) / 82
     (play/pause) рядом с 83 (rec): нажатие 82/81 останавливает транспорт →
     клик молчит **by design** (метроном определён только при playing,
     tests/metronome.rs tc3); (2) **маскировка по громкости**: клик 0.55/0.9 ×
     drumVol² против полноразмерных записываемых хитов — на слух «тонет».
   → Группа A = только регрессионные тесты + документирование; **движок не
   трогаем** (no-op), чеклист ручной верификации прилагается.

2. **#19 quick win: guard УЖЕ есть в коде.** `DrumEngine::tick`
   (drum.rs:1218-1226): весь блок `sin()/floor()` обёрнут в
   `if self.click_env > 0.0 { ... }`, а `click_env` зануляется при
   `< 0.001` (drum.rs:1225) и равен 0.0 при выключенном метрономе/остановленном
   транспорте (триггер единственный источник ненулевого env). В покое cost = один
   f32-компарер на сэмпл. Оптимизировать нечего → Группа B = docs-only
   (пометить (в) в techdebt #19 проверенным). Полный #19 (flamegraph, denormals,
   bisect) — ОТДЕЛЬНАЯ будущая работа, остаётся в body #19.

3. **#18: дизайн из заявки подтверждён кодом.** `AudioBackend` (audio.rs:87-97)
   device name НЕ хранит (строятся на `device` из audio.rs:122-124);
   `reconnect_audio` (gui/settings.rs:186-349) — GUI-поточная операция
   (перестраивает stream/engine на GUI-потоке, callback видит только готовый
   новый stream; аллокации в callback не попадают — rt-зона в audio.rs:188 не
   задета); looper restore переживает (settings.rs:309-332, PR #2);
   `console_rx`/LED-socket reconnect_audio НЕ рвёт (не пересоздаются — п. (б)
   techdebt #18 снят чтением). Разрыв стрима ловится (audio.rs:291-294 →
   disconnected-атом), но retry нет и default-device change не обнаруживается
   (gui/mod.rs:476-481 — одна попытка по флагу).

**Architecture:**
- Группа C добавляет в `src/audio.rs` (GUI-сторона, pub, unit-тестирование из
  tests/): `default_output_device_name(host_id)` — имя текущего системного
  default-устройства; `ReconnectPolicy` — маленькая state-машина
  `Connected | Retrying { at, attempts }` с 2 c backoff и тостом только на
  первой неудаче серии. `AudioBackend` получает `pub device_name: Option<String>`.
  GUI (gui/mod.rs update): desktop-поллер раз в 1.5 c сравнивает сохранённое имя
  с текущим default; изменение или dead-stream флаг или due-retry →
  `reconnect_audio()`; неудача → policy.on_failure. Android: поллер off
  (AAudio-роутинг, default-switch нет); dead-stream флаг остаётся.
- Никаких изменений в аудио-callback → RT-безопасность не затронута (rt_safety
  гейт всё равно гоняется).

**Tech Stack:** Rust, cpal (уже есть), std Instant — без новых зависимостей.

**Spec context:** docs/TECHDEBT.md #22 (репорт 2026-10-29), #19 (в),
#18 (сценарий наушники ↔ колонки).

**Проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

Полные гейты перед финальным коммитом группы: добавить `cargo deny check`.
`cargo fmt` по всей базе НЕ запускать (компактный стиль проекта).

---

## Test Cases (from the requirement)

Требование #22a («включил секвенсор с клавы — метроном затихает»):
1. Метроном on, транспорт играет (120 BPM) → включить Rec (GUI-событие
   `DrumSeqRecord{true}`) посреди потока → клики продолжаются, интервал между
   соседними кликами по обе стороны границы Rec ≈ 750 блоков (±4).
2. То же через клавиатурный путь: `MidiEvent::NoteOn{ch9, note 83, vel 127}` →
   клики продолжаются.
3. Rec из остановленного транспорта → auto-play → клики появляются и идут через
   ≈750 блоков.
4. Двойное нажатие note-83 (rec on, затем off) → клики продолжаются без разрыва.
5. `NoteOn{ch9, 83, velocity 0}` (MIDI-кодировка note-off) при остановленном
   транспорте → rec НЕ тоглится, авто-play НЕ происходит, тишина.
6. Метроном off / транспорт стоит → тишина (уже покрыто tc3/tc4, не дублируется).

Требование #18 («отключил наушники → включил обратно — звука нет без рестарта»):
7. Чистая функция сравнения: сохранённое имя == текущему → false; отличается →
   true; текущего устройства нет (`None`) → false (транзиент, не триггер).
8. Первая неудача реконнекта → `Retrying { at = now+2s, attempts = 1 }` +
   тост (return true); повторные неудачи → тоста нет (return false), attempts
   растут, at сдвигается на +2s от каждой попытки.
9. `retry_due`: до `at` → false, в `at` и позже → true; в `Connected` → false.
10. Успех → `Connected`, bookkeeping сброшен; новая серия неудач снова тостит
    один раз.

Полное аудио-переключение (реальный hotplug) — ручная верификация, чеклист в
Task C4. Полл/state-машина — автотесты (Tasks C1-C3); device_name сравнение —
чистая функция.

## Negative Matrix

#22a (тесты tc1-tc5):
- rec toggle при выключенном метрономе → тишина (tc3-семантика, orthogonal).
- velocity-0 note-83 — не тогглит (guard `velocity > 0`, mod.rs:1760) — tc5.
- двойной toggle — rec завершается OFF, клики без разрыва — tc4.
- Rec из стопа: авто-play БЕЗ reset step → клик стартует с текущей позиции
  паттерна (не с 0) — допустимо, не ассертится (позиция зависит от history).
- Драм-хиты при записи громче клика (маскировка) — воспринимаемое, не баг;
  задокументировано в findings, отдельной работы нет.
- Пэды 81/82/83 — транспорт-ноты ch9: случайное 82/81 глушит клик by design —
  чеклист ручной верификации, движок не меняется.

#18 (тесты tc7-tc10):
- `None` устройство → false: исчезнувший default во время живого стрима не
  дёргает реконнект (транзиент поллера); отсутствие устройства после разрыва
  стрима покрывается retry-путём.
- Пустое имя stored (`""`) vs `Some("")` → false; vs другое → true (tc6-группа).
- Clock skew — неприменимо: `Instant` монотонный, тесты подают явные `Instant`.
- Concurrent/duplicate вызовы: GUI однопоточный, `update()` синхронный;
  state-машина сериализует попытки (`retry_due` + `attempts`).
- Unbounded retry: устройство так и не вернулось → попытки каждые 2 c
  бесконечно, молча (один тост). Принято: одна попытка ≈ cpal enum +фейл,
  cost ничтожен; выход — рестарт. Документировано, не лечится счётчиком
  (счётчик attempts ведётся, но не капится).
- ALSA-хост: `default_output_device().name()` константен («default») — поллер
  слеп к смене железа; там стрим обычно и не рвётся (dmix/PipeWire-plugin).
  Задокументированное ограничение; PipeWire/JACK имена трекают устройство.
- `device.name()` Err → `audio_device_name: None` → поллер для сессии
  отключён, dead-stream флаг работает как раньше (degradation, не блокер).
- Android: поллер off (cfg); dead-stream путь общий.
- reconnect_audio блокирует GUI ~150-300 ms (sleep в AudioBackend::new + ребилд
  движка) — существующее поведение, при retry повторяется раз в 2 c. Принято.

#19: покой (метроном off / не играет / хвост затух) → один компарер на сэмпл
(drum.rs:1220); хвост клика дотухает сам (env < 0.001 → 0) — уже покрыто
tests/metronome.rs tc4 и rt-тестом `metronome_click_does_not_allocate`.

---

# Группа A: TECHDEBT #22a — метроном при записи (регрессионные тесты + docs)

**Формат TDD здесь особый:** тесты написаны из текста требования, ожидаемый
результат — **ЗЕЛЁНЫЙ сразу** (находка №1: баги в движке нет). Красный тест в
этой группе = механизм найден → действовать по дереву диагностики в Step 3.

### Task A1: Регрессионные тесты «клик переживает Rec» в tests/metronome.rs

**Files:**
- Modify: `tests/metronome.rs`

- [ ] **Step 1:** в `tests/metronome.rs` заменить строку импорта

```rust
use mini_midi_synth::synth::{ControlEvent, SynthEngine, BLOCK_SIZE};
```

на

```rust
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};
```

и добавить в конец файла:

```rust
/// Stitch two click_events runs into one absolute timeline: `tail` block
/// indices are shifted by `pre_blocks`.
fn stitched(mut pre: Vec<(usize, f32)>, mut tail: Vec<(usize, f32)>, pre_blocks: usize)
    -> Vec<(usize, f32)>
{
    for t in tail.iter_mut() { t.0 += pre_blocks; }
    pre.append(&mut tail);
    pre
}

fn assert_beat_intervals(events: &[(usize, f32)], min_events: usize) {
    assert!(events.len() >= min_events, "expected >= {min_events} beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750: {events:?}");
    }
}

/// Requirement: metronome click must keep sounding at the same beat interval
/// when sequencer recording starts mid-playback (TECHDEBT #22a).
#[test]
fn tc6_rec_mid_playback_click_continues() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let pre = click_events(&mut e, 500);
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    let tail = click_events(&mut e, 1000);
    assert!(!pre.is_empty(), "no clicks before rec");
    assert!(!tail.is_empty(), "clicks vanished after rec start");
    assert_beat_intervals(&stitched(pre, tail, 500), 3);
}

/// Same via the keyboard transport path (pad note 83 on ch 9).
#[test]
fn tc7_keyboard_rec_note83_click_continues() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let pre = click_events(&mut e, 500);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 83 });
    let tail = click_events(&mut e, 1000);
    assert!(!pre.is_empty(), "no clicks before kb rec toggle");
    assert!(!tail.is_empty(), "clicks vanished after kb rec toggle");
    assert_beat_intervals(&stitched(pre, tail, 500), 3);
}

/// Rec from a stopped transport auto-plays (commit 0983413) — the click must
/// START (it was silent while stopped by design).
#[test]
fn tc8_rec_from_stopped_autoplay_clicks_start() {
    let mut e = metronome_engine(120.0, true);
    assert!(click_events(&mut e, 400).is_empty(), "click while stopped");
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    let events = click_events(&mut e, 1600);
    assert_beat_intervals(&events, 2);
}

/// Double press of note 83: rec ends OFF, click timeline must not break.
#[test]
fn tc9_double_toggle_click_continues() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let pre = click_events(&mut e, 500);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 }); // rec on
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 }); // rec off
    let tail = click_events(&mut e, 1000);
    assert!(!tail.is_empty(), "clicks vanished after double toggle");
    assert_beat_intervals(&stitched(pre, tail, 500), 3);
}

/// NoteOn with velocity 0 is the MIDI note-off encoding: it must NOT toggle
/// rec (no auto-play, hence silence while the transport is stopped).
#[test]
fn tc10_note83_velocity0_does_not_toggle_rec() {
    let mut e = metronome_engine(120.0, true);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 0 });
    assert!(click_events(&mut e, 800).is_empty(),
        "velocity-0 toggled rec / started transport");
}
```

- [ ] **Step 2: запуск**

```bash
cargo test --test metronome
# ОЖИДАНИЕ: 10 passed (tc1-tc5 уже зелёные, tc6-tc10 зелёные = бага нет).
```

- [ ] **Step 3: если какой-то из tc6-tc10 КРАСНЫЙ** — механизм найден, план
  меняется на фикс. Дерево диагностики:
  - красный tc6/tc7 (клики пропали после rec): дампать `click_env` в момент
    rec через отладочный тест — смотреть, кто его зануляет (пути из
    `handle_control` arm 2308 / note-83 arm 1762);
  - красный tc8 (авто-play без кликов): проверить, что `set_playing` (drum.rs:994)
    не появился reset, и что `DrumSeqRecord` arm не сбрасывает step;
  - красный tc10: guard `velocity > 0` (mod.rs:1760) потерян — вернуть;
  - красный по интервалам (а не наличию): сброс `phase_acc` где-то в Rec-пути —
    искать `sequencer.reset()`/`current_step =` в ветках записи.
  Фикс минимальный в найденном месте + тест остаётся как регрессия.

- [ ] **Step 4: гейты + коммит**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
git add tests/metronome.rs
git commit -m "test: metronome click survives sequencer rec start (TECHDEBT #22a)"
```

### Task A2: Документирование — techdebt #22a расследован, движок чист

**Files:**
- Modify: `docs/TECHDEBT.md`

- [ ] **Step 1:** в пункте 22 заменить блок гипотез «(a) метроном: при Rec
  авто-старте проверь set_playing/set_recording → не сбрасывает ли
  click_env/state; или drum-хиты маскируют клик по громкости (клик
  масштабируется drum_volume) — сравнить громкость до/во время записи.» на:

```markdown
- (a) метроном: РАССЛЕДОВАНО 2026-10-29 (план
  docs/superpowers/plans/2026-10-29-metronome-hotplug.md) — движок чист:
  клик продолжает звучать при старте записи во всех путях (GUI DrumSeqRecord,
  клавиатурный note-83, авто-play из стопа, двойной toggle), rec-путь не трогает
  click_*/volume/phase (set_playing без reset, save_snapshot без click-поля).
  Покрытие: tests/metronome.rs tc6-tc10. Вероятные причины репорта: (1) соседние
  пэды 82 (play/pause) / 81 (stop) — без playing клик молчит by design;
  (2) маскировка: клик 0.55/0.9 × drumVol² против полноразмерных записанных
  хитов (отдельный «Metro volume» — возможный будущий UX-Item, не баг).
  П. (b) (#22) остаётся design-Item, не трогается.
```

- [ ] **Step 2: чеклист ручной верификации (выполнить, отметить в PR):**
  1. `cargo run` → Drums → Metro on → Play → нажать rec-пэд (83) на SMK-37:
     клик слышен непрерывно, интервал тот же, запись пишется.
  2. Транспорт стоит → 83: транспорт стартовал, клик звучит.
  3. Нажать 82 (play/pause) во время игры: клик замолчал (by design) — это и
     есть самый вероятный «затих» из репорта; 82 ещё раз — вернулся.
  4. Записывать активные хиты: клик тише хитов (маскировка) — зафиксировать как
     ожидаемое, Metro volume — будущий Item.
- [ ] **Step 3: коммит**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: techdebt #22a investigated — engine clean, regression tests added"
```

---

# Группа B: TECHDEBT #19 quick win — метроном idle guard (docs-only)

### Task B1: Пометить (в) проверенным — guard уже в коде

**Files:**
- Modify: `docs/TECHDEBT.md`

Проверка факта (уже сделана в этой сессии, исполнитель перепроверяет чтением):
`src/synth/drum.rs` @ ecf67e7, строки 1218-1226:

```rust
        // Metronome click: per-sample decaying sine (both DSP and SF2 modes).
        let mut click = 0.0_f32;
        if self.click_env > 0.0 {
            click = self.click_phase.sin() * self.click_env;
            self.click_phase += self.click_phase_inc;
            self.click_phase -= self.click_phase.floor();
            self.click_env *= self.click_decay;
            if self.click_env < 0.001 { self.click_env = 0.0; }
        }
```

sin/floor считаются ТОЛЬКО при `click_env > 0.0`; env ненулевой только ≤30 ms
после бита; зануляется при < 0.001. Покой = один f32-компарер на сэмпл.
Дополнительного guard не требуется; тишина в покое покрыта tc3, RT-чистота —
`metronome_click_does_not_allocate` (tests/rt_safety.rs).

- [ ] **Step 1:** в пункте 19 заменить подпункт «(в) метроном: проверка — не
  считается ли sin/floor когда click_env == 0 (guard),» на:

```markdown
(в) метроном: ПРОВЕРЕНО 2026-10-29 — guard уже есть: sin/floor считаются только
при click_env > 0.0, env зануляется при < 0.001 (drum.rs:1218-1226 @ ecf67e7);
в покое cost = один f32-компарер на сэмпл. Оптимизация не требуется; полный
аудит (#19: flamegraph/denormals/bisect) остаётся отдельной работой,
```

(далее по тексту пункта 19 — подпункты (a),(б),(г) без изменений).

- [ ] **Step 2: коммит**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: techdebt #19 metronome guard already present, no idle cost"
```

---

# Группа C: TECHDEBT #18 — audio hotplug reconnect (poller + retry)

### Task C1: audio.rs — device_name, default-device helper, ReconnectPolicy

**Files:**
- Modify: `src/audio.rs`
- Create: `tests/audio_hotplug.rs`

- [ ] **Step 1: failing-тест** — создать `tests/audio_hotplug.rs`:

```rust
//! Audio hotplug reconnect policy (TECHDEBT #18).
//! Plan: docs/superpowers/plans/2026-10-29-metronome-hotplug.md

use std::time::{Duration, Instant};

use mini_midi_synth::audio::{ReconnectPolicy, ReconnectState, RECONNECT_BACKOFF};

#[test]
fn tc1_device_unchanged_is_not_a_change() {
    assert!(!ReconnectPolicy::device_changed("Pipelewire: headphones", Some("Pipelewire: headphones")));
}

#[test]
fn tc2_device_absent_is_transient_not_a_change() {
    // Default device momentarily gone (or name() failed): the live stream
    // keeps playing — must NOT trigger a reconnect by itself.
    assert!(!ReconnectPolicy::device_changed("speakers", None));
}

#[test]
fn tc3_device_changed_triggers() {
    assert!(ReconnectPolicy::device_changed("headphones", Some("speakers")));
}

#[test]
fn tc4_empty_name_edge_cases() {
    assert!(!ReconnectPolicy::device_changed("", Some("")));
    assert!(ReconnectPolicy::device_changed("", Some("x")));
    assert!(!ReconnectPolicy::device_changed("", None));
}

#[test]
fn tc5_first_failure_toasts_then_retries_are_silent() {
    let mut p = ReconnectPolicy::default();
    let t0 = Instant::now();
    // First failure: toast once, arm a +2 s retry.
    assert!(p.on_failure(t0), "first failure must request a toast");
    assert_eq!(p.state, ReconnectState::Retrying { at: t0 + RECONNECT_BACKOFF, attempts: 1 });
    // Duplicate/consecutive failures: silent, attempt counter grows, deadline
    // slides forward from each attempt.
    assert!(!p.on_failure(t0 + Duration::from_millis(500)), "retry failure must not toast");
    assert_eq!(p.state, ReconnectState::Retrying {
        at: t0 + Duration::from_millis(500) + RECONNECT_BACKOFF, attempts: 2,
    });
    assert!(!p.on_failure(t0 + Duration::from_secs(10)));
}

#[test]
fn tc6_retry_due_respects_backoff_deadline() {
    let mut p = ReconnectPolicy::default();
    let t0 = Instant::now();
    assert!(!p.retry_due(t0), "Connected must not retry");
    p.on_failure(t0);
    assert!(!p.retry_due(t0 + RECONNECT_BACKOFF - Duration::from_millis(1)));
    assert!(p.retry_due(t0 + RECONNECT_BACKOFF));
    assert!(p.retry_due(t0 + RECONNECT_BACKOFF + Duration::from_secs(60)));
}

#[test]
fn tc7_success_resets_and_new_failure_streak_toasts_again() {
    let mut p = ReconnectPolicy::default();
    let t0 = Instant::now();
    p.on_failure(t0);
    p.on_failure(t0 + Duration::from_millis(100));
    p.on_success();
    assert_eq!(p.state, ReconnectState::Connected);
    assert!(!p.retry_due(t0 + Duration::from_secs(5)), "retry bookkeeping cleared");
    // A later, unrelated failure streak starts toasting from scratch.
    assert!(p.on_failure(t0 + Duration::from_secs(30)), "new streak must toast once");
}

#[test]
fn tc8_many_failures_keep_counting_no_cap() {
    // Unbounded silent retries while the device is absent (documented tradeoff).
    let mut p = ReconnectPolicy::default();
    let mut now = Instant::now();
    let mut toasts = 0;
    for _ in 0..50 {
        if p.on_failure(now) { toasts += 1; }
        now += RECONNECT_BACKOFF;
    }
    assert_eq!(toasts, 1);
    assert_eq!(p.state, ReconnectState::Retrying { at: now, attempts: 50 });
}
```

- [ ] **Step 2: запуск (ожидание: не компилируется)**

```bash
cargo test --test audio_hotplug
# error[E0432]: unresolved imports `mini_midi_synth::audio::{ReconnectPolicy, ReconnectState, RECONNECT_BACKOFF}`
```

- [ ] **Step 3: реализация** — `src/audio.rs`.

3a. Поле в `AudioBackend` — в структуру (строки 87-97) после
`pub paused: Arc<AtomicBool>,` (строка 96, перед закрывающей `}` строки 97):

```rust
    /// Name of the output device this stream was built on (TECHDEBT #18
    /// hotplug poll compares it against the current system default).
    /// `None` = the backend failed to report a name; the name poll is
    /// disabled for the session (dead-stream flag still works).
    pub device_name: Option<String>,
```

3b. В `AudioBackend::new` сразу после получения устройства (после строк
122-124 `let device = host.default_output_device().context(...)?;`):

```rust
        let device_name = device.name().ok();
```

и в возврате (строка 308) заменить

```rust
        Ok((Self { stream, disconnected, paused }, final_sr))
```

на

```rust
        Ok((Self { stream, disconnected, paused, device_name }, final_sr))
```

3c. Хелпер и state-машина — добавить в конец файла (после `negotiate_config`,
перед закрывающим `}` не вставлять — после всего impl-блока, на верхнем уровне):

```rust
/// Name of the current system default output device. GUI-thread only
/// (enumerates devices, may allocate). `None` = no default device right now,
/// or the backend failed to report the name.
pub fn default_output_device_name(host_id: cpal::HostId) -> Option<String> {
    let host = cpal::host_from_id(host_id).ok()?;
    let device = host.default_output_device()?;
    device.name().ok()
}

/// Backoff between reconnect attempts (TECHDEBT #18: 2 s).
pub const RECONNECT_BACKOFF: std::time::Duration = std::time::Duration::from_secs(2);

/// GUI-side hotplug reconnect state machine (TECHDEBT #18).
/// `Connected` = stream alive, watching the default-device name.
/// `Retrying` = last reconnect attempt failed; retry no earlier than `at`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectState {
    Connected,
    Retrying { at: std::time::Instant, attempts: u32 },
}

#[derive(Debug, Clone, Copy)]
pub struct ReconnectPolicy {
    pub state: ReconnectState,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self { state: ReconnectState::Connected }
    }
}

impl ReconnectPolicy {
    /// Pure decision: does the system default device no longer match the one
    /// the stream was built on? `None` (device temporarily absent) is NOT a
    /// change — absence after a dead stream is handled by the retry path.
    pub fn device_changed(stored: &str, current: Option<&str>) -> bool {
        match current {
            Some(name) => name != stored,
            None => false,
        }
    }

    /// Record a failed reconnect attempt: arms a +2 s retry, counts attempts.
    /// Returns true exactly once per failure streak (the FIRST failure) —
    /// the GUI shows a toast only then; retries stay silent.
    pub fn on_failure(&mut self, now: std::time::Instant) -> bool {
        let attempts = match self.state {
            ReconnectState::Retrying { attempts, .. } => attempts,
            ReconnectState::Connected => 0,
        } + 1;
        self.state = ReconnectState::Retrying { at: now + RECONNECT_BACKOFF, attempts };
        attempts == 1
    }

    /// Reconnect succeeded: back to watching, retry bookkeeping cleared.
    pub fn on_success(&mut self) {
        self.state = ReconnectState::Connected;
    }

    /// Is a retry attempt due?
    pub fn retry_due(&self, now: std::time::Instant) -> bool {
        matches!(self.state, ReconnectState::Retrying { at, .. } if now >= at)
    }
}
```

- [ ] **Step 4: зелёный + гейты**

```bash
cargo test --test audio_hotplug   # 8 passed
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety   # callback не тронут — зелёный
```

- [ ] **Step 5: коммит**

```bash
git add src/audio.rs tests/audio_hotplug.rs
git commit -m "feat: audio device name capture + hotplug reconnect state machine (#18)"
```

### Task C2: GUI — poller, retry-цикл, toast-throttling

**Files:**
- Modify: `src/gui/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: App-поля** — `src/gui/mod.rs`, в struct `App` после
  `pub audio_disconnected: std::sync::atomic::AtomicBool,` (строка 422,
  перед `pub _audio_handle: ...` строка 423):

```rust
    /// Device name the current stream was built on (TECHDEBT #18). `None` =
    /// unknown (device.name() failed) — name poll disabled, dead-stream flag
    /// still works.
    pub audio_device_name: Option<String>,
    /// Hotplug reconnect retry state machine (TECHDEBT #18).
    pub audio_watch: crate::audio::ReconnectPolicy,
    /// Next default-device name poll deadline (desktop hotplug poller).
    pub audio_poll_at: Instant,
```

- [ ] **Step 2: константа** — `src/gui/mod.rs`, после
  `const PLAY_MODE_NAMES: ...` (строка 35):

```rust
/// Hotplug poll cadence for the default-output-device name (TECHDEBT #18).
const AUDIO_POLL_INTERVAL: Duration = Duration::from_millis(1500);
```

- [ ] **Step 3: инициализация** — `src/lib.rs`, в литерале `App { ... }`
  (run_gui) после `audio_disconnected: c.audio_disconnected.clone(),`
  (строка 869):

```rust
        audio_device_name: c._audio_handle.device_name.clone(),
        audio_watch: crate::audio::ReconnectPolicy::default(),
        audio_poll_at: std::time::Instant::now(),
```

(Порядок важен: `c._audio_handle` перемещается строкой ниже — клон имени
обязан идти раньше; в литерале поля вычисляются в порядке записи.)

- [ ] **Step 4: поллер + retry в update()** — `src/gui/mod.rs`, заменить блок

```rust
        // Check for audio device disconnect (headphones unplugged etc.)
        if self.audio_disconnected.swap(false, std::sync::atomic::Ordering::Relaxed) {
            match self.reconnect_audio() {
                Ok(()) => self.show_toast("Audio reconnected".to_string(), ToastKind::Success),
                Err(e) => self.show_toast(format!("Audio reconnect failed: {e}"), ToastKind::Error),
            }
        }
```

(строки 475-481) на:

```rust
        // Audio hotplug (TECHDEBT #18): (1) dead-stream flag from the cpal
        // error callback; (2) desktop poller — the system default output
        // device name changed (a device swap does not error the old stream:
        // cpal keeps "playing" into the dead device); (3) a failed reconnect
        // retries every RECONNECT_BACKOFF until the device returns. The first
        // failure of a streak toasts; retries are silent. reconnect_audio is
        // a GUI-thread operation (rebuilds the stream); the audio callback
        // only ever sees the finished new stream.
        let now = Instant::now();
        #[cfg(not(target_os = "android"))]
        if now >= self.audio_poll_at {
            self.audio_poll_at = now + AUDIO_POLL_INTERVAL;
            if let Some(stored) = self.audio_device_name.as_deref() {
                let current = crate::audio::default_output_device_name(cpal::default_host().id());
                if crate::audio::ReconnectPolicy::device_changed(stored, current.as_deref()) {
                    log::info!("[audio] default device changed: {stored:?} -> {current:?}");
                    self.audio_disconnected.store(true, Ordering::Relaxed);
                }
            }
        }
        if self.audio_disconnected.swap(false, Ordering::Relaxed) || self.audio_watch.retry_due(now) {
            match self.reconnect_audio() {
                Ok(()) => {
                    self.audio_watch.on_success();
                    self.audio_device_name =
                        self._audio_handle.as_ref().and_then(|b| b.device_name.clone());
                    self.show_toast("Audio reconnected".to_string(), ToastKind::Success);
                }
                Err(e) => {
                    if self.audio_watch.on_failure(now) {
                        self.show_toast(format!("Audio reconnect failed: {e}"), ToastKind::Error);
                    }
                }
            }
        }
```

(`Ordering`/`Instant`/`Duration` уже импортированы gui/mod.rs:19-20;
`cpal::default_host()` — cpal в scope по прецеденту settings.rs:220.)

- [ ] **Step 5: зелёный + гейты** (GUI не имеет автотестов — логика покрыта
  tests/audio_hotplug.rs; wiring проверяется компиляцией и Task C4)

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 6: коммит**

```bash
git add src/gui/mod.rs src/lib.rs
git commit -m "feat: audio hotplug poller + reconnect retry with throttled toast (#18)"
```

### Task C3: TECHDEBT #18 — пометить реализованным

**Files:**
- Modify: `docs/TECHDEBT.md`

- [ ] **Step 1:** в пункте 18 после строки «умеет, см. PR #2); (c) выбор
  устройства: дефолт системы или из config.» дописать:

```markdown

РЕАЛИЗОВАНО 2026-10-29 (план docs/superpowers/plans/2026-10-29-metronome-hotplug.md):
GUI-поллер default-output-device (1.5 c, desktop; Android off — AAudio-роутинг),
ReconnectPolicy (Connected | Retrying{at, attempts}, backoff 2 c, тост только на
первой неудаче серии), AudioBackend.device_name. Подпункт (б) снят чтением:
reconnect_audio не пересоздаёт console_rx/LED-socket. Осталось: ручная
верификация hotplug по чеклисту плана; известное ограничение — ALSA-хост
(константное имя «default») поллером не отслеживается.
```

- [ ] **Step 2: коммит**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: techdebt #18 hotplug reconnect implemented"
```

### Task C4: Ручная верификация hotplug (чеклист, не коммитится)

Выполнить на desktop (PipeWire-хост, `cargo run`), отметить в PR:

1. Играть ноту → выдернуть наушники: стрим получает ошибку → reconnect
   (устройство-приёмник уже колонки) → звук продолжается на колонках, тост
   «Audio reconnected».
2. Вставить наушники обратно (default вернулся): поллер замечает смену имени
   ≤1.5 c → reconnect → звук в наушниках. В логе `[audio] default device
   changed: ...`.
3. Выдернуть наушники и НЕ давать системе переключиться на существующее
   устройство (нет второго устройства): первая неудача → один тост ошибки,
   дальше тихие retry каждые 2 c; вставить наушники → очередной retry
   подключается, тост успеха.
4. Дёргать устройство быстро 5-6 раз подряд: нет спама тостов (успех тостит,
   повторные неудачи нет), GUI не подвисает заметно (каждый reconnect ~0.3 s),
   CPU в покое не растёт.
5. Лупер: записать петлю → hotplug → петля играет после reconnect (PR #2).
6. LED-консоль/банк-2 пэды живы после reconnect (console_rx/socket не рвутся).
7. Headless (`cargo run -- --headless` если доступно): разрыв стрима по-прежнему
   печатает «restart to recover» (gui-поллер там отсутствует — by design).

---

## Финальные гейты (после всех групп)

```bash
cargo clippy --all-targets -- -D warnings   # clean
cargo test                                  # all green (metronome 10, audio_hotplug 8)
cargo test --features rt-assert --test rt_safety   # all green, no aborts
cargo deny check                            # clean
```

## Известные риски

1. **Группа A — тесты ожидаются зелёными сразу.** Это не «слабое TDD», а
   зафиксированный результат расследования: бага в движке нет. Красный тест =
   механизм найден → дерево диагностики (Task A1 Step 3) + фикс в найденном
   месте; тест тогда остаётся регрессионным.
2. **ALSA-хост слеп к hotplug** (константное имя «default»): поллер не
   сработает; там же стрим обычно не рвётся. PipeWire/JACK — имена трекают
   устройство. Задокументировано в techdebt #18.
3. **Unbounded retry** при отсутствующем устройстве: бесконечные тихие попытки
   раз в 2 c (одна попытка дешёвая). Если когда-нибудь станет проблемой — кап
   на attempts + статус в GUI, отдельный Item.
4. **tc6-tc9 интервалы 750±4 блока**: клик стартует в блоке, содержащем границу
   шага; Stitching сохраняет абсолютную шкалу; допуск покрывает ±1 блок jitter.
5. **device_name.clone() в литерале App** (lib.rs): клон обязан стоять до
   move `c._audio_handle`; компилятор поймает перестановку (use after move).
6. **reconnect_audio при Android не должен вызываться поллером** — поллер
   cfg-выключен; dead-stream путь на Android сохранён как был.
7. Полный **#19** (flamegraph, denormals-аудит, bisect по PR #4/#5) — не входит
   в этот план, остаётся в body techdebt #19.
