# Quick Wins batch: RT-alloc fixes (QW1/QW2), drain_notes helper (QW3), looper atoms (QW6), config debounce (QW9), dead code sweep (QW8)

anchor-checked: 2026-10-30 (все якоря сняты чтением кода на master @ a4426fc в этой сессии; номера строк REFACTOR_AUDIT.md могли дрейфовать — здесь актуальные)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Каждый таск исполняется СВЕЖИМ субагентом, получающим только этот документ.

**Goal:** Закрыть пять quick wins из REFACTOR_AUDIT.md:
- **QW1** — wavetable-аллокация на каждом note-on (8 KB heap + Arc + 13 312 `sin()` + dealloc) → общий `OnceLock<Arc<Vec<f32>>>` кэш built-in таблицы + безусловный прогрев в `init_common` + rt-assert тест note-on с Wavetable-патчем.
- **QW2** — LFO sine LUT (8 KB `Box<[f32; 2048]>`) лениво строится на аудиопотоке → безусловный прогрев в `init_common` до создания стрима.
- **QW3** — `MidiSeqLoad` / `MidiSeqPlay`-stop / `AllNotesOff` не чистят `root_held`/`root_active` → ghost-rekey на root_only-патче; хелпер `Part::drain_notes` закрывает пробел и дедуплицирует 7 cleanup-сайтов.
- **QW6** — looper-атомы пишутся на каждом сэмпле (4 atomic store + f64 деление, ~48k/s @48 kHz, даже в Idle) → write-on-change с кэшем + force-refresh в restore/clear/stop.
- **QW9** — `save_global_to_config()` (весь Config в pretty-JSON = 2 аллокации + write+rename) на каждый changed-кадр при драге sustain-release/SF2-rel слайдеров → существующий `global_dirty` debounce.
- **QW8** — dead code sweep: `_save_window_size`, пустая ветка, placeholder-комментарии, мёртвая пара `load_patch`, 3 мёртвых `save_to_params` (+2 осиротевших `index()`), устаревший `#[allow(dead_code)]` на живом `Config::save`.

**Architecture:**
- Аудиопоток: cpal callback → `SynthEngine::tick_block` (src/synth/mod.rs). RT-контракт (CLAUDE.md): никаких аллокаций/мьютексов/IO; heap → `src/trash.rs`. Все правки Tasks 1–4 лежат в этой зоне — после каждой нужен прогон `cargo test --features rt-assert --test rt_safety`.
- `init_common` (src/lib.rs:142) вызывается из `desktop_main` (:683) и `run_headless` (:410) СТРОГО до `AudioBackend::new` (:252) — правильное место для прогрева статических таблиц (про GTA-thread, до старта callback'а).
- Note lifecycle: каждый note-on → `Voice::init` (src/synth/voice.rs:245 `oscs[0].reset()` + match :248–346). Для `OscType::Wavetable` (param `osc_type` = 27.0, oscillator.rs:122) путь: `reset()` ставит placeholder-состояние с пустым `Arc<Vec<f32>>` (oscillator.rs:641 — аллокация!), затем `init_wavetable` (voice.rs:332 → oscillator.rs:2545) пересобирает таблицу 8×256 через `take_buf` (mem::take + resize = аллокация при каждом вызове, т.к. Wavetable-буферы исключены из recycle — oscillator.rs:470–472) и заворачивает в новый `Arc`. Кастомные .wt идут через дешёвый `Arc::clone` (voice.rs:326 → `init_wavetable_from_data`) — не трогаем.
- Root-only gate: `Part::root_on`/`root_off` (src/synth/mod.rs:933–957) ведут `root_held: [bool; 128]` + `root_active: Option<u8>`. `SetPartEnabled`/`SetPartMute`/`SetPartSf2Mode`/CC120/123 их чистят (mod.rs:2154–2155, 2179–2180, 2484–2485, 1980–1981), а `MidiSeqLoad` (2561–2565), `MidiSeqPlay`-stop (2592–2596) и `AllNotesOff` (2425–2435) — НЕТ: после MIDI-stop/паники stale `root_active` давит все новые ноты выше него (`root_on` → `Suppress`, mod.rs:946).
- Looper: `MidiLooper::update_atoms` (src/synth/looper.rs:577–585) зовётся из всех трёх веток `tick` (:472 Idle, :486 Recording, :542 Playing/Overdub) — на каждом сэмпле. `state`/`len`/`current_layer` меняются только по control-границам; `position` квантован в u8 и после wrap-reset (:528–542) считается от нового playback_tick, поэтому кэш-сравнение wrap 254→0 обрабатывает само.
- GUI debounce: `App.global_dirty` + 2-секундный таймер (src/gui/mod.rs:574–577) уже существует и используется тоном/ревер/volume-слайдерами (:937, :951, :965, :980); `save_global_to_config` (:1497–1516) сериализует `sustain_release`/`sf2_release_scale` (:1505–1506), так что перевод двух слайдеров на debounce ничего не теряет.

**Tech Stack:** Rust 2021; `std::sync::OnceLock` (уже используется в lfo.rs:6); test harness `tests/rt_safety.rs` (feature `rt-assert`, `assert_no_alloc`), `tests/root_only.rs`, `tests/sustain_audit.rs`, unit-модуль `src/synth/tests.rs`, `#[cfg(test)] mod restore_tests` в looper.rs. Без новых зависимостей.

**Правила (CLAUDE.md, binding):** никаких аллокаций в аудио-callback; **`cargo fmt` по всей базе НЕ запускать**; тесты писать из требования, negative cases обязательны; ослабление существующего теста — только с явного согласия пользователя.

**Проверки перед каждым коммитом (добавить `cargo deny check` перед финальным):**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требования записаны ДО реализации. Каждый нумерованный TC — отдельный тест.

1. **TC1 (QW1)** `wavetable_note_on_does_not_allocate` — движок с part 0 = патч `osc_type=27.0` (built-in wavetable, без .wt данных), статические таблицы прогреты → NoteOn + 8 блоков + NoteOff + 8 блоков внутри `assert_no_alloc` не абортят. (RED до Task 1: сегодня note-on с wavetable делает `Arc::new(Vec::new())` в `reset()` + `take_buf`+`Arc::new` в `init_wavetable` внутри guarded-зоны → abort. Именно пробел в покрытии — существующий `tick_block_does_not_allocate` держит NoteOn ВНЕ guard'а (rt_safety.rs:22) — позволил багу жить.)
2. **TC2 (QW1, guard)** существующий `switching_between_wavetable_patches_does_not_free_in_rt` (rt_safety.rs:88) и весь rt_safety suite остаются зелёными — кастомный .wt путь (`Arc::clone`) не изменился.
3. **TC3 (QW2)** `lfo_sine_lut_does_not_allocate` — после `warmup_sine_lut()` 128 тиков `tick_with_deform` (Sine deform 0.8 + Triangle deform 0.8) внутри guard'а не абортят. (RED до Task 2: compile error — `warmup_sine_lut` не существует; семантически первый тик Sine вызывает `sine_lut` (lfo.rs:354) и `apply_deform` при `|deform| >= 0.5` для ЛЮБОЙ волны (lfo.rs:286) → get_or_init 8 KB внутри guard'а.)
4. **TC4 (QW3)** `all_notes_off_clears_root_tracking` — root_only part: NoteOn 36 (root звучит), NoteOn 60 (подавлен), `ControlEvent::AllNotesOff`, затем NoteOn 48 → 48 ЗВУЧИТ (новый корень). (RED до Task 3: stale `root_active=Some(36)` → 48 подавлен, тишина.)
5. **TC5 (QW3)** `midi_seq_stop_clears_root_tracking` — та же последовательность, но вместо AllNotesOff — `ControlEvent::MidiSeqPlay { playing: false }`. (RED до Task 3.)
6. **TC6 (QW3, unit)** `all_notes_off_clears_root_held_and_root_active` — прямой ассерт на приватных полях `parts[0].root_active.is_none()` / `root_held` все false после AllNotesOff при активном root. (RED до Task 3; живёт в `src/synth/tests.rs`, где видны приватные поля.)
7. **TC7 (QW3, guard)** весь `tests/sustain_audit.rs` зелёный (включая `cc120_kills_sound_immediately`, `cc123_keeps_release_tail_then_silences`, `part_disable_releases_pedal_held_notes`, `mute_gates_the_mix_within_10ms`) — доказательство, что дедупликация 7 cleanup-сайтов не изменила DSP-поведение.
8. **TC8 (QW6)** `clear_refreshes_atoms_immediately` — запись события, один `tick()` публикует `event_count=1`, `clear()` → атомы `event_count=0/state=Idle/layer_count=1` БЕЗ вызова tick. (RED до Task 4: `clear()` вообще не зовёт `update_atoms` — атомы протухают до следующего тика.)
9. **TC9 (QW6)** `stop_publishes_idle_state_without_tick` — Playing (подтверждено тиком), `toggle_play()` → stop → атом state = 0 (Idle) без тика. (RED до Task 4: атом остаётся 2.)
10. **TC10 (QW6, guard)** `position_atom_walks_and_wraps` — loop 256 тиков, 600 тиков: позиция доходит до 254 и после wrap возвращается в 0 (кэш переживает wrap; замечание: u8-позиция в принципе НЕ достигает 255 — `(t*255/len) as u8` при `t<len` даёт max 254 — существующее поведение, не меняем).
11. **TC11 (QW9)** ручная проверка (GUI автотестами не покрывается): драг SR-слайдера 3 секунды → mtime `~/.config/mini_midi_synth/config.json` меняется ≤ 1 раза за 2 с (debounce), значение сохраняется после остановки драга; SF2 rel — аналогично. Существующие значения перечитываются после рестарта.
12. **TC12 (QW8)** регресс-гейты: `cargo clippy --all-targets -- -D warnings` чист (удаление мёртвого кода не оставило осиротевших ссылок), весь `cargo test` зелёный, grep подтверждает ноль оставшихся вхождений удалённых имён.

## Negative Matrix

- **Missing field / null:** QW1-тест ставит `osc_type=27.0` при `wavetable_data=None` — built-in ветка; presets с .wt идут через `init_wavetable_from_data` и не затрагиваются (TC2). Если фабричный Init-пресет пропадёт — тест падает с понятным сообщением, не паникует (`expect`).
- **Zero/empty:** QW1 placeholder-таблица (пустой `Arc<Vec<f32>>` в `reset()`) становится общим статическим Arc — содержимое то же (пусто), поведение `tick_wavetable` на пустой таблице не меняется (таблица всё равно заменяется `init_wavetable` до первого тика; сегодня ровно та же пустая таблица создавалась per-note-on). QW6: `clear()` при пустом лупере → `sync_atoms` публикует нули, идемпотентно; повторный `clear()` — no-op по атомам (кэш совпадает).
- **Duplicate/concurrent:** QW3: повторный AllNotesOff — helper идемпотентен (note_off на releasing-голосе перезаряжает release с текущего уровня — как и сегодня); CC120 после CC123 — kill поверх release, net-kill (как сегодня). QW6: `sync_atoms` при уже совпадающих значениях — одна лишняя перезапись кэша, атомы не трогаются повторно.
- **Ordering:** QW3: все три gap-сайта уже зовут `set_sustain(0)` ДО цикла (mod.rs:2557, 2587, 2424) — pedal/sustained-состояние поднято до drain; helper не трогает `part.sustain_pedal` (его выставляет `set_sustain`, mod.rs:1494), поэтому у `SetPartEnabled`/`SetPartMute`/`SetPartSf2Mode` семантика педали не меняется. QW6: `stop()` зовёт `sync_atoms` до следующего `tick` — атом не может мигнуть старым значением, т.к. tick после stop() попадает в Idle-ветку и сравнение с кэшем даёт no-op.
- **Clock skew между независимыми часами:** QW9 использует уже существующий монотонный `Instant` (`last_config_save`) — не wall-clock; skew неприменим. QW6 позиция — счётчик сэмплов аудиопотока, единственный источник.
- **External-call timeout:** QW9: `config.save()` остаётся на GUI-потоке (fs write+rename), но теперь ≤ 1/2 с вместо ≤ 60/с — Android FS перестаёт страдать. Новых cross-thread вызовов нет.
- **Extreme values:** QW6: `loop_length_ticks == 0` → pos 0 (ветка сохранена); u16-кэш позиций покрывает все u8. QW3: `parts.get_mut(part >= 16)` → None → no-op (как сегодня).
- **Warmup пропущен (QW1/QW2):** `get_or_init` остаётся как страховка — если кто-то строит `SynthEngine` в обход `init_common` (тесты, будущий headless-путь), таблица строится лениво на первом использовании того потока; prod-путь (`init_common`) прогревает обе таблицы до `AudioBackend::new`. Rt-тесты зовут warmup явно, поэтому guard измеряет именно note-on/tick-путь.

---

### Task 0: Ветка

- [ ] Из master:
```bash
cd /home/dima/Projects/mini_midi_synth
git checkout master && git pull
git checkout -b refactor-qw-batch
```

---

# Группа A: RT-аллокации (QW1 → QW2, независимые таски)

### Task 1: QW1 — кэш built-in wavetable + прогрев + rt-тест

**Files:**
- Modify: `tests/rt_safety.rs` (TC1)
- Modify: `src/synth/oscillator.rs` (статики, `init_wavetable`, reset-arm, warmup)
- Modify: `src/lib.rs` (прогрев в `init_common`)

- [ ] **Step 1: failing-тест** — добавить в `tests/rt_safety.rs` (после `switching_between_wavetable_patches_does_not_free_in_rt`):

```rust
#[test]
fn wavetable_note_on_does_not_allocate() {
    // Built-in wavetable bank: every note-on used to rebuild the 8x256 table
    // (take_buf + Arc::new + 13312 sin()) and drop an empty placeholder Arc
    // from reset(). The OnceLock cache plus warmup must make the whole
    // note-on path allocation-free. Plan: docs/superpowers/plans/2026-10-30-refactor-qw-batch.md
    let mut patch = preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists");
    patch.params.insert("osc_type".into(), 27.0); // OscType::Wavetable, built-in bank
    patch.wavetable_data = None;                  // custom .wt path is already Arc-shared

    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });

    // init_common does this before the audio stream exists; the test mirrors it
    // so the guard below measures the note-on path, not the lazy build.
    mini_midi_synth::synth::oscillator::warmup_builtin_wavetable();

    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        for _ in 0..8 { engine.tick_block(&mut l, &mut r); }
        engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
        for _ in 0..8 { engine.tick_block(&mut l, &mut r); }
    });
}
```

- [ ] **Step 2: красный** — ожидание: НЕ компилируется (`warmup_builtin_wavetable` не существует):

```bash
cargo test --features rt-assert --test rt_safety
# error[E0599]: no function or associated item named `warmup_builtin_wavetable`
```

(Это красный этап TDD для нового API. Семантический красный проверяется после Step 3 временно: закомментировать вызов warmup в тесте — guard должен абортнуть на первом NoteOn; вернуть вызов.)

- [ ] **Step 3: статики** — `src/synth/oscillator.rs`, после `use super::piano::PianoModel;` (строка 8) добавить импорт и статики:

```rust
use std::sync::OnceLock;

/// Shared built-in wavetable bank (8 x 256 additive waveforms, sine->saw).
/// Built once by `warmup_builtin_wavetable` (called from init_common, before
/// the audio stream exists) so a note-on only clones the Arc. `get_or_init`
/// stays as a safety net for engine users that skipped warmup (tests) — the
/// build then happens off the audio thread's first note, not per note-on.
static BUILTIN_WAVETABLE: OnceLock<Arc<Vec<f32>>> = OnceLock::new();
/// Shared empty placeholder table for reset(); the real table arrives via
/// init_wavetable before the first tick. Previously this was a fresh
/// `Arc::new(Vec::new())` on EVERY note-on.
static EMPTY_WAVETABLE: OnceLock<Arc<Vec<f32>>> = OnceLock::new();

fn build_builtin_wavetable() -> Arc<Vec<f32>> {
    let wt_frames = Oscillator::WT_COUNT;
    let wt_frame_size = Oscillator::WT_SIZE;
    let mut table = vec![0.0_f32; wt_frames * wt_frame_size];
    let harmonics: [usize; 8] = [1, 2, 3, 4, 6, 8, 12, 16];
    for (wt, &n_harm) in harmonics.iter().enumerate() {
        let offset = wt * wt_frame_size;
        let mut peak = 0.0_f32;
        for i in 0..wt_frame_size {
            let p = i as f32 / wt_frame_size as f32;
            let mut s = 0.0f32;
            for h in 1..=n_harm {
                s += (p * h as f32 * TAU).sin() / h as f32;
            }
            table[offset + i] = s;
            peak = peak.max(s.abs());
        }
        if peak > 0.001 {
            for x in &mut table[offset..offset + wt_frame_size] { *x /= peak; }
        }
    }
    Arc::new(table)
}

/// Pre-builds both static wavetable Arcs. MUST be called from init_common
/// before the audio stream starts; without it the first Wavetable note-on
/// builds the bank (8 KB alloc + 13312 sin()) in the audio callback.
pub fn warmup_builtin_wavetable() {
    let _ = BUILTIN_WAVETABLE.get_or_init(build_builtin_wavetable);
    let _ = EMPTY_WAVETABLE.get_or_init(|| Arc::new(Vec::new()));
}
```

- [ ] **Step 4: `init_wavetable` из кэша** — заменить тело `init_wavetable` (oscillator.rs:2545–2573) на:

```rust
    /// Initialize built-in wavetable (8 additive waveforms, sine→saw).
    pub fn init_wavetable(&mut self, morph: f32) {
        self.wavetable_morph = morph.clamp(0.0, 1.0);
        self.reclaim_buffers();
        // Shared bank: Arc::clone instead of a per-note-on rebuild (the old
        // take_buf path allocated on every note-on because Wavetable buffers
        // are excluded from the pool recycle — see reclaim_buffers).
        let table = BUILTIN_WAVETABLE.get_or_init(build_builtin_wavetable).clone();
        self.state = OscState::Wavetable {
            phase: 0.0,
            table,
            wt_frames: Self::WT_COUNT,
            wt_frame_size: Self::WT_SIZE,
        };
    }
```

- [ ] **Step 5: пустой placeholder из статики** — в `reset()`, arm `OscType::Wavetable` (oscillator.rs:639–644), заменить `table: Arc::new(Vec::new()),` на:

```rust
            OscType::Wavetable => OscState::Wavetable {
                phase: 0.0,
                table: EMPTY_WAVETABLE.get_or_init(|| Arc::new(Vec::new())).clone(),
                wt_frames: Self::WT_COUNT,
                wt_frame_size: Self::WT_SIZE,
            },
```

- [ ] **Step 6: прогрев в `init_common`** — `src/lib.rs`, сразу после `let patches = preset::load_all_patches();` (строка 144):

```rust
    // Warm static DSP tables BEFORE the audio stream exists: both are lazily
    // built otherwise and the first build would land in the audio callback
    // (wavetable bank on the first note-on, LFO sine LUT on the first tick).
    synth::oscillator::warmup_builtin_wavetable();
```

(QW2 добавит вторую строку в Task 2 — можно вставить обе сразу и в Task 1, тогда убрать комментарий «QW2 добавит». Компилятор не требует.)

- [ ] **Step 7: зелёный** —

```bash
cargo test --features rt-assert --test rt_safety
# test wavetable_note_on_does_not_allocate ... ok (и все 5 старых тестов ok)
```

Если red (abort "memory allocation/free"): аллокация в другом месте note-on пути с этим патчем — сузить guard (сначала только `tick_block` внутри guard, NoteOn снаружи), найти offending-вызов чтением `Voice::init`/`route_note_on` и починить его отдельно; НЕ ослаблять тест.

- [ ] **Step 8: гейты + коммит** —

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
git add src/synth/oscillator.rs src/lib.rs tests/rt_safety.rs
git commit -m "perf: cache built-in wavetable in OnceLock, warm up in init_common — note-on no longer allocates 8KB + Arc per press"
```

**RT-safety note:** после фикса note-on путь для Wavetable делает только `Arc::clone` (atomic refcount) и `get_or_init`-lookup (atomic load) — RT-safe. Прогрев живёт на control-потоке до старта стрима.

### Task 2: QW2 — прогрев LFO sine LUT

**Files:**
- Modify: `tests/rt_safety.rs` (TC3)
- Modify: `src/synth/lfo.rs` (`warmup_sine_lut`)
- Modify: `src/lib.rs` (вторая строка прогрева)

- [ ] **Step 1: failing-тест** — добавить в `tests/rt_safety.rs`:

```rust
#[test]
fn lfo_sine_lut_does_not_allocate() {
    use mini_midi_synth::synth::lfo::{Lfo, LfoWaveform};
    // The 8 KB sine LUT is hit by every Sine tick AND by apply_deform at
    // deform >= 0.5 for ANY waveform (lfo.rs apply_deform). Without warmup it
    // is built lazily on the audio thread's first tick.
    mini_midi_synth::synth::lfo::warmup_sine_lut();
    let mut lfo = Lfo::new(SR);
    assert_no_alloc(|| {
        for _ in 0..64 {
            // deform 0.8 exercises both LUT consumers
            let _ = lfo.tick_with_deform(5.0, LfoWaveform::Sine, 0.8);
            let _ = lfo.tick_with_deform(5.0, LfoWaveform::Triangle, 0.8);
        }
    });
}
```

- [ ] **Step 2: красный** — ожидание: compile error (`warmup_sine_lut` не существует):

```bash
cargo test --features rt-assert --test rt_safety
# error[E0599]: no function or associated item named `warmup_sine_lut` found
```

- [ ] **Step 3: warmup fn** — `src/synth/lfo.rs`, заменить `sine_lut` (строки 8–18) на:

```rust
fn build_sine_lut() -> Box<[f32; 2048]> {
    let mut t = Box::new([0.0f32; 2048]);
    for i in 0..2048 {
        t[i] = (i as f32 / 2048.0 * std::f32::consts::TAU).sin();
    }
    t
}

/// Pre-builds the 2048-sample sine LUT. Called from init_common before the
/// audio stream exists: the LUT is read by every Sine tick and by
/// apply_deform (deform >= 0.5, any waveform) — lazily the first read would
/// allocate 8 KB in the audio callback.
pub fn warmup_sine_lut() {
    let _ = SINE_TABLE.get_or_init(build_sine_lut);
}

fn sine_lut(phase: f32) -> f32 {
    let table = SINE_TABLE.get_or_init(build_sine_lut);
    let idx = (phase * 2048.0) as usize & 2047;
    table[idx]
}
```

- [ ] **Step 4: вторая строка прогрева** — `src/lib.rs`, в `init_common` рядом с вызовом из Task 1 Step 6:

```rust
    synth::lfo::warmup_sine_lut();
```

- [ ] **Step 5: зелёный + гейты + коммит** —

```bash
cargo test --features rt-assert --test rt_safety
cargo clippy --all-targets -- -D warnings
cargo test
git add src/synth/lfo.rs src/lib.rs tests/rt_safety.rs
git commit -m "perf: warm LFO sine LUT in init_common — no lazy 8KB alloc on the audio thread"
```

**RT-safety note:** `get_or_init`-lookup в тике — atomic load, без изменений runtime-профиля.

---

# Группа B: QW3 — Part::drain_notes (закрытие root_held/root_active)

### Task 3: хелпер drain_notes + 7 сайтов

**Files:**
- Modify: `tests/root_only.rs` (TC4, TC5)
- Modify: `src/synth/tests.rs` (TC6)
- Modify: `src/synth/mod.rs` (хелпер + сайты)

**Факты (anchor-verified на a4426fc):** полный per-part cleanup-блок («note_off голосам + part_all_notes_off + 6 полей: sustained_notes, keys_down, note_stack, latch_held, root_held, root_active») дублируется в `SetPartEnabled` (mod.rs:2140–2155), `SetPartMute` (2163–2180), `SetPartSf2Mode` (2475–2485); CC120/123 arm (1972–1985) — тот же блок плюс `v.kill()` для CC120 и `sustain_pedal = 0.0` (избыточно: `set_sustain(0)` на :1970 уже обнуляет педаль всем частям, mod.rs:1494). Три сайта чистят ТОЛЬКО голоса и note_stack/latch/sustain-поля, оставляя `root_held`/`root_active` stale: `MidiSeqLoad` (2561–2565), `MidiSeqPlay`-stop (2592–2596), `AllNotesOff` (2425–2435) — ghost-rekey на root_only-патче (ср. с комментарием на 2190–2191). `part_all_notes_off(part)` — sampler.rs:468.

- [ ] **Step 1: failing-тесты** — в `tests/root_only.rs` добавить (после существующих тестов; harness `render_peak`/`settle` уже в файле):

```rust
fn root_only_engine() -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    let _ = e.handle_control(ControlEvent::SetPartRootOnly { part: 0, on: true });
    e
}

#[test]
fn all_notes_off_clears_root_tracking() {
    // Stale root_active after a panic must not ghost-suppress the next note
    // on a root_only patch: press root 36 + suppressed 60, AllNotesOff, then
    // 48 must sound as the new lowest (and only) root.
    let mut e = root_only_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 36, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 }); // suppressed
    assert!(render_peak(&mut e, 50) > SILENCE, "root 36 did not sound");
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    settle(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 127 });
    let peak = render_peak(&mut e, 100);
    assert!(peak > SILENCE, "note 48 ghost-suppressed after AllNotesOff (peak {peak})");
}

#[test]
fn midi_seq_stop_clears_root_tracking() {
    // Same ghost through the MIDI transport stop path.
    let mut e = root_only_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 36, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 50) > SILENCE, "root 36 did not sound");
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    settle(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 127 });
    let peak = render_peak(&mut e, 100);
    assert!(peak > SILENCE, "note 48 ghost-suppressed after MIDI-seq stop (peak {peak})");
}
```

- [ ] **Step 2: unit-тест** — в конец `src/synth/tests.rs` (модуль имеет `use super::*`, приватные поля Part видны):

```rust
// ── Part::drain_notes root-state guard ─────────────────────────

#[test]
fn all_notes_off_clears_root_held_and_root_active() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRootOnly { part: 0, on: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 36, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }); // suppressed
    for _ in 0..50 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(36), "root tracking did not engage");
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    assert!(e.parts[0].root_active.is_none(), "root_active survived AllNotesOff");
    assert!(e.parts[0].root_held.iter().all(|&h| !h), "root_held survived AllNotesOff");
    assert!(e.parts[0].note_stack.is_empty(), "note_stack survived AllNotesOff");
    assert!(!e.parts[0].keys_down.iter().any(|&k| k), "keys_down survived AllNotesOff");
}
```

- [ ] **Step 3: красный** —

```bash
cargo test --test root_only all_notes_off_clears_root_tracking
cargo test --test root_only midi_seq_stop_clears_root_tracking
cargo test --lib all_notes_off_clears_root_held
```
Ожидание: все три FAIL (`ghost-suppressed` / `root_active survived`). Остальные тесты root_only/sustain_audit зелёные.

- [ ] **Step 4: хелпер** — `src/synth/mod.rs`, в `impl Part` сразу после `root_off` (после строки 957):

```rust
    /// Drains a part on state changes / panic: releases (or kills) DSP voices,
    /// releases this part's SF2 voices, and clears every held-note tracking
    /// field — pedal-held notes, keys down, mono/latch stacks, and the
    /// root-only rekey state. Without the root clear a stale root_active
    /// ghost-suppresses the next note after MIDI-stop / AllNotesOff on
    /// root_only patches (see root_on/root_off). `kill`: CC120 semantics
    /// (immediate) vs release (CC123 / state change). Caller must have lifted
    /// engine-level sustain via set_sustain(0) first — this fn does not touch
    /// part.sustain_pedal. Zero-allocation (audio thread).
    fn drain_notes(&mut self, sampler: &mut SamplerEngine, idx: usize, kill: bool) {
        sampler.part_all_notes_off(idx);
        for voice in &mut self.voices {
            if voice.active {
                if kill { voice.kill(); } else { voice.note_off(); }
            }
        }
        self.sustained_notes = [false; 128];
        self.keys_down = [false; 128];
        self.note_stack.clear();
        self.latch_held.clear();
        self.root_held = [false; 128];
        self.root_active = None;
    }
```

- [ ] **Step 5: переписать 7 сайтов** (каждый — точная замена, поведение задокументировано):

1. **SetPartEnabled** (mod.rs:2138–2159) — заменить весь arm:
```rust
            ControlEvent::SetPartEnabled { part, enabled } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if !enabled {
                        l.drain_notes(&mut self.sampler, part, false);
                    }
                    l.enabled = enabled;
                }
            }
```
2. **SetPartMute** (mod.rs:2160–2186) — заменить весь arm:
```rust
            ControlEvent::SetPartMute { part, mute } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.mute = mute;
                    if mute {
                        // Drain both voice owners + held-note state; SF2 output
                        // is summed globally in tick_block, so also gate at the
                        // source with CC7 (instant; the release drain above
                        // keeps the tail short).
                        l.drain_notes(&mut self.sampler, part, false);
                        self.sampler.set_layer_volume(part, 0.0);
                    } else {
                        // Restore the SF2 channel volume zeroed on mute.
                        self.sampler.set_layer_volume(part, l.volume);
                    }
                }
            }
```
3. **CC120/123 arm** (mod.rs:1966–2001) — заменить per-part цикл (1972–1985) на:
```rust
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        part.drain_notes(&mut self.sampler, i, all_sound_off);
                    }
```
   (Строки `part.note_stack.clear()` … `part.sustain_pedal = 0.0` внутри старого цикла удаляются: helper покрывает все поля, а `sustain_pedal` уже обнулён `set_sustain(0)` выше. Глобальные `all_sound_off()`/`all_notes_off()` ПОСЛЕ цикла (1986–1990) остаются — они покрывают drum-канал и каналы, не замапленные на части; per-part `part_all_notes_off` до них избыточен, но идемпотентен для rustysynth.)
4. **AllNotesOff** (mod.rs:2423–2445) — заменить цикл (2425–2436) на:
```rust
                for (i, part) in self.parts.iter_mut().enumerate() {
                    part.drain_notes(&mut self.sampler, i, false);
                }
```
   (Остальное в arm не менять.)
5. **SetPartSf2Mode** (mod.rs:2468–2494) — удалить отдельный вызов `self.sampler.part_all_notes_off(part);` (2475) и заменить блок очистки (2477–2485) на `l.drain_notes(&mut self.sampler, part, false);`; `l.sf2_mode = enabled;` и pan-блок остаются:
```rust
                // Engine switch: both voice owners must be drained
                if let Some(l) = self.parts.get_mut(part) {
                    l.drain_notes(&mut self.sampler, part, false);
                    l.sf2_mode = enabled;
                    // (Re)apply the part's pan on the SF2 channel — CC10 was
                    // never sent while the part was in DSP mode.
                    if enabled {
                        let cc = (l.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0;
                        self.sampler.set_channel_pan(part, cc as i32);
                    }
                }
                self.sampler.set_part_mode(part, enabled);
```
6. **MidiSeqLoad** (mod.rs:2550–2579) — заменить цикл голосов (2561–2565) на:
```rust
                // Full per-part drain: closes the root_held/root_active gap
                // (ghost rekey on root_only patches after loading a new MIDI
                // file). The global all_notes_off above stays for drum/seq
                // channels not mapped to parts; the per-part CC123 is
                // redundant but idempotent.
                for (i, part) in self.parts.iter_mut().enumerate() {
                    part.drain_notes(&mut self.sampler, i, false);
                }
```
7. **MidiSeqPlay-stop** (mod.rs:2580–2598) — заменить цикл голосов (2592–2596) на тот же цикл с тем же комментарием (сокращённым до первой фразы про root_held/root_active).

- [ ] **Step 6: зелёный + полные регресс-гейты** —

```bash
cargo test --test root_only
cargo test --test sustain_audit
cargo test --lib
cargo test
cargo test --features rt-assert --test rt_safety
```
Ожидание: все зелёные, включая весь sustain_audit (TC7 — доказательство неизменности DSP-поведения: CC120-kill, CC123-release, mute-гейт, pedal-сценарии). `kill`-ветка helper'а покрыта `cc120_kills_sound_immediately`; suppress-семантика root_only — существующими тестами root_only.rs.

- [ ] **Step 7: коммит** —

```bash
git add src/synth/mod.rs src/synth/tests.rs tests/root_only.rs
git commit -m "fix: Part::drain_notes — AllNotesOff/MIDI-seq stop now clear root_held/root_active (ghost-rekey on root_only), dedupe 7 per-part cleanup sites"
```

**RT-safety note:** helper — plain field writes + `Voice::note_off/kill` + rustysynth CC-пуш; borrow `parts.iter_mut()` + `&mut self.sampler` — disjoint-field borrow, без аллокаций. Прогон rt_safety обязателен (Step 6).

---

# Группа C: QW6 — looper-атомы write-on-change

### Task 4: кэш последних значений + force-refresh

**Files:**
- Modify: `src/synth/looper.rs` (кэш-поля, `update_atoms`, `sync_atoms`, вызовы)

**Факты (anchor-verified):** `update_atoms` (looper.rs:577–585) — 4 atomic store + f64 деление — зовётся на каждом сэмпле из всех веток `tick` (:472 Idle, :486 Recording, :542 Playing). `state`/`len`/`current_layer` меняются только по control-границам; `position` — u8-квант (`playback_tick * 255 / loop_length_ticks`, после wrap-reset :528–542, поэтому wrap обрабатывается сравнением). `restore` публикует принудительно (:357–358), `clear` (:310–323) и `stop` (:282–288) — НЕТ (атомы протухают до следующего тика).

- [ ] **Step 1: failing-тесты** — в `#[cfg(test)] mod restore_tests` (looper.rs:589+, helper `snap` уже есть) добавить:

```rust
    #[test]
    fn clear_refreshes_atoms_immediately() {
        let mut l = MidiLooper::new(48_000.0);
        l.state = LooperState::Recording;
        l.record_event(60, 100, 0);
        l.tick(); // publishes event_count = 1
        assert_eq!(l.atoms.event_count.load(Ordering::Relaxed), 1);
        l.clear();
        assert_eq!(l.atoms.event_count.load(Ordering::Relaxed), 0,
            "clear must refresh atoms without waiting for a tick");
        assert_eq!(l.atoms.state.load(Ordering::Relaxed), 0);
        assert_eq!(l.atoms.layer_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stop_publishes_idle_state_without_tick() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[snap(0, 60, 127)], 48_000);
        l.toggle_play();
        l.tick();
        assert_eq!(l.atoms.state.load(Ordering::Relaxed), 2, "expected Playing");
        l.toggle_play(); // -> stop()
        assert_eq!(l.atoms.state.load(Ordering::Relaxed), 0,
            "stop must refresh the state atom immediately");
    }

    #[test]
    fn position_atom_walks_and_wraps() {
        // loop of 256 ticks: pos = playback_tick (max 254 — the u8 quant
        // (t*255/len) as u8 never reaches 255; existing behavior).
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[snap(0, 60, 127)], 256);
        l.toggle_play();
        let mut seen_254 = false;
        let mut saw_zero_after_254 = false;
        for _ in 0..600 {
            l.tick();
            let p = l.atoms.position.load(Ordering::Relaxed);
            if p == 254 { seen_254 = true; }
            if seen_254 && p == 0 { saw_zero_after_254 = true; break; }
        }
        assert!(seen_254, "position never reached 254");
        assert!(saw_zero_after_254, "position atom did not wrap back to 0");
    }
```

- [ ] **Step 2: красный** —

```bash
cargo test --lib clear_refreshes_atoms_immediately
cargo test --lib stop_publishes_idle_state_without_tick
cargo test --lib position_atom_walks_and_wraps
```
Ожидание: первые два FAIL (атомы протухают), wrap-тест может быть зелёным (guard).

- [ ] **Step 3: кэш** — в `MidiLooper` (looper.rs:145–169) после `atoms: Arc<LooperAtoms>,` добавить поле, в `new` (:172–195) — инициализацию:

```rust
    atoms: Arc<LooperAtoms>,
    /// Last values published to `atoms` (write-on-change: tick() calls
    /// update_atoms every sample). position is cached as u16 because every
    /// u8 value is legal — u16::MAX is the "never written" sentinel.
    atom_cache: (u8, u16, u16, u8),
```
```rust
            atoms: LooperAtoms::new(),
            atom_cache: (u8::MAX, u16::MAX, u16::MAX, u8::MAX),
```

- [ ] **Step 4: write-on-change + force-refresh** — заменить `update_atoms` (:577–585) на:

```rust
    /// Publishes looper state to the GUI atoms, storing only fields whose
    /// value actually changed. Called every sample from tick() — at 48 kHz
    /// the old version did ~48k atomic stores + an f64 divide per second even
    /// while Idle; the GUI reads at <= 60 Hz, so per-change publication is
    /// visually identical.
    fn update_atoms(&mut self) {
        let pos = if self.loop_length_ticks > 0 {
            (self.playback_tick as f64 / self.loop_length_ticks as f64 * 255.0) as u8
        } else { 0 };
        let next = (self.state.to_u8(), pos as u16, self.len as u16, self.current_layer + 1);
        if next.0 != self.atom_cache.0 {
            self.atoms.state.store(next.0, Ordering::Relaxed);
            self.atom_cache.0 = next.0;
        }
        if next.1 != self.atom_cache.1 {
            self.atoms.position.store(next.1 as u8, Ordering::Relaxed);
            self.atom_cache.1 = next.1;
        }
        if next.2 != self.atom_cache.2 {
            self.atoms.event_count.store(next.2, Ordering::Relaxed);
            self.atom_cache.2 = next.2;
        }
        if next.3 != self.atom_cache.3 {
            self.atoms.layer_count.store(next.3, Ordering::Relaxed);
            self.atom_cache.3 = next.3;
        }
    }

    /// Unconditionally re-publishes: invalidates the cache first. Required in
    /// restore/clear/stop, which mutate several fields at once and may run
    /// while tick() is not driving updates.
    fn sync_atoms(&mut self) {
        self.atom_cache = (u8::MAX, u16::MAX, u16::MAX, u8::MAX);
        self.update_atoms();
    }
```

- [ ] **Step 5: вызовы force-refresh** —
  - `restore` (:357–358): заменить пару `self.atoms.state.store(LooperState::Idle.to_u8(), Ordering::Relaxed); self.update_atoms();` на `self.sync_atoms();`.
  - `clear` (:310–323): после `self.update_display();` добавить `self.sync_atoms();`.
  - `stop` (:282–288): в конце добавить `self.sync_atoms();`.
  - (`undo`/`stop_record`/`toggle_play` не трогаем: следующий `tick()` подхватит изменения сравнением с кэшем — поведение идентично текущему.)

- [ ] **Step 6: зелёный + гейты** —

```bash
cargo test --lib
cargo test --features rt-assert --test rt_safety   # looper_control_events_do_not_allocate остаётся зелёным
cargo clippy --all-targets -- -D warnings
cargo test
```

- [ ] **Step 7: коммит** —

```bash
git add src/synth/looper.rs
git commit -m "perf: looper GUI atoms publish write-on-change (cached), force-refresh on restore/clear/stop"
```

**RT-safety note:** кэш — 4 plain-поля на `MidiLooper`; f64 деление выполняется по-прежнему раз/сэмпл (только когда длина > 0), но atomic store — только при изменении. Если захочется убрать и деление в Idle-ветке — отдельная задача, здесь не делать (риск рассинхрона позиции после смены bpm/длины).

---

# Группа D: QW9 — debounce config-save на слайдерах

### Task 5: sustain-release / SF2-rel → global_dirty

**Files:**
- Modify: `src/gui/mod.rs` (2 строки)

**Факты (anchor-verified):** `save_global_to_config()` зовётся на каждый changed-кадр: SR-слайдер (gui/mod.rs:913) и SF2 rel DragValue (:923). Каждый вызов = весь Config в pretty-JSON (drums_remaps, keybinds, pad_perf_map, …) → 2 аллокации + write+rename, до 60 раз/с при драге — заметно на Android FS. Оба поля уже сериализуются (:1505–1506); debounce `global_dirty` + 2 с (`:574–577`) уже существует и используется соседними слайдерами (:937, :951, :965, :980). Строка :905 — одноразовый клик SR-кнопки (не драг) — НЕ трогать. Существующий `global_dirty`-консьюмер обновляет `last_config_save` внутри `save_global_to_config` (:1515), так что непрерывный драг пишет не чаще раза в 2 с.

- [ ] **Step 1: замены** — в `src/gui/mod.rs`:
  - строка 913 (внутри `if ui.add(egui::Slider...).changed()` для SR): `self.save_global_to_config();` → `self.global_dirty = true;`
  - строка 923 (внутри changed-блока SF2 rel): `self.save_global_to_config();` → `self.global_dirty = true;`
  - Контрольный пуш `ControlEvent::SetSustainRelease` / `SetSf2ReleaseScale` в движок остаётся на каждый changed-кадр (дешёвый ring-push, отзывчивость не деградирует).

- [ ] **Step 2: гейты** —

```bash
cargo clippy --all-targets -- -D warnings
cargo test
```

- [ ] **Step 3: ручная проверка (TC11)** — `cargo run`:
  1. Панель F1: драгнуть SR-слайдер непрерывно ~3 с → `stat -c %y ~/.config/mini_midi_synth/config.json` изменился ≤ 1 раза; через ~2 с после отпускания — финальное значение в файле (`sustain_release`).
  2. SF2 rel: то же самое (`sf2_release_scale`).
  3. Рестарт приложения → оба значения перечитались.
  4. Соседние слайдеры (Tone/Reverb/Vol/PB) не деградировали (их debounce не менялся).

- [ ] **Step 4: коммит** —

```bash
git add src/gui/mod.rs
git commit -m "perf: sustain-release/SF2-rel sliders write config via global_dirty debounce instead of a full JSON save per frame"
```

**RT-safety note:** GUI-поток только; в аудио-зону не попадает. (Toast при ошибке `config.save()` на 17 молчаливых сайтах — вне скоупа этого плана.)

---

# Группа E: QW8 — dead code sweep (последней: трогает те же файлы, что Task 5)

### Task 6: удаления + устаревший allow

**Files:**
- Modify: `src/gui/mod.rs`, `src/lib.rs`, `src/preset.rs`, `src/synth/mod.rs`, `src/synth/mseg.rs`, `src/synth/mod_matrix.rs`, `src/synth/step_seq.rs`, `src/config.rs`

Все удаления верифицированы чтением кода (a4426fc). ПОПРАВКА к аудиту: `Config::save` — ЖИВОЙ (13 вызовов в gui/mod.rs + settings.rs), удаляется только `#[allow(dead_code)]`. Мёртвые `save_to_params` — три, аудит пропустил `step_seq.rs`; удаление `step_seq::save_to_params` осиротит `StepRate::index`/`ScaleType::index` (единственные вызовы — :338–339) — их тоже удалить.

- [ ] **Step 1: `_save_window_size` + `_frame_count`** —
  - `src/gui/mod.rs`: удалить метод `_save_window_size` (строки 2211–2272, вместе с fs::write в /tmp на :2231).
  - `src/gui/mod.rs:248`: удалить поле `pub _frame_count: u64,`.
  - `src/lib.rs:713`: удалить `_frame_count: 0,` из литерала App (литерал исчерпывающий — компилятор сам найдёт, если пропустите).

- [ ] **Step 2: пустая ветка + shown_cat** — `src/gui/mod.rs`, блок поиска пресетов:
  - строка 1234: удалить `let mut shown_cat = String::new();`.
  - строки 1236–1240 («Track category boundaries»): оставить инкремент `cat_idx`, убрать мёртвое присваивание:
```rust
                                if cat_idx < categories.len() && categories[cat_idx].1 == i {
                                    cat_idx += 1;
                                }
```
  (ВНИМАНИЕ: инкремент `cat_idx` load-bearing — от него зависит заголовок категорий на :1256; удалять весь блок нельзя.)
  - строки 1248–1251: удалить целиком мёртвую ветку (комментарий `// Show category header once per group` + `if shown_cat != ... { // We handle this below }`).

- [ ] **Step 3: placeholder-комментарии** — `src/preset.rs:500–512`: удалить 13 строк (`// Sequences (ported from Surge XT)` + 12 `// Surge XT: <Category>`) — следов портинга Surge в FACTORY_PRESETS нет.

- [ ] **Step 4: мёртвая пара `load_patch`** — `src/synth/mod.rs`:
  - удалить `Part::load_patch` (строки 577–589 вместе с `#[allow(dead_code)]`).
  - удалить `SynthEngine::load_patch` (строки 1665–1668 вместе с `#[allow(dead_code)]`).
  - Верификация: `grep -rn "load_patch" src/ tests/` — остаются только `load_patch_from` / `load_patch_from_edited` (живые пути, используются тестами и GUI).

- [ ] **Step 5: три мёртвых `save_to_params` + осиротевшие `index()`** —
  - `src/synth/mseg.rs`: удалить `Mseg::save_to_params` (с док-комментарием и `#[allow(dead_code)]`, ~262–281).
  - `src/synth/mod_matrix.rs`: удалить `ModMatrix::save_to_params` (~576–585). (Массивы `MOD_*_KEYS` остаются — их читает `load_from_params` :565–567.)
  - `src/synth/step_seq.rs`: удалить `StepSequencer::save_to_params` (~333–346); удалить `StepRate::index` (42–52) и `ScaleType::index` (103–114) — единственные вызовы были внутри удалённого fn, оба уже `#[allow(dead_code)]`.
  - (`looper::Quantize::index` тоже мёртв, но вне скоупа аудита QW8 — не трогать.)

- [ ] **Step 6: устаревший allow** — `src/config.rs:288`: удалить строку `#[allow(dead_code)]` над `pub fn save(&self)` (fn живой: 13 вызовов в gui/mod.rs + 4 в settings.rs).

- [ ] **Step 7: верификация (TC12)** —

```bash
grep -rn "_save_window_size\|_frame_count\|shown_cat" src/ | wc -l        # 0
grep -rn "save_to_params" src/ | wc -l                                    # 0
grep -rn "fn load_patch\b" src/ | wc -l                                   # 0 (load_patch_from не матчится с \b)
cargo clippy --all-targets -- -D warnings    # чисто — ни одной осиротевшей ссылки
cargo test                                   # 287+ тестов зелёные (удаления ничего не ломают)
cargo test --features rt-assert --test rt_safety
cargo deny check
```

- [ ] **Step 8: коммит** —

```bash
git add src/gui/mod.rs src/lib.rs src/preset.rs src/synth/mod.rs src/synth/mseg.rs src/synth/mod_matrix.rs src/synth/step_seq.rs src/config.rs
git commit -m "chore: dead code sweep — _save_window_size, empty search branch, Surge placeholder comments, dead load_patch pair, 3 dead save_to_params (+2 dead index), stale allow on live Config::save"
```

**RT-safety note:** все удаления вне аудио-горячего пути (`load_patch`-пара вызывалась бы из handle_control, но ноль вызовов; `save_to_params` — GUI/preset-сторона). rt-прогон в Step 7 — страховка.

---

## Финальные гейты (после всех тасков)

```bash
cargo clippy --all-targets -- -D warnings          # clean
cargo test                                         # all green
cargo test --features rt-assert --test rt_safety   # all green, no aborts (теперь 8 тестов)
cargo deny check                                   # clean (pre-existing yanked clipboard-win warning — warn-level, не блокер)
```

## Ручной чек-лист (после Task 5)

- [ ] SR-слайдер: драг 3 с → config.json меняется ≤ 1 раза / 2 с, финальное значение сохранено (TC11.1)
- [ ] SF2 rel: то же (TC11.2)
- [ ] Рестарт → значения перечитаны (TC11.3)
- [ ] Wavetable-пресет (built-in, без .wt): нота играется, первый note-on после старта не щёлкает/не лагает (прогрев в init_common)
- [ ] LFO-пресет (Sine, deform > 0.5): модуляция работает (LUT прогрета)
- [ ] Root-only-перф: паника (CC120 из GUI) → следующая нота звучит (не подавлена)
- [ ] MIDI-плеер: load → stop → игра на клавиатуре в root-only-части звучит нормально
- [ ] Looper-таймлайн в GUI: позиция/слой/события обновляются как раньше; Undo/Clear/Stop — мгновенно

## Известные риски

1. **Task 1 (QW1): скрытые аллокации на note-on пути.** Guard в TC1 впервые накрывает NoteOn — если post-fix red из-за ЧУЖОЙ аллокации (не wavetable), это новая находка того же класса, что и баг: чинить её в этом же таске или отдельным коммитом, НЕ ослаблять тест. Метод сужения — в Step 7.
2. **Task 1: `EMPTY_WAVETABLE` lookup на каждом reset.** `OnceLock::get` — atomic load; note-on и так делает несколько atomic ops (Arc refcount). Измеримого эффекта нет.
3. **Task 3 (QW3): расширение cleanup на MidiSeqLoad/MidiSeqPlay-stop/AllNotesOff** добавляет `part_all_notes_off` (per-part CC123) там, где был только глобальный вызов, и чистит `note_stack`/`latch_held` там, где не чистились. Для rustysynth повторный CC123 идемпотентен; sustain_audit — регресс-щит. Поведенческое следствие: после MIDI-stop не resurrect'ятся ни root-rekey, ни latch-ноты — это и есть цель.
4. **Task 3: borrow `parts.iter_mut()` + `&mut self.sampler`.** Disjoint-field borrow через прямой доступ к полям `self` — компилируется; если ed-версия компилятора вдруг запятнает — вынести `(let parts = &mut self.parts; let sampler = &mut self.sampler;)` перед циклом.
5. **Task 4 (QW6): позиция атома не достигает 255** (`(t*255/len) as u8` max 254) — существующее поведение, тест это документирует. GUI-индикатор не менялся.
6. **Task 5 (QW9): значение сохраняется с задержкой до 2 с** после отпускания слайдера — та же семантика, что у Tone/Reverb/Vol; если пользователь убьёт процесс в окне 2 с, потеряется последний тик драга (приемлемо, консистентно с соседями).
7. **Warmup и Android:** `init_common` — общий путь GUI/headless/Android, прогрев наследуется всеми тремя; `./android/build_rust.sh` + сборка не требуются (нет cfg-гейтов), но при желании прогнать `./android/build_rust.sh` на всякий случай после Task 1/2.
