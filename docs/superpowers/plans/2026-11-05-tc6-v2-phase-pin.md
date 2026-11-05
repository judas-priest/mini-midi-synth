# tc6v2 — Deterministic No-Drain Pin (TECHDEBT #52а) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-05 (база = master d30f5a3; факты вычитаны из vendor/rustysynth и sampler.rs; практики — Tavily 2026-11-05: state-machine интроспекция вместо сигнала (CLAP/Rust ADSRState-паттерн), и первый раунд исследований 2026-11-03 (детерминизм DSP-тестов))

**Goal:** Детерминированный регрессионный пин «no-op `SetPartSf2Mode` не дренит
хвосты» — через интроспекцию фазы голосов в vendored rustysynth, вместо двух
провалившихся амплитудных попыток (10-блочное окно; 100-блок+преролл).

**Architecture:** ТриAccess-аксессора: `Voice::is_in_release()` (voice.rs,
состояние ≠ PLAYING), `Synthesizer::get_voices_in_release_count()`
(итерация `get_active_voices()`), `SamplerEngine::voices_in_release()` (в
Option-synth). Тест: держится нота → no-op Sf2Mode → счётчик == 0; контроль с
РЕАЛЬНЫМ флипом в DSP (drain) → счётчик > 0. Ноль окон, ноль порогов, ноль
флаков: фаза меняется атомарно в момент drain.

**Tech Stack:** Rust; правка vendored rustysynth (канон по CLAUDE.md: правки
вендора не откатывать при обновлении — прецеденты cpal/rustysynth уже есть);
тест в существующий tests/sf2_channel_gain.rs.

**Проверенные факты (вычитано; не пересматривать):**
- `Voice.voice_state: i32` (voice.rs:75); `VoiceState` — unit-struct с
  константами (:312); переходы: `start()` → PLAYING (:179), `note_off()`
  PLAYING → RELEASE_REQUESTED (:184-185), `process()`:
  RELEASE_REQUESTED && !hold_pedal → RELEASED (:292-297); `end()`:
  PLAYING → RELEASE_REQUESTED (:183-186).
- `Synthesizer.voices: VoiceCollection` (synthesizer.rs:31);
  `VoiceCollection::get_active_voices(&mut self) -> &mut [Voice]`
  (voice_collection.rs:89, pub(crate)) — итерируется в synthesizer.rs:188.
  `Voice` хранит канал? — НЕ проверено: для теста достаточно ОБЩЕГО счётчика
  по синту (в тесте звучит одна партия).
- `SamplerEngine.keys: SubSynth` (sampler.rs:204), `keys.synth: Option<Synthesizer>`
  (:94-95); образец Option-аксессора: `set_pitch_bend_range` (:574-578).
- `drain_notes` (synth/mod.rs:1083-1096): `part_all_notes_off(idx)` →
  `note_off_all_channel(ch, false)` → voices в RELEASE_REQUESTED/RELEASED;
  DSP-голоса → `voice.note_off()`. Real-flip контроль в тесте гарантированно
  даёт release-состояния.
- Существующий тест-файл tests/sf2_channel_gain.rs (5 тестов, tc6 удалён —
  см. план #52) + helpers `engine_with_sf2`/`gui_switch_part_to_sf2`.
- Right-side инвариант: аксессоры — read-only, НЕ вызываются из RT-пути
  (только тесты); `get_active_voices(&mut)` — нужен `&mut Synthesizer`,
  в тестах ок.

**Best practices (Tavily, вшиты):** state-машина голоса — первичный объект
ассертов (CLAP/Rust туториалы ассертят `ADSRState::Release` напрямую, не
выходной сигнал); детерминизм (Microsoft/testim) — фаза атомарна, окон нет;
STK-канон: drain = note_off → release-фаза, не мгновенный kill (kill=false).

---

## Test Cases (from the requirement)

Требование (#52а): пин «no-op Sf2Mode не дренит» — детерминированный, без
амплитудных окон.

- TC-A (главный пин): SF2-парта держит ноту (voices PLAYING) → no-op
  Sf2Mode(true) при уже-true → `voices_in_release() == 0`. ДО фикса-класса
  (гипотетический регресс: безусловный drain) → > 0 — тест красный.
- TC-B (контроль семантики drain): тот же сетап, РЕАЛЬНЫЙ флип Sf2Mode(false)
  → `voices_in_release() > 0` — доказывает, что счётчик вообще видит drain
  (иначе TC-A тривиально зелёный).
- TC-C (не-регрессия существующих): все 5 старых тестов файла зелёные.

## Negative Matrix

- SF2-файл отсутствует — автопропуск (канон файла).
- Счётчик при ОТСУТСТВУЮЩем synth — 0 (Option guard), тест не дойдёт (нет
  звука) — неважно, TC-B использует живой синт.
- HOLD-педаль внизу во время drain — voice остаётся RELEASE_REQUESTED (не
  RELEASED) — ОБА состояния ≠ PLAYING, счётчик их покрывает (is_in_release =
  state != PLAYING).
- RT: аксессоры только в тестах; `get_active_voices(&mut)` требует &mut
  Synthesizer — тесты владеют движком.
- Несколько голосов (унисон) — счётчик суммарный; ассерты относительные
  (0 vs >0), точность не нужна.
- Вендор-обновление rustysynth — правки переносить руками (CLAUDE.md-канон,
  прецедент cpal/oboe).

---

### Task 1: Аксессоры фазы (vendor + sampler)

**Files:**
- Modify: `vendor/rustysynth/src/voice.rs` (pub-метод на Voice)
- Modify: `vendor/rustysynth/src/synthesizer.rs` (pub-метод на Synthesizer)
- Modify: `src/synth/sampler.rs` (pub-метод на SamplerEngine)

- [ ] **Step 1: Voice::is_in_release** — в `impl Voice` (voice.rs, рядом с
  `kill()`) добавить:

```rust
    /// Фаза ≠ PLAYING (RELEASE_REQUESTED или RELEASED) — голос отпущен и
    /// доигрывает хвост. Пин «no-op не дренит» (#52а): интроспекция вместо
    /// амплитудных окон.
    pub fn is_in_release(&self) -> bool {
        self.voice_state != VoiceState::PLAYING
    }
```

- [ ] **Step 2: Synthesizer::get_voices_in_release_count** — в impl
  Synthesizer (рядом с `process_midi_message`):

```rust
    /// Число активных голосов в release-фазе (все каналы). Тест-аксессор:
    /// пин «no-op Sf2Mode не дренит» (#52а). НЕ вызывать из RT-кода.
    pub fn get_voices_in_release_count(&mut self) -> usize {
        self.voices
            .get_active_voices()
            .iter()
            .filter(|v| v.is_in_release())
            .count()
    }
```

- [ ] **Step 3: SamplerEngine::voices_in_release** — в src/synth/sampler.rs
  (рядом с set_pitch_bend_range):

```rust
    /// Тест-аксессор фазы голосов SF2-синта (TECHDEBT #52а). НЕ для RT-пути
    /// (get_active_voices требует &mut).
    pub fn voices_in_release(&mut self) -> usize {
        if let Some(synth) = &mut self.keys.synth {
            synth.get_voices_in_release_count()
        } else {
            0
        }
    }
```

- [ ] **Step 4: компиляция**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2`
Expected: чисто (новые методы пока не используются вне тестов — если
dead_code: методы pub на pub-типах в lib-crate НЕ мёртвый код; SamplerEngine
pub-метод тоже; проверка выдаст).

- [ ] **Step 5: коммит**

```bash
git add vendor/rustysynth/src/voice.rs vendor/rustysynth/src/synthesizer.rs src/synth/sampler.rs && git commit -m "feat: voice-phase introspection accessors — Voice/Synthesizer/SamplerEngine (#52а)"
```

### Task 2: tc6v2 — детерминированный пин

**Files:**
- Modify: `tests/sf2_channel_gain.rs` (добавить 2 теста; helpers уже есть)

- [ ] **Step 1: тесты** (рядом с tc5):

```rust
/// #52а: детерминированный пин «no-op Sf2Mode не дренит» (замена
/// амплитудного tc6: 2 попытки провалились — release-vs-sustain
/// неразличимы по сигналу; фаза голоса атомарна).
#[test]
fn tc6a_noop_switch_does_not_release_voices() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, false, 0, 127);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    // прогреть: голос PLAYING, счётчик release == 0
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..10 { e.tick_block(&mut l, &mut r); }
    assert_eq!(e.sampler_voices_in_release(), 0, "held note must be PLAYING");
    // no-op switch (SF2 -> SF2): голос обязан остаться PLAYING
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    assert_eq!(e.sampler_voices_in_release(), 0, "no-op switch released voices (drain on no-op)");
}

#[test]
fn tc6b_real_flip_releases_voices() {
    // Контроль TC-A: счётчик вообще видит drain (иначе tc6a тривиален)
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, false, 0, 127);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..10 { e.tick_block(&mut l, &mut r); }
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: false });
    assert!(e.sampler_voices_in_release() > 0, "real flip did not release voices");
}
```

САМПЛЕР-МОСТ: `e.sampler_voices_in_release()` — у SynthEngine нет публ.
поля sampler; добавить в src/synth/mod.rs (impl SynthEngine) мост:

```rust
    /// Тест-аксессор #52а. НЕ для RT-пути.
    pub fn sampler_voices_in_release(&mut self) -> usize {
        self.sampler.voices_in_release()
    }
```

(в Files Task 2 добавить src/synth/mod.rs.)

- [ ] **Step 2: прогнать — зелёные**

Run: `cargo test --test sf2_channel_gain 2>&1 | tail -10`
Expected: 7/7 ok (5 старых + tc6a/tc6b). Если tc6a красный на ТЕКУЩЕМ коде —
это НАЙДЕН РЕАЛЬНЫЙ баг (no-op дренит): НЕ править тест, отчитаться пиками
и кодом обработчика.

- [ ] **Step 3: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`
Expected: чисто, всё зелёное.

- [ ] **Step 4: коммит**

```bash
git add tests/sf2_channel_gain.rs src/synth/mod.rs && git commit -m "test: tc6a/tc6b — deterministic no-drain pin via voice-phase introspection (#52а)"
```

### Task 3: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch (release-бинарь не меняет
  поведение — только тесты/аксессоры; перезапуск для консистентности)
- [ ] TECHDEBT #52а: пометить ГОТОВО (ссылка на PR; амплитудный tc6 остаётся
  снятым)

## Out of scope

- Структурная переделка Bowed (питч-корректность мод) — отдельный план (#53 out of scope).
- Питч-специфичный счётчик (voices на КОНКРЕТНОМ канале) — общий счётчик
  достаточен для одно-партного теста.
