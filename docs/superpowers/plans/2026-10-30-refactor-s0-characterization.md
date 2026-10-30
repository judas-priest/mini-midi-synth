# S0: Characterization-тесты — golden-signal harness + RT-assert coverage (REFACTOR_AUDIT S0)

anchor-checked: 2026-10-30 (все якоря сняты чтением кода на master @ a4426fc в этой сессии; номера строк REFACTOR_AUDIT.md могли дрейфовать — здесь актуальные)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Каждый таск исполняется СВЕЖИМ субагентом, получающим только этот документ.

**Характер плана: TEST-ONLY.** Ни одного изменения в prod-коде. Известно-красные тесты пинуются как `#[ignore]` с TODO на S1 (см. TC10/TC11) — сьют остаётся зелёным, баг документирован.

**Goal:** Закрыть пробелы покрытия S0 из REFACTOR_AUDIT.md ПЕРЕД любым рефакторингом (план блокирует S1–S5):
1. **Golden-signal harness** — бит-точный детерминизм + golden-хэши для всех FX-модулей (bonsai/neuron/combulator/resonator/exciter/reverb/chorus/spring_reverb/vocoder/nimbus/delay), для 16-слотового fx_chain на уровне движка и для всех фабричных пресетов.
2. **assert_no_alloc coverage** — note-on под guard'ом для каждого семейства осцилляторов, tick_block с активным FxChain, tick_block с LFO-deform, set_sample_rate (known-RED → S1), drum set_sample_rate (known-RED → S1).
3. **Looper timing** — `tick`/`record_event`/`quantize_tick`/`undo` (drum-seq undo покрыт существующими тестами `tests/drum_*`, это другой код).
4. **Voice stealing** — oldest-first и Poly-High/Low (`steal_prefers_releasing_voice` есть, остального нет).
5. **Perf-load последовательность** (engine-сторона) и **re-send контракт** `send_full_part_state` (reconnect).
6. **CI-пункт** — см. «Обязательные гейты» ниже.

## Разделение с планом QW-batch (Р-1)

План `docs/superpowers/plans/2026-10-30-refactor-qw-batch.md` (дальше — «QW-план») УЖЕ покрывает два RT-теста, которые аудит S0 упоминает. **Этот план их НЕ дублирует:**

| Покрытие | Где живёт |
|---|---|
| Wavetable note-on под guard'ом (`wavetable_note_on_does_not_allocate`, osc_type=27) | QW-план, Task 1 (TC1) |
| LFO sine LUT на уровне unit (`lfo_sine_lut_does_not_allocate`, `Lfo::tick_with_deform` + warmup) | QW-план, Task 2 (TC3) |

**Что этот план добавляет поверх (не пересекается):** engine-уровень `tick_block` с `lfo_deform >= 0.5` (другой сайт: part-LFO тикаются в `tick_block` — mod.rs:2758–2765, QW2-тест — unit-уровень `Lfo`), FxChain, все остальные семейства осцилляторов, `set_sample_rate`, drum `set_sample_rate`. Если QW-план ещё не выполнен, тесты `wavetable_note_on_does_not_allocate` и `lfo_sine_lut_does_not_allocate` в `tests/rt_safety.rs` будут ОТСУТСТВОВАТЬ — это норма, они придут с QW-планом; настоящий план их не добавляет.

## Architecture

- Аудиопоток: cpal callback → `rt_guard` + `no_denormals::no_denormals` (src/audio.rs:199) → `SynthEngine::tick_block` (src/synth/mod.rs:2692). Golden-рендер и guard-тесты повторяют ровно эту обёртку (`no_denormals` — обычная зависимость, Cargo.toml:41; `BLOCK_SIZE = 32`, mod.rs:382).
- **Детерминизм:** в движке нет wall-clock (grep `Instant|SystemTime` по `src/synth/` даёт единственное вхождение — `PatchParams::random_map()`, patch_params.rs:625, GUI-рандомизатор пресетов, в render-путь не входит). Все noise-RNG сидируются константами: oscillator.rs:397 (`0x12345678`), lfo.rs:182 (`0xCAFEBABE`), drum.rs:213/441 (`0xDEADBEEF`/`0xCAFEBABE`), oscillator.rs:633 (`0x42`), Part::new mod.rs:537 (`0x5851f42d4c957f2d`). Вывод: два свежих движка на одной машине/тулчейне дают **бит-идентичный** выход — это и пинуют TC1/TC2.
- **Golden-хэши платформо/тулчейн-специфичны** (float-codegen x86_64 vs aarch64 может отличаться). Контракт: golden-файл валиден для текущей платформы; любая непреднамеренная разница хэша = изменение звука = расследование; намеренная — регенерация `MINI_MIDI_GOLDEN_UPDATE=1` с осознанным коммитом. Тесты детерминизма (TC1/TC2) платформонезависимы и золот не требуют.
- FX-инстансы движка (mod.rs:1144–1177): 30+ синглтонов, тикаются из legacy-цепочки (mod.rs:3069–3186) и/или 16-слотового `apply_fx_slot` (mod.rs:3216–3344). Слоты едят `fx{i}_type/en/mix/p0..p3` ключи патча; `FxChain` становится active только при наличии ключа `fx0_type` (patch_params.rs:464). Параметры FX берутся из первой ВКЛЮЧЁННОЙ части (mod.rs:2911).
- Looper: `MidiLooper` (src/synth/looper.rs), поля `state/bpm/bars/quantize/loop_length_ticks` pub, внутренние (`events/len/playback_tick/playback_cursor/quantize_tick`) — приватные → юнит-тайминг-тесты живут в `#[cfg(test)]`-модуле самого looper.rs; engine-уровень — через `ControlEvent::LooperRestore` (mod.rs:232–235, возвращает `Some(spent)` — heap-payload) + `LooperTogglePlay`, старт строго с tick 0, что даёт детерминиримый timing.
- Note lifecycle: `Part::new` → `enabled: false` (mod.rs:528) — в engine-тестах часть надо включать `SetPartEnabled`. Полифония: `VOICES_PER_LAYER = 8` (mod.rs:381); steal-логика в `Part::note_on` (mod.rs:714–746): play_mode 4 = Poly-High (крадёт низшую ноту), 5 = Poly-Low (крадёт высшую), прочее — releasing-first → oldest (mod.rs:736–743). Существующий тест `steal_prefers_releasing_voice` (src/synth/tests.rs:897) — образец стиля.
- Reconnect: `reconnect_audio` (gui/settings.rs:186) пересоздаёт движок и зовёт `send_initial_presets`/`send_full_part_state`/`send_cc_map`/`send_drum_state` (settings.rs:320–323). `send_full_part_state` (gui/mod.rs:1714–1741) пушит фиксированную последовательность `SetPart*`-событий. `App` в тестах не собрать (`_audio_handle` требует живой cpal-стрим, lib.rs:884) → контракт re-send тестируется на уровне движка: replay последовательности на свежий движок должен восстанавливать part-состояние (TC23). Это зеркало в стиле `tests/ui_layout_contract.rs` (ручное зеркало с якорем).
- Perf-load: GUI `load_performance_by_name` (gui/mod.rs:2131–2204) меняет mirror-состояние и пушит ту же `SetPart*`-последовательность + tail-loop отключения отсутствующих частей (mod.rs-сторона: 2197–2202). Парсинг/миграция файлов — `preset::load_performance`/`migrate_performance_parts` (preset.rs:810–829) — полностью тестируемы headless.

**Tech Stack:** Rust 2021; существующие dev-deps: `tempfile` (Cargo.toml [dev-dependencies]), `serde_json`/`serde` ([dependencies]); `no_denormals` (Cargo.toml:41); `assert_no_alloc` за feature `rt-assert`. Новых зависимостей нет.

**Правила (CLAUDE.md, binding):** никаких аллокаций в аудио-callback; **`cargo fmt` по всей базе НЕ запускать**; тесты писать из требования, negative cases обязательны; ослабление существующего теста — только с явного согласия пользователя (маркер `test-weaken`).

## Обязательные гейты (CI-пункт из аудита — фиксируется здесь для ВСЕХ последующих планов)

**Под обычным `cargo test` suite `rt_safety` молча компилируется в 0 тестов** (feature-gate `#![cfg(feature = "rt-assert")]`, rt_safety.rs:8) — аудит прогнал его отдельно: 5 passed. CI-системы в проекте нет, поэтому гейт фиксируется процессуально: **каждый** план серии (S1–S5 и далее) обязан включать в свой список гейтов строку ниже; каждый исполнитель обязан прогонять её перед коммитом:

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety   # ОБЯЗАТЕЛЬНО, не опционально
cargo deny check
```

---

## Test Cases (from the requirement)

Требования записаны ДО реализации. Каждый нумерованный TC — отдельный тест. Ожидание для всех characterization-тестов: **зелёные сразу** (пинуют то, что ЕСТЬ), кроме явно помеченных known-RED (TC10/TC11 — `#[ignore]`).

1. **TC1** `fx_modules_render_bit_deterministic` — каждый из 11 FX-модулей (chorus, delay, reverb, spring_reverb, vocoder, nimbus, combulator, resonator, exciter, bonsai, neuron), 2 набора параметров, 4096 сэмплов xorshift-входа: два свежих инстанса → бит-идентичные хэши; выход не silent. (Пин детерминизма; golden-хэши отдельно в TC4.)
2. **TC2** `engine_preset_render_bit_deterministic` — подмножество фабричных пресетов (по ключевым словам epiano/piano/chorus/neuron/reverb/vocoder/nimbus/combulator/resonator/exciter/spring/bonsai/organ/fm/bell/saw, cap 16; fallback — первые 8): два рендера → одинаковый хэш.
3. **TC3** `golden_preset_hashes_match` — ВСЕ фабричные пресеты (`load_all_patches()`, 283 шт.): 0.25 c @48 kHz через `tick_block` под `no_denormals`, FNV-1a по битам f32 → сравнение с `tests/golden/preset_hashes.txt`. Регенерация: `MINI_MIDI_GOLDEN_UPDATE=1`.
4. **TC4** `golden_fx_module_hashes_match` — те же 11 модулей × 2 параметризации → `tests/golden/fx_module_hashes.txt`.
5. **TC5** `golden_engine_fx_chain_slots` — движок + init-патч со слотами fx0_type = Delay(13)/Chorus(7)/SpringReverb(17)/Nimbus(19)/Vocoder(26), mix 0.5, note-on, 0.25 c → `tests/golden/fx_chain_engine_hashes.txt`. Пин уровней «слот → движок» перед S3 (консолидация FX).
6. **TC6** `note_on_guarded_for_every_oscillator_family` — 29 семейств осцилляторов (osc_type 0..26, 28, 29; **27 = Wavetable исключён — покрывает QW-план**): под `assert_no_alloc` NoteOn + 4 блока + NoteOff + 8 блоков. Policy при RED — в Task 2 Step 4 (не чинить, изолировать в `#[ignore]`).
7. **TC7** `tick_block_with_active_fx_chain_does_not_allocate` — init-патч + `fx0_type=13` (Delay) + mix 0.5, NoteOn, 128 блоков под guard'ом.
8. **TC8** `tick_block_with_lfo_deform_does_not_allocate` — init-патч + `lfo_deform=0.8` (LUT-путь `apply_deform` для ЛЮБОЙ волны, lfo.rs:286), NoteOn, 64 блока под guard'ом. (Дополняет QW2-unit-тест, см. «Разделение».)
9. **TC9** `set_sample_rate_is_allocation_free` — **KNOWN-RED, `#[ignore]`**: `engine.set_sample_rate(96000)` под guard'ом. Сегодня реаллоцирует 30+ FX (`reverb.rs:298-300` `*self = Self::new` + vec!-delay-lines, chorus.rs:73, spring_reverb.rs:82–85) и делает save/restore-танец 13 полей в drum.rs:1054–1090. Снять ignore после S1.
10. **TC10** `drum_set_sample_rate_is_allocation_free` — **KNOWN-RED, `#[ignore]`**: `engine.drum_engine.set_sample_rate(96000)` под guard'ом. Сегодня `*self = Self::new(sr)` (drum.rs:1054–1090) аллоцирует 3 Arc-атома (drum.rs:1009–1011) + Vec голосов в callback'е. Снять ignore после S1.
11. **TC11** `quantize_grids_round_to_nearest` — Quarter/Eighth/Sixteenth/Off, округление к ближайшей линии сетки (120 BPM @ 48k: beat 24000; quarter 24000, eighth 12000, sixteenth 6000).
12. **TC12** `quantize_wraps_into_loop_length` — квантование у конца лупа модулится по `loop_length_ticks` (95000 → 0).
13. **TC13** `degenerate_grid_passes_tick_through` — grid == 0 (bpm 4 000 000) → сырой tick без деления на ноль (ветка looper.rs:219).
14. **TC14** `record_event_inserts_sorted_and_shifts_cursor` — сортированная вставка, два события на один tick, вставка до курсора сдвигает курсор.
15. **TC15** `record_event_caps_at_max_events` — `MAX_LOOP_EVENTS` (4096) cap, лишние дропаются (looper.rs:419).
16. **TC16** `playback_fires_events_at_exact_ticks_and_wraps` — события тикают ровно на своём tick; два на одном tick; wrap форсирует note-off для звучащих нот (looper.rs:528–537); рестарт с tick 0.
17. **TC17** `stop_drains_active_note_offs_on_next_tick` — `stop()` с активными нотами → `needs_note_offs` дренируется следующим tick'ом в Idle (looper.rs:460–471).
18. **TC18** `overdub_layers_and_undo_removes_only_top_layer` — overdub инкрементирует слой; undo снимает ровно верхний слой; undo на слое 0 — no-op.
19. **TC19** `undo_mid_pass_repartitions_cursor` — undo во время проигрывания пересчитывает `playback_cursor` (partition_point, looper.rs:304–305) → удалённый слой НЕ переигрывается, базовый — играет.
20. **TC20** `first_pass_recording_switches_to_overdub_at_loop_end` — первая запись по достижении конца лупа автоматически → Overdubbing, layer+1 (looper.rs:477–488).
21. **TC21** `looper_replays_events_at_recorded_sample_positions` — engine-уровень: `LooperRestore` (note-on tick 1000, note-off 2000, loop 24000) → `LooperTogglePlay` → звук начинается в блоке ~31 (± 2) в КАЖДОЙ из 3 пассов (сэмпл-точный loop-timing через `tick_block`).
22. **TC22** `poly_high_steals_the_lowest_note` — play_mode 4, 8 sustained-голосов → крадётся нота 36 (минимальная), 43 жива.
23. **TC23** `poly_low_steals_the_highest_note` — play_mode 5 → крадётся 43, 36 жива.
24. **TC24** `default_poly_steals_the_oldest_sustained_voice` — play_mode 0, без releasing → крадётся самый старый (36), новейший (43) жив. (Releasing-first уже покрыт `steal_prefers_releasing_voice`.)
25. **TC25** `part_control_events_land_in_engine_fields` — каждый `SetPart*`/`SetMacro` садится в ровно то поле движка (контракт, на котором держатся perf-load и re-send). Известные трансформации пинуются как есть: `pan` клампится в [-1,1] (mod.rs:2216), volume хранится raw (mod.rs:2198), out-of-range part — no-op.
26. **TC26** `full_state_re_send_restores_a_fresh_engine` — движок A (патч + нота + полный `SetPart*`-набор) vs свежий движок B, получивший ТОЛЬКО re-send-последовательность (зеркало `send_full_part_state`, gui/mod.rs:1714–1741): part-конфиг-поля совпадают побитово. Звучащее состояние (голоса) re-send НЕ переносит — пинуется комментарием.
27. **TC27** `absent_parts_are_disabled_by_tail_loop` — tail-loop perf-load (mod.rs-зеркало gui/mod.rs:2197–2202): включённые части за пределами перфа отключаются повторным `SetPartEnabled { enabled: false }`; идемпотентно при повторе.
28. **TC28** `legacy_8_part_performance_migrates_zone_b_to_8_12` — `load_performance` 8-частного файла → 12 частей: 0..4 = zone A, 4..8 пустые disabled, 8..12 = бывшая zone B (preset.rs:818–829); `split_point: None` сохраняется.
29. **TC29** `missing_optional_fields_get_defaults` — рукописный минимальный JSON без `vel_min/vel_max/mute/pan/transpose/sf2_*/root_only` → дефолты 1/127/false/0.0/0/false/0/false (serde-defaults preset.rs:762–787).
30. **TC30** `sixteen_part_performance_passes_through_unchanged` — 16-частный перф с `split_point: Some(60)` проходит миграцию без изменений.

## Negative Matrix

- **Missing field / null:** TC29 покрывает отсутствующие опциональные поля JSON; TC3/TC4/TC5 — отсутствие записи в golden-файле даёт fail с инструкцией регенерации (не тихий pass); отсутствие init-пресета — `expect` с понятным сообщением.
- **Zero/empty:** TC13 — grid==0 не делит на ноль; TC18/TC19 — undo на пустом/нулевом слое — no-op; TC15 — cap при переполнении; TC5-слоты с mix 0 не тикаются (но в тестах mix 0.5 — пин активного пути).
- **Duplicate/concurrent:** TC1/TC2 — два независимых инстанса = бит-идентичный выход (то же требование, что «дубликат вызова»); TC27 — повторное отключение уже отключённой части идемпотентно; TC25 — повтор `SetPart*` переписывает поле тем же значением.
- **Clock skew между независимыми часами:** wall-clock в движке отсутствует (проверено grep'ом; единственный `SystemTime` — `random_map`, вне render-пути) — тайминги TC11–TC21 выражены только в сэмплах одного счётчика.
- **External-call timeout:** неприменимо (тесты без I/O кроме чтения/записи golden-файлов вне RT-зоны).
- **Extreme values:** TC13 (bpm 4e6), TC15 (cap 4096), TC22–TC24 (все 8 голосов заняты), TC25 (part 99 — no-op), TC12 (tick у самой границы лупа).
- **Платформенный дрейф:** golden-файлы валидны per-platform/toolchain; TC1/TC2 не зависят от платформы. Кросс-платформенная регенерация — осознанный коммит golden-файлов (aarch64 Orange Pi 5 — отдельная регенерация при необходимости).

---

### Task 0: Ветка

- [ ] Из master:
```bash
cd /home/dima/Projects/mini_midi_synth
git checkout master && git pull
git checkout -b refactor-s0-characterization
```

---

# Группа A: Golden-signal harness

### Task 1: tests/golden_fx.rs + tests/golden/*

**Files:**
- Create: `tests/golden_fx.rs`
- Create (генерируются Step 3, коммитятся): `tests/golden/preset_hashes.txt`, `tests/golden/fx_module_hashes.txt`, `tests/golden/fx_chain_engine_hashes.txt`

Проверенные сигнатуры тиков (для кода ниже): `chorus::Chorus::new(sr)`, `tick(input: f32, mix: f32) -> (f32, f32)` (chorus.rs:23,36); `delay::StereoDelay::new(sr)`, `tick(in_l, in_r, time_l, time_r, feedback, filter, ping_pong: bool, mix)` (delay.rs:21,51); `reverb::Reverb::new(sr)`, `tick(in_l, in_r, room_size, damping, width, pre_delay, mix)` (reverb.rs:260,305); `spring_reverb::SpringReverb::new(sr)`, `tick(in_l, in_r, size, decay, reflections, damping, spin, _chaos, mix)` (spring_reverb.rs:59,121); `vocoder::Vocoder::new(sr)`, `tick(car_l, car_r, mod_in, env_follow, gate, mix)` (vocoder.rs:124,166); `nimbus::Nimbus::new(sr)`, `tick(in_l, in_r, position, size, pitch, density, spread, texture, mix)` (nimbus.rs:61,188); `combulator::Combulator::new(sr)`, `tick(in_l, in_r, freq, offset2, offset3, feedback, tone, mix)` (combulator.rs:83,108); `resonator::Resonator::new(sr)`, `tick(input, mix) -> (f32, f32)` (resonator.rs:27,62); `exciter::Exciter::new(sr)`, `tick(in_l, in_r, drive, freq, presence, mix)` (exciter.rs:38,138); `bonsai::Bonsai::new(sr)`, `tick(in_l, in_r, drive, tone, asym, mode: BonsaiMode, mix)`, `BonsaiMode::from_param(f32)` (bonsai.rs:38,58); `neuron::Neuron::new(sr)`, `tick(in_l, in_r, drive, squash, stab, asym, bias, comb_freq, comb_sep, mix)` (neuron.rs:22,85). Все модули — `pub mod` (synth/mod.rs:6–44).

- [ ] **Step 1: файл** — создать `tests/golden_fx.rs`:

```rust
//! Golden-signal characterization harness (REFACTOR_AUDIT S0).
//! Pins bit-exact deterministic output of FX modules, the 16-slot fx_chain
//! (engine level) and every factory preset.
//!
//! Run:        cargo test --test golden_fx
//! Regenerate: MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx
//!             (commit the regenerated tests/golden/*.txt deliberately —
//!              a golden change IS an audible-behavior change)
//!
//! Goldens are platform/toolchain specific (float codegen differs between
//! x86_64 and aarch64). The determinism tests below are platform-independent.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{
    bonsai, chorus, combulator, delay, exciter, neuron, nimbus, resonator, reverb,
    spring_reverb, vocoder, ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE,
};

const SR: f32 = 48_000.0;
/// 0.25 s per preset — matches the runtime budget of all_presets_no_nan_no_silence.
const BLOCKS_PER_RENDER: usize = 48_000 / BLOCK_SIZE / 4; // 375
const FX_SAMPLES: usize = 4096;

// ── hashing ──────────────────────────────────────────────────────────────────

fn fnv1a_update(h: &mut u64, sample: f32) {
    for b in sample.to_bits().to_le_bytes() {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

/// Deterministic xorshift32 input signal (fixed seed — no wall-clock).
fn xorshift_input(seed: u32) -> impl Iterator<Item = f32> {
    let mut st = seed;
    std::iter::from_fn(move || {
        st ^= st << 13;
        st ^= st >> 17;
        st ^= st << 5;
        Some((st as f32 / u32::MAX as f32) * 2.0 - 1.0)
    })
}

// ── golden files ─────────────────────────────────────────────────────────────

fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

/// Compares `entries` against tests/golden/<file>, or (re)writes the file
/// when MINI_MIDI_GOLDEN_UPDATE is set.
fn check_golden(file: &str, entries: &[(String, u64)]) {
    let path = golden_path(file);
    if std::env::var("MINI_MIDI_GOLDEN_UPDATE").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut body = String::new();
        for (k, v) in entries {
            body.push_str(&format!("{k}\t{v:016x}\n"));
        }
        std::fs::write(&path, body).unwrap();
        eprintln!("goldens written: {}", path.display());
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "golden file {} missing ({e}); run MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx",
            path.display()
        )
    });
    let map: std::collections::HashMap<&str, u64> = text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('\t')?;
            Some((k, u64::from_str_radix(v, 16).ok()?))
        })
        .collect();
    let mut bad = Vec::new();
    for (k, v) in entries {
        match map.get(k.as_str()) {
            Some(g) if *g == *v => {}
            Some(g) => bad.push(format!("{k}: golden {g:016x} != actual {v:016x}")),
            None => bad.push(format!("{k}: no golden entry (regenerate goldens)")),
        }
    }
    assert!(
        bad.is_empty(),
        "golden mismatches — behavior changed. Intentional? Regenerate with \
         MINI_MIDI_GOLDEN_UPDATE=1 and commit deliberately:\n{}",
        bad.join("\n")
    );
}

// ── FX module table ──────────────────────────────────────────────────────────

type StereoFx = Box<dyn FnMut(f32, f32) -> (f32, f32)>;

/// The FX families named by the audit S0 gap list. Two parameter sets each.
fn fx_modules() -> Vec<(&'static str, Vec<StereoFx>)> {
    vec![
        ("chorus", vec![
            Box::new({ let mut m = chorus::Chorus::new(SR); move |l, _r| m.tick(l, 0.5) }),
            Box::new({ let mut m = chorus::Chorus::new(SR); move |l, _r| m.tick(l * 0.5 + 0.1, 0.9) }),
        ]),
        ("delay", vec![
            Box::new({ let mut m = delay::StereoDelay::new(SR); move |l, r| m.tick(l, r, 0.3, 0.4, 0.4, 0.5, false, 0.4) }),
            Box::new({ let mut m = delay::StereoDelay::new(SR); move |l, r| m.tick(l, r, 0.6, 0.2, 0.7, 0.3, true, 0.8) }),
        ]),
        ("reverb", vec![
            Box::new({ let mut m = reverb::Reverb::new(SR); move |l, r| m.tick(l, r, 0.6, 0.4, 0.8, 0.1, 0.4) }),
            Box::new({ let mut m = reverb::Reverb::new(SR); move |l, r| m.tick(l, r, 0.9, 0.2, 1.0, 0.3, 0.9) }),
        ]),
        ("spring_reverb", vec![
            Box::new({ let mut m = spring_reverb::SpringReverb::new(SR); move |l, r| m.tick(l, r, 0.7, 0.6, 0.5, 0.3, 0.2, 0.0, 0.4) }),
            Box::new({ let mut m = spring_reverb::SpringReverb::new(SR); move |l, r| m.tick(l, r, 0.3, 0.9, 0.8, 0.1, 0.8, 0.5, 0.7) }),
        ]),
        ("vocoder", vec![
            Box::new({ let mut m = vocoder::Vocoder::new(SR); move |l, r| m.tick(l, r, (l + r) * 0.5, 0.5, 0.1, 0.6) }),
            Box::new({ let mut m = vocoder::Vocoder::new(SR); move |l, r| m.tick(l, r, l.abs(), 0.9, 0.4, 0.9) }),
        ]),
        ("nimbus", vec![
            Box::new({ let mut m = nimbus::Nimbus::new(SR); move |l, r| m.tick(l, r, 0.3, 0.4, 0.5, 0.5, 0.3, 0.2, 0.5) }),
            Box::new({ let mut m = nimbus::Nimbus::new(SR); move |l, r| m.tick(l, r, 0.8, 0.9, 0.2, 0.9, 0.7, 0.6, 0.9) }),
        ]),
        ("combulator", vec![
            Box::new({ let mut m = combulator::Combulator::new(SR); move |l, r| m.tick(l, r, 0.4, 0.5, 0.5, 0.3, 0.6, 0.5) }),
            Box::new({ let mut m = combulator::Combulator::new(SR); move |l, r| m.tick(l, r, 0.8, 0.2, 0.7, 0.8, 0.3, 0.9) }),
        ]),
        ("resonator", vec![
            Box::new({ let mut m = resonator::Resonator::new(SR); move |l, r| m.tick((l + r) * 0.5, 0.6) }),
            Box::new({ let mut m = resonator::Resonator::new(SR); move |l, r| m.tick((l - r) * 0.5, 0.95) }),
        ]),
        ("exciter", vec![
            Box::new({ let mut m = exciter::Exciter::new(SR); move |l, r| m.tick(l, r, 0.5, 0.5, 0.4, 0.4) }),
            Box::new({ let mut m = exciter::Exciter::new(SR); move |l, r| m.tick(l, r, 0.9, 0.8, 0.9, 0.8) }),
        ]),
        ("bonsai", vec![
            Box::new({ let mut m = bonsai::Bonsai::new(SR); move |l, r| m.tick(l, r, 0.5, 0.5, 0.5, bonsai::BonsaiMode::from_param(0.3), 0.5) }),
            Box::new({ let mut m = bonsai::Bonsai::new(SR); move |l, r| m.tick(l, r, 0.9, 0.2, 0.8, bonsai::BonsaiMode::from_param(0.8), 0.9) }),
        ]),
        ("neuron", vec![
            Box::new({ let mut m = neuron::Neuron::new(SR); move |l, r| m.tick(l, r, 0.5, 0.4, 0.3, 0.0, 0.5, 0.6, 0.5, 0.4) }),
            Box::new({ let mut m = neuron::Neuron::new(SR); move |l, r| m.tick(l, r, 0.9, 0.8, 0.7, 0.2, 0.1, 0.3, 0.9, 0.8) }),
        ]),
    ]
}

fn render_fx(fx: &mut StereoFx) -> (u64, f32) {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut peak = 0.0_f32;
    for s in xorshift_input(0x1234567).take(FX_SAMPLES) {
        let (l, r) = fx(s, s * 0.7);
        fnv1a_update(&mut h, l);
        fnv1a_update(&mut h, r);
        peak = peak.max(l.abs()).max(r.abs());
    }
    (h, peak)
}

// ── TC1: determinism (platform-independent) ──────────────────────────────────

#[test]
fn fx_modules_render_bit_deterministic() {
    for (name, mut variants) in fx_modules() {
        for (set, fx) in variants.iter_mut().enumerate() {
            let (a, peak_a) = render_fx(fx);
            let (b, _) = render_fx(fx);
            assert_eq!(a, b, "fx '{name}' set {set}: not bit-deterministic across two runs");
            assert!(peak_a > 1e-4, "fx '{name}' set {set}: output silent (peak {peak_a})");
            assert!(peak_a.is_finite(), "fx '{name}' set {set}: non-finite output");
        }
    }
}

// ── TC4: fx module goldens ───────────────────────────────────────────────────

#[test]
fn golden_fx_module_hashes_match() {
    let mut entries = Vec::new();
    for (name, mut variants) in fx_modules() {
        for (set, fx) in variants.iter_mut().enumerate() {
            let (h, _) = render_fx(fx);
            entries.push((format!("{name}_set{set}"), h));
        }
    }
    check_golden("fx_module_hashes.txt", &entries);
}

// ── engine helpers ───────────────────────────────────────────────────────────

fn load_init_patch(engine: &mut SynthEngine, patch: &preset::Patch) {
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = engine.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
}

/// Renders 0.25 s of a preset through tick_block under no_denormals (the
/// production wrapper, audio.rs:199) and returns the FNV-1a hash of the
/// interleaved output bits.
fn render_preset_hash(patch: &preset::Patch) -> u64 {
    let mut engine = SynthEngine::new(SR);
    engine.set_patches(vec![patch.clone()]);
    load_init_patch(&mut engine, patch);
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

    let mut h: u64 = 0xcbf29ce484222325;
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    no_denormals::no_denormals(|| {
        for _ in 0..BLOCKS_PER_RENDER {
            engine.tick_block(&mut l, &mut r);
            for i in 0..BLOCK_SIZE {
                fnv1a_update(&mut h, l[i]);
                fnv1a_update(&mut h, r[i]);
                assert!(l[i].is_finite() && r[i].is_finite(), "non-finite output");
            }
        }
    });
    h
}

// ── TC2: engine determinism (platform-independent) ───────────────────────────

#[test]
fn engine_preset_render_bit_deterministic() {
    let patches = preset::load_all_patches();
    let keys = ["epiano", "piano", "chorus", "neuron", "reverb", "vocoder", "nimbus",
        "combulator", "resonator", "exciter", "spring", "bonsai", "organ", "fm", "bell", "saw"];
    let mut subset: Vec<&preset::Patch> = patches
        .iter()
        .filter(|p| keys.iter().any(|k| p.name.to_lowercase().contains(k)))
        .take(16)
        .collect();
    if subset.is_empty() {
        subset = patches.iter().take(8).collect();
    }
    assert!(!subset.is_empty(), "no factory presets compiled in");
    for p in &subset {
        let a = render_preset_hash(p);
        let b = render_preset_hash(p);
        assert_eq!(a, b, "preset '{}' not bit-deterministic across two runs", p.name);
    }
}

// ── TC3: golden hash for EVERY factory preset ────────────────────────────────

#[test]
fn golden_preset_hashes_match() {
    let patches = preset::load_all_patches();
    let mut entries = Vec::new();
    for p in &patches {
        entries.push((p.name.clone(), render_preset_hash(p)));
    }
    check_golden("preset_hashes.txt", &entries);
}

// ── TC5: engine-level 16-slot fx_chain goldens ───────────────────────────────

/// (slot type number, FxSlotType name) — fx_chain.rs:11-51 (repr u8).
const CHAIN_SLOTS: &[(f32, &str)] = &[
    (13.0, "Delay"), (7.0, "Chorus"), (17.0, "SpringReverb"),
    (19.0, "Nimbus"), (26.0, "Vocoder"),
];

#[test]
fn golden_engine_fx_chain_slots() {
    let base = preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists");
    let mut entries = Vec::new();
    for (ty, name) in CHAIN_SLOTS {
        let mut patch = base.clone();
        // FxChain becomes active iff the fx0_type key exists (patch_params.rs:464)
        patch.params.insert("fx0_type".into(), *ty);
        patch.params.insert("fx0_en".into(), 1.0);
        patch.params.insert("fx0_mix".into(), 0.5);
        let h = render_preset_hash(&patch);
        entries.push((format!("fx_chain_{name}"), h));
    }
    check_golden("fx_chain_engine_hashes.txt", &entries);
}
```

- [ ] **Step 2: красный (expected)** —
```bash
cargo test --test golden_fx
# TC3/TC4/TC5 fail: golden file missing; TC1/TC2 green (детерминизм уже есть — это пин)
```
Если TC1/TC2 RED на этом шаге — движок недетерминимен (не должно: все RNG сидированы константами, wall-clock отсутствует). Остановиться и расследовать — это блокер для всей серии golden-тестов, НЕ глушить assert.

- [ ] **Step 3: регенерация golden-файлов** —
```bash
MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx
cargo test --test golden_fx          # теперь все зелёные
ls -la tests/golden/                 # 3 файла
```
Оценка времени: ~283 рендера × 0.25 c tick_block ≈ 2–4 мин на прогон. Если дольше 10 мин — уменьшить `BLOCKS_PER_RENDER` до 188 (0.125 c) ОДИН раз в этом таске и перегенерировать (золот должен пинуть атаку+стабильную фазу, достаточно).

- [ ] **Step 4: гейты + коммит** —
```bash
cargo clippy --all-targets -- -D warnings
cargo test --test golden_fx
git add tests/golden_fx.rs tests/golden/
git commit -m "test: golden-signal harness — bit-determinism + golden hashes for FX modules, 16-slot fx_chain (engine) and all factory presets (audit S0)"
```

**RT-safety note:** только тесты; golden I/O вне аудио-зоны.

---

# Группа B: assert_no_alloc coverage (Task 2 — tests/rt_safety.rs)

### Task 2: guarded note-on per osc family, FxChain, LFO-deform, set_sample_rate

**Files:**
- Modify: `tests/rt_safety.rs`

**Факты (anchor-verified):** guard-сьют сейчас — 5 тестов; `tick_block_does_not_allocate` держит NoteOn ВНЕ guard'а (rt_safety.rs:22) — именно этот пробел позволил wavetable-багу жить (чинит QW-план, не мы). Осц-семейства: `OscType::from_param` oscillator.rs:95–126 (0..29; 27 = Wavetable → QW-план). LFO-deform: `apply_deform` читает sine LUT при `deform >= 0.5` для любой волны (lfo.rs:286); part-LFO тикаются в `tick_block` (mod.rs:2758–2765); ключи патча `lfo_deform`/`lfo_waveform` (mod.rs:1092, patch_params.rs). FxChain active — по наличию `fx0_type` (patch_params.rs:464). `set_sample_rate` движка зовётся из callback (audio.rs:199–205) под `rt_guard` — сегодня реаллоцирует FX и drum (known-RED, S1). `drum_engine` и `set_sample_rate` — pub (mod.rs:1133, 2647).

- [ ] **Step 1: тесты** — добавить в конец `tests/rt_safety.rs`:

```rust
// ── S0 coverage expansion (audit REFACTOR_AUDIT S0) ─────────────────────────

fn init_patch_with(mutations: &[(&str, f32)]) -> preset::Patch {
    let mut patch = preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists");
    for (k, v) in mutations {
        patch.params.insert(k.to_string(), *v);
    }
    patch
}

fn guarded_note_cycle(engine: &mut SynthEngine) {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        for _ in 0..4 { engine.tick_block(&mut l, &mut r); }
        engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
        for _ in 0..8 { engine.tick_block(&mut l, &mut r); }
    });
}

/// RT-guarded note-on for EVERY oscillator family. The existing suite kept
/// NoteOn OUTSIDE the guard (rt_safety.rs tick_block_does_not_allocate) — the
/// exact coverage gap that let the wavetable per-note-on alloc live.
/// OscType 27 (Wavetable) is covered by the QW-batch plan
/// (wavetable_note_on_does_not_allocate) and intentionally excluded here.
#[test]
fn note_on_guarded_for_every_oscillator_family() {
    // (osc_type value, family name) — OscType::from_param, oscillator.rs:95-126
    const FAMILIES: &[(f32, &str)] = &[
        (0.0, "Sine"), (1.0, "Saw"), (2.0, "Square"), (3.0, "Triangle"),
        (4.0, "Fm"), (5.0, "Noise"), (6.0, "KarplusStrong"), (7.0, "Organ"),
        (8.0, "FmPiano"), (9.0, "CommutedPiano"), (10.0, "BandedWG"),
        (11.0, "AdditivePiano"), (12.0, "DrumSynth"), (13.0, "BassGuitar"),
        (14.0, "BowedString"), (15.0, "Brass"), (16.0, "PhaseDistortion"),
        (17.0, "Wavefolder"), (18.0, "ModalResonator"), (19.0, "HardSync"),
        (20.0, "Supersaw"), (21.0, "PianoModel"), (22.0, "Accordion"),
        (23.0, "Saxophone"), (24.0, "ElectricPiano"), (25.0, "Alias"),
        (26.0, "Window"), (28.0, "Fm3"), (29.0, "Twist"),
    ];
    for (osc, name) in FAMILIES {
        let patch = init_patch_with(&[("osc_type", *osc)]);
        let mut engine = SynthEngine::new(SR);
        let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
        drop(spent);
        let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
        guarded_note_cycle(&mut engine); // aborts (process dies) on any alloc
    }
}

/// Template for isolating a family that turns out to allocate today:
/// remove its entry from FAMILIES above and add an ignored test like this
/// with a TODO referencing the new finding. Do NOT fix prod code here
/// (this plan is test-only) and do NOT weaken the main test.
//
// #[test]
// #[ignore = "KNOWN RED (finding YYYY-MM-DD): <family> note-on allocates <what> \
//             (file:line). TODO(<plan/issue>): fix, then un-ignore."]
// fn family_X_note_on_does_not_allocate() {
//     let patch = init_patch_with(&[("osc_type", X.0)]);
//     let mut engine = SynthEngine::new(SR);
//     let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
//     drop(spent);
//     let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
//     guarded_note_cycle(&mut engine);
// }

/// 16-slot fx_chain: slot params are read per block, slot DSP instances are
/// preallocated — tick_block with an active chain must stay allocation-free.
#[test]
fn tick_block_with_active_fx_chain_does_not_allocate() {
    let patch = init_patch_with(&[("fx0_type", 13.0), ("fx0_en", 1.0), ("fx0_mix", 0.5)]);
    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        for _ in 0..128 { engine.tick_block(&mut l, &mut r); }
    });
}

/// Engine-level LFO site: part LFOs tick in tick_block, and apply_deform
/// routes ANY waveform through the sine LUT at deform >= 0.5 (lfo.rs:286).
/// Distinct from the QW-batch unit test (Lfo::tick_with_deform + warmup).
#[test]
fn tick_block_with_lfo_deform_does_not_allocate() {
    let patch = init_patch_with(&[("lfo_deform", 0.8), ("lfo_waveform", 0.0)]);
    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        for _ in 0..64 { engine.tick_block(&mut l, &mut r); }
    });
}

/// KNOWN RED (REFACTOR_AUDIT S1): set_sample_rate runs in the audio callback
/// (audio.rs:199-205 reacts to the jack_sr atom) but reallocates 30+ FX
/// (reverb.rs:298-300 `*self = Self::new` + vec! delay lines, chorus.rs:73,
/// spring_reverb.rs:82-85) and re-runs the drum save/restore dance
/// (drum.rs:1054-1090). Un-ignore after the S1 fix makes it allocation-free.
#[test]
#[ignore = "KNOWN RED (audit S1): set_sample_rate reallocates FX in the callback"]
fn set_sample_rate_is_allocation_free() {
    let mut engine = SynthEngine::new(SR);
    assert_no_alloc(|| engine.set_sample_rate(96_000.0));
}

/// KNOWN RED (audit S1): DrumEngine::set_sample_rate does
/// `*self = Self::new(sr)` (drum.rs:1054-1090), allocating the 3 Arc atoms
/// (drum.rs:1009-1011) plus voice Vecs inside the callback.
#[test]
#[ignore = "KNOWN RED (audit S1): drum set_sample_rate rebuilds via Self::new (3 Arc allocs, drum.rs:1009-1011)"]
fn drum_set_sample_rate_is_allocation_free() {
    let mut engine = SynthEngine::new(SR);
    assert_no_alloc(|| engine.drum_engine.set_sample_rate(96_000.0));
}
```

- [ ] **Step 2: прогон** —
```bash
cargo test --features rt-assert --test rt_safety
```
Ожидание: все новые зелёные, кроме двух `#[ignore]`; старые 5 не тронуты.

- [ ] **Step 3: known-RED верификация** — подтвердить, что игнорируемые тесты действительно красные (аборт при аллокации), по одному в изолированном процессе:
```bash
cargo test --features rt-assert --test rt_safety set_sample_rate_is_allocation_free -- --ignored
# EXPECT: process abort (memory allocation) — это и есть документируемый баг S1
cargo test --features rt-assert --test rt_safety drum_set_sample_rate_is_allocation_free -- --ignored
# EXPECT: process abort
```
Если какой-то из них ВДРУГ зелёный — убрать `#[ignore]` (пин стал бы обычным guard-тестом) и отметить в коммит-сообщении.

- [ ] **Step 4: policy при RED в TC6 (osc families).** Если какой-то семейство абортит процесс при прогоне Step 2 — это НОВАЯ находка того же класса, что wavetable-баг. Процедура: (1) запустить по одному семейству, чтобы найти виновника (`cargo test --features rt-assert --test rt_safety note_on_guarded -- --exact` не параметризуешь — временно сузить таблицу FAMILIES локально, НЕ коммитить сужение); (2) исключить семейство из `FAMILIES`, добавить `#[ignore]`-тест по шаблону из Step 1 с якорем `file:line` аллоцирующего вызова; (3) НЕ чинить prod-код; (4) перечислить находки в финальном отчёте плана — они идут в backlog S1/quick-wins. Если RED не в note-on, а в самих блоках (`tick_block`) с этим семейством — та же процедура, сообщение ignore-теста отражает сайт.

- [ ] **Step 5: гейты + коммит** —
```bash
cargo test --features rt-assert --test rt_safety   # зелёный (ignored не считаются)
cargo clippy --all-targets -- -D warnings
cargo test
git add tests/rt_safety.rs
git commit -m "test: rt-assert coverage — guarded note-on for every osc family, active fx_chain and lfo-deform tick_block; pin known-RED set_sample_rate allocs as ignored (audit S0/S1)"
```

**RT-safety note:** guard-тесты выполняются в тестовом процессе с assert_no_alloc-аллокатором библиотеки (файл не объявляет свой `#[global_allocator]` — см. шапку rt_safety.rs).

---

# Группа C: Looper timing (Task 3)

### Task 3: looper.rs `#[cfg(test)] mod timing_tests` + tests/looper_timing.rs

**Files:**
- Modify: `src/synth/looper.rs` (новый `#[cfg(test)] mod timing_tests` рядом с существующим `restore_tests`, looper.rs:589–635)
- Create: `tests/looper_timing.rs`

**Факты (anchor-verified):** `quantize_tick` (looper.rs:215–229): `nearest = ((tick + grid/2)/grid)*grid`, `% loop_length_ticks` при длине > 0, `grid == 0` → сырой tick. `record_event` (415–449): sorted insert через `partition_point` + `copy_within`, cap `MAX_LOOP_EVENTS` (419), cursor сдвигается при `pos <= cursor && cursor < len` (445–447). `tick` (454–544): Idle-ветка дренит `needs_note_offs` (460–471); Recording не играет (477–488), на конце лупа → Overdubbing + layer+1; Playing/Overdub — события при `event.tick == playback_tick` (505–523), mute/solo-гейт (512), wrap форсирует note-offs (528–537). `stop` (282–288) ставит `needs_note_offs`; drain в Idle-ветке. `undo` (291–307): удаляет layer == current_layer, `current_layer -= 1`, cursor = partition_point (304–305), layer 0 — no-op (292). `restore` (332–360) сбрасывает playback в 0 и ставит current_layer = max(layer_id). Engine: `ControlEvent::LooperRestore` (mod.rs:232–235) возвращает `Some(spent)` (heap-payload — `drop(spent)` обязателен), `LooperTogglePlay` из Idle с `len > 0` стартует с tick 0 (looper.rs:273–277). `MAX_SIMULTANEOUS = 16`, отсутствие события — `(0, 0, 255)`.

- [ ] **Step 1: юнит-модуль** — в `src/synth/looper.rs` после `mod restore_tests` добавить:

```rust
#[cfg(test)]
mod timing_tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Looper with an established loop length (bars x 4 beats), in Recording.
    fn loop_of(bpm: f32, bars: u8) -> MidiLooper {
        let mut l = MidiLooper::new(SR);
        l.bpm = bpm;
        l.bars = bars;
        l.start_record(); // Idle -> Recording, loop_length_ticks established
        l
    }

    #[test]
    fn quantize_grids_round_to_nearest() {
        // 120 BPM @ 48k: beat = 24000 samples; quarter 24000, eighth 12000, 16th 6000
        let mut l = MidiLooper::new(SR);
        l.bpm = 120.0;
        l.quantize = Quantize::Eighth;
        assert_eq!(l.quantize_tick(1000), 0, "far below half-grid -> snaps to 0");
        assert_eq!(l.quantize_tick(5000), 0);
        assert_eq!(l.quantize_tick(7000), 12000, "past half-grid -> snaps up");
        l.quantize = Quantize::Sixteenth;
        assert_eq!(l.quantize_tick(4000), 6000);
        l.quantize = Quantize::Quarter;
        assert_eq!(l.quantize_tick(20000), 24000);
        l.quantize = Quantize::Off;
        assert_eq!(l.quantize_tick(12345), 12345, "Off passes through");
    }

    #[test]
    fn quantize_wraps_into_loop_length() {
        // 120 BPM, 1 bar = 96000 ticks; quarter grid 24000.
        // tick 95000: nearest = ((95000+12000)/24000)*24000 = 96000 -> % len -> 0
        let mut l = loop_of(120.0, 1);
        assert_eq!(l.loop_length_ticks, 96_000);
        l.quantize = Quantize::Quarter;
        assert_eq!(l.quantize_tick(95_000), 0);
    }

    #[test]
    fn degenerate_grid_passes_tick_through() {
        // grid == 0 (absurd bpm) must not divide by zero (looper.rs:219)
        let mut l = MidiLooper::new(SR);
        l.bpm = 4_000_000.0; // beat_samples = 0.72 -> grid 0
        l.quantize = Quantize::Sixteenth;
        assert_eq!(l.quantize_tick(777), 777);
    }

    #[test]
    fn record_event_inserts_sorted_and_shifts_cursor() {
        let mut l = loop_of(120.0, 1);
        l.state = LooperState::Recording;
        l.playback_tick = 24_000;
        l.playback_cursor = 1;
        l.record_event(60, 127, 0); // tick 24000 (quantize Off)
        l.record_event(62, 127, 0); // same tick -> inserted after
        l.playback_tick = 12_000;
        l.playback_cursor = 2;
        l.record_event(55, 100, 0); // earlier tick -> inserted at 0
        assert_eq!(l.len(), 3);
        assert_eq!(l.events[0].tick, 12_000);
        assert_eq!(l.events[1].tick, 24_000);
        assert_eq!(l.events[2].tick, 24_000);
        assert_eq!(l.playback_cursor, 3, "insertion before the cursor shifts it right");
    }

    #[test]
    fn record_event_caps_at_max_events() {
        let mut l = loop_of(120.0, 1);
        l.state = LooperState::Overdubbing;
        for i in 0..MAX_LOOP_EVENTS + 10 {
            l.playback_tick = (i as u32) % l.loop_length_ticks;
            l.record_event(60, 100, 0);
        }
        assert_eq!(l.len() as usize, MAX_LOOP_EVENTS, "excess events must be dropped");
    }

    #[test]
    fn playback_fires_events_at_exact_ticks_and_wraps() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(10, 60, 127), snap(10, 64, 127), snap(20, 60, 0)], 100);
        l.toggle_play(); // Playing from tick 0
        let quiet = l.tick(); // playback_tick 0: no events at 0
        assert_eq!(quiet[0], (0, 0, 255), "sentinel triple = no event");
        for _ in 1..10 { let _ = l.tick(); }
        let ev = l.tick(); // playback_tick == 10: BOTH events fire
        assert_eq!(ev[0], (60, 127, 0));
        assert_eq!(ev[1], (64, 127, 0));
        for _ in 11..20 { let _ = l.tick(); }
        let off = l.tick(); // playback_tick == 20
        assert_eq!(off[0], (60, 0, 0));
        // note 64 still active -> forced note-off at wrap (tick 99 -> len 100)
        for _ in 21..99 { let _ = l.tick(); }
        let wrap = l.tick();
        assert_eq!(wrap[0], (64, 0, 0), "wrap must force note-off for held notes");
        let restart = l.tick(); // playback_tick wrapped to 0
        assert_eq!(restart[0], (0, 0, 255), "no events at tick 0");
    }

    #[test]
    fn stop_drains_active_note_offs_on_next_tick() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(0, 60, 127)], 1000);
        l.toggle_play();
        let _ = l.tick(); // fires note-on; active_notes[60] = 0
        l.toggle_play();  // Playing -> stop(): needs_note_offs = true
        let drained = l.tick(); // Idle branch drains pending offs
        assert_eq!(drained[0], (60, 0, 0));
        let again = l.tick();
        assert_eq!(again[0], (0, 0, 255), "drain happens exactly once");
    }

    #[test]
    fn overdub_layers_and_undo_removes_only_top_layer() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(0, 60, 127), snap(10, 60, 0)], 1000); // base layer 0
        l.toggle_play();
        l.start_record(); // Playing -> Overdubbing, layer 1
        assert_eq!(l.current_layer, 1);
        l.playback_tick = 5;
        l.record_event(64, 100, 0); // layer 1
        l.stop_record();
        l.undo(); // removes exactly layer 1
        assert_eq!(l.len(), 2, "base layer must survive");
        assert_eq!(l.current_layer, 0);
        let notes: Vec<u8> = (0..l.len() as usize).map(|i| l.events[i].note).collect();
        assert_eq!(notes, vec![60, 60]);
        l.undo(); // layer 0: no-op
        assert_eq!(l.len(), 2);
        assert_eq!(l.current_layer, 0);
    }

    #[test]
    fn undo_mid_pass_repartitions_cursor() {
        // Undo while past the removed event's tick: cursor must be
        // recomputed (partition_point, looper.rs:304-305) so the stale
        // cursor never re-fires or skips events on the next pass.
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(100, 60, 127), snap(150, 60, 0)], 3000);
        l.start_record();              // Overdubbing, layer 1
        l.playback_tick = 1500;
        l.record_event(70, 100, 0);
        l.stop_record();               // Playing
        for _ in 0..1600 { let _ = l.tick(); } // playback_tick 1600, past 1500
        l.undo();                      // removes (1500, note 70)
        assert_eq!(l.len(), 2);
        assert_eq!(l.playback_cursor, 2, "cursor repartitioned past tick 1600");
        for _ in 0..1400 { let _ = l.tick(); } // finish the pass (3000 -> wrap)
        let mut fired_70 = 0;
        let mut fired_60 = 0;
        for _ in 0..3000 {             // full second pass
            for &(n, _v, p) in &l.tick() {
                if p == 255 { break; }
                match n { 70 => fired_70 += 1, 60 => fired_60 += 1, _ => {} }
            }
        }
        assert_eq!(fired_70, 0, "removed layer must not fire");
        assert_eq!(fired_60, 1, "base layer fires once per pass (positive control)");
    }

    #[test]
    fn first_pass_recording_switches_to_overdub_at_loop_end() {
        let mut l = loop_of(120.0, 1); // loop 96000
        l.record_event(60, 127, 0);
        for _ in 0..95_999 { let _ = l.tick(); }
        assert_eq!(l.state, LooperState::Recording, "first pass does not play back");
        let _ = l.tick(); // playback_tick reaches loop_length -> switch
        assert_eq!(l.state, LooperState::Overdubbing);
        assert_eq!(l.current_layer, 1);
        assert_eq!(l.playback_tick, 0, "position resets for the overdub pass");
        assert_eq!(l.loop_length_ticks, 96_000, "loop length survives the switch");
    }
}
```

(`snap` — хелпер из соседнего `restore_tests`, looper.rs:593–595; продублировать локально, если модуль не видит его: `fn snap(tick: u32, note: u8, vel: u8) -> LooperEventSnapshot { LooperEventSnapshot { tick, note, velocity: vel, layer_id: 0, part_id: 0 } }`.)

- [ ] **Step 2: engine-level тест** — создать `tests/looper_timing.rs`:

```rust
//! Engine-level looper timing (REFACTOR_AUDIT S0): LooperRestore + play must
//! fire recorded events at sample-accurate positions through tick_block,
//! and the loop wrap must repeat them every pass.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{looper::LooperEventSnapshot, ControlEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
/// Loop 24000 ticks = exactly 750 BLOCK_SIZE blocks — block-aligned passes.
const LOOP_TICKS: u32 = 24_000;
const LOOP_BLOCKS: usize = LOOP_TICKS as usize / BLOCK_SIZE;
const PASSES: usize = 3;

fn engine_with_note() -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches);
    let spent = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    e
}

#[test]
fn looper_replays_events_at_recorded_sample_positions() {
    let mut e = engine_with_note();
    // note-on at tick 1000 (block 31.25), note-off at tick 2000 (block 62.5)
    let events: Box<[LooperEventSnapshot]> = vec![
        LooperEventSnapshot { tick: 1000, note: 60, velocity: 127, layer_id: 0, part_id: 0 },
        LooperEventSnapshot { tick: 2000, note: 60, velocity: 0, layer_id: 0, part_id: 0 },
    ]
    .into_boxed_slice();
    let spent = e.handle_control(ControlEvent::LooperRestore { events, loop_length_ticks: LOOP_TICKS });
    drop(spent); // heap payload comes back out of handle_control
    let _ = e.handle_control(ControlEvent::LooperTogglePlay); // Playing from tick 0

    let mut first_sound = [None; PASSES];
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for b in 0..PASSES * LOOP_BLOCKS {
        e.tick_block(&mut l, &mut r);
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &s| m.max(s.abs()));
        let pass = b / LOOP_BLOCKS;
        let in_pass = b % LOOP_BLOCKS;
        // skip the first 3 blocks of a pass: release tail of the previous
        // pass must not be mistaken for the new attack
        if peak > 1e-3 && in_pass > 3 && first_sound[pass].is_none() {
            first_sound[pass] = Some(in_pass);
        }
    }
    for (p, fs) in first_sound.iter().enumerate() {
        let fs = fs.unwrap_or_else(|| panic!("pass {p}: looper never sounded"));
        // tick 1000 lives in block 31 (ticks 992..1023)
        assert!(
            (29..=33).contains(&fs),
            "pass {p}: first sounding block {fs}, expected ~31 (event at tick 1000)"
        );
    }
}
```

- [ ] **Step 3: зелёный (characterization)** —
```bash
cargo test --lib timing_tests
cargo test --test looper_timing
```
Ожидание: все зелёные СРАЗУ (пинуем существующее поведение). Если `undo_mid_pass_repartitions_cursor` красный — пересчитать арифметику по фактическому коду `undo`/`tick` (метод: печатать `playback_tick`/`playback_cursor` через `dbg!` во временном прогоне) и скорректировать ОЖИДАНИЯ теста под фактическое поведение с комментарием «pins current behavior» — это characterization, не багфикс. Если красный `looper_replays_events_at_recorded_sample_positions` — проверить порог 1e-3 против фактической громкости init-патча (`render_peak`-подход из tests/root_only.rs) и window 29..=33 против фактического блока первого звука; подвинуть окно под измеренное, не сужать семантику.

- [ ] **Step 4: гейты + коммит** —
```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
git add src/synth/looper.rs tests/looper_timing.rs
git commit -m "test: looper timing characterization — quantize grids, sorted record insert, wrap note-offs, overdub/undo layer and cursor semantics, engine-level sample-accurate replay (audit S0)"
```

**RT-safety note:** тесты; looper.rs меняется только внутри `#[cfg(test)]`.

---

# Группа D: Voice stealing (Task 4)

### Task 4: src/synth/tests.rs — Poly-High / Poly-Low / oldest-first

**Files:**
- Modify: `src/synth/tests.rs` (рядом с `steal_prefers_releasing_voice`, :896–918)

**Факты (anchor-verified):** `Part::note_on` — mod.rs:617; poly-путь mod.rs:712–746: free-voice-first (714), иначе play_mode 4 → `min_by_key(note)` (717–723), 5 → `max_by_key(note)` (725–730), default → releasing-oldest → oldest (736–743). `VOICES_PER_LAYER = 8` (mod.rs:381). `Part::new` — mod.rs:527; `enabled: false` по умолчанию (528) — существующий тест ставит `p.enabled = true` (tests.rs:899). Существующий тест доказывает releasing-first; oldest-first без releasing, Poly-High и Poly-Low — не покрыты.

- [ ] **Step 1: тесты** — добавить в `src/synth/tests.rs` после `steal_prefers_releasing_voice`:

```rust
// ── Voice stealing: Poly-High / Poly-Low / oldest-first (audit S0) ────

fn part_with_mode(play_mode: f32) -> Part {
    let mut p = Part::new(48_000.0);
    p.enabled = true; // note_on bails on !enabled (mod.rs:613... now 617)
    p.params.play_mode = play_mode;
    p
}

/// Occupy ALL 8 voices (VOICES_PER_LAYER, mod.rs:381) with sustained notes
/// 36..44 pressed in order — nothing releasing, ages strictly increasing.
fn fill_voices(p: &mut Part) {
    for n in 36..36 + p.voices.len() as u8 {
        p.note_on(n, 100);
    }
}

#[test]
fn poly_high_steals_the_lowest_note() {
    // play_mode 4: highest note wins -> lowest (36) is stolen (mod.rs:717-723)
    let mut p = part_with_mode(4.0);
    fill_voices(&mut p);
    p.note_on(80, 100);
    assert!(p.voices.iter().any(|v| v.note == 80), "new note must sound");
    assert!(!p.voices.iter().any(|v| v.src_note == 36), "lowest note 36 must be stolen");
    let hi = p.voices.iter().find(|v| v.src_note == 43).expect("highest note 43 must survive");
    assert!(hi.active && hi.note == 43, "surviving voice must still sound note 43");
}

#[test]
fn poly_low_steals_the_highest_note() {
    // play_mode 5: lowest note wins -> highest (43) is stolen (mod.rs:725-730)
    let mut p = part_with_mode(5.0);
    fill_voices(&mut p);
    p.note_on(80, 100);
    assert!(p.voices.iter().any(|v| v.note == 80), "new note must sound");
    assert!(!p.voices.iter().any(|v| v.src_note == 43), "highest note 43 must be stolen");
    let lo = p.voices.iter().find(|v| v.src_note == 36).expect("lowest note 36 must survive");
    assert!(lo.active && lo.note == 36, "surviving voice must still sound note 36");
}

#[test]
fn default_poly_steals_the_oldest_sustained_voice() {
    // No releasing voices -> oldest sustained (first pressed, 36) dies;
    // JUCE-style order, mod.rs:736-743. (Releasing-first is covered by
    // steal_prefers_releasing_voice above.)
    let mut p = part_with_mode(0.0);
    fill_voices(&mut p);
    p.note_on(80, 100);
    assert!(p.voices.iter().any(|v| v.note == 80), "new note must sound");
    assert!(!p.voices.iter().any(|v| v.src_note == 36), "oldest voice (36) must be stolen");
    let recent = p.voices.iter().find(|v| v.src_note == 43).expect("newest voice 43 must survive");
    assert!(recent.active && recent.note == 43);
}
```

- [ ] **Step 2: зелёный (characterization)** —
```bash
cargo test --lib poly_high_steals_the_lowest_note
cargo test --lib poly_low_steals_the_highest_note
cargo test --lib default_poly_steals_the_oldest_sustained_voice
cargo test --lib steal_prefers_releasing_voice   # сосед не сломан
```
Ожидание: зелёные сразу. Если RED — behavior не такой, как читается по коду (например, envelope-фаза ноты ещё не «sustained» к моменту steal — env в attack): пиновать ФАКТИЧЕСКОЕ поведение — пере-сформулировать ассерты после чтения `Voice::is_releasing` (voice.rs) и рендера нескольких тиков (`for _ in 0..50 { p.tick(&mods) }` при необходимости), с комментарием, фиксирующим фактическую семантику. Не ослаблять до тривиальности.

- [ ] **Step 3: гейты + коммит** —
```bash
cargo clippy --all-targets -- -D warnings
cargo test
git add src/synth/tests.rs
git commit -m "test: voice-stealing characterization — Poly-High steals lowest, Poly-Low steals highest, default steals oldest sustained (audit S0)"
```

---

# Группа E: Perf-load последовательность + re-send контракт (Task 5)

### Task 5: src/synth/tests.rs + tests/perf_load_seq.rs

**Files:**
- Modify: `src/synth/tests.rs` (TC25–TC27)
- Create: `tests/perf_load_seq.rs` (TC28–TC30)

**Факты (anchor-verified):** движковые армы: `SetPartEnabled` mod.rs:2138–2159 (drain при disable), `SetPartMute` :2160–2186 (mute → drain + sampler CC7 gate 0), `SetPartVolume` :2196–2204 (`l.volume = volume` raw), `SetPartRange` :2205–2207, `SetPartVelRange` :2208–2210 (raw), `SetPartPan` :2211–2222 (`pan.clamp(-1,1)`), `SetPartTranspose` :2223–2231 (raw), `SetPartMono/Legato/Glide` :2232+, `SetMacro` :2263+, `SetPartSf2Mode` :2468+, `SetPartSf2Program` :259-. Out-of-range part → `get_mut` None → no-op везде. Re-send: `send_full_part_state` (gui/mod.rs:1714–1741) пушит per part: Enabled, Mute, Volume, Range, VelRange, Pan, Transpose, SetMacro ×8, и при sf2_mode — Sf2Mode + Sf2Program. Perf-load tail-loop: gui/mod.rs:2197–2202. `App` в тестах несобираем (live `AudioBackend` в `_audio_handle`, lib.rs:884) → re-send/perf-load покрываются как контракт последовательности на уровне движка (зеркальный тест, как ui_layout_contract.rs). Пресет-сторона: `load_performance`/`migrate_performance_parts` (preset.rs:810–829), serde-дефолты (preset.rs:762–787). `tempfile` в [dev-dependencies], `serde_json` в [dependencies].

- [ ] **Step 1: engine-side контракт-тесты** — добавить в конец `src/synth/tests.rs`:

```rust
// ── Part control-event → engine-field contract (audit S0) ──────────────
// Perf-load (gui load_performance_by_name) and the audio reconnect re-send
// (send_full_part_state) rely on every SetPart* event landing in exactly
// one engine field. This pins that mapping — including the pan clamp
// (mod.rs:2216) and the raw volume store (mod.rs:2198).

#[test]
fn part_control_events_land_in_engine_fields() {
    let mut e = SynthEngine::new(48_000.0);
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 0.42 });
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 36, max_note: 84 });
    let _ = e.handle_control(ControlEvent::SetPartVelRange { part: 0, vel_min: 20, vel_max: 100 });
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 0, pan: -0.5 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: -12 });
    let _ = e.handle_control(ControlEvent::SetMacro { part: 0, index: 3, value: 0.75 });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    let p = &e.parts[0];
    assert!(p.enabled);
    assert!((p.volume - 0.42).abs() < 1e-6, "volume is stored raw (mod.rs:2198)");
    assert!(p.mute);
    assert_eq!((p.min_note, p.max_note), (36, 84));
    assert_eq!((p.vel_min, p.vel_max), (20, 100));
    assert!((p.pan + 0.5).abs() < 1e-6);
    assert_eq!(p.transpose, -12);
    assert!((p.macro_vals[3] - 0.75).abs() < 1e-6);
    assert!(p.sf2_mode);
    // pan is clamped into [-1, 1] (mod.rs:2216) — pin the clamp
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 0, pan: 7.0 });
    assert!((e.parts[0].pan - 1.0).abs() < 1e-6, "pan clamped to +1");
    // Negative: out-of-range part is a no-op everywhere
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 99, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 99, pan: 0.1 });
    let _ = e.handle_control(ControlEvent::SetMacro { part: 99, index: 0, value: 1.0 });
    assert_eq!(e.parts.len(), crate::synth::MAX_PARTS, "no part was created");
}

#[test]
fn full_state_re_send_restores_a_fresh_engine() {
    // Mirrors send_full_part_state (gui/mod.rs:1714-1741): after an audio
    // reconnect a brand-new engine receives ONLY this SetPart* sequence and
    // must match the pre-reconnect engine's part configuration. Sounding
    // voices are deliberately NOT carried (documented re-send semantics).
    let mut configured = SynthEngine::new(48_000.0);
    let patches = crate::preset::load_all_patches();
    let _ = configured.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    configured.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    push_full_part_state(&mut configured, 0, true, 0.9, 24, 96, 10, 120, 0.25, 7);
    push_full_part_state(&mut configured, 1, false, 0.5, 0, 127, 1, 127, 0.0, 0);

    let mut fresh = SynthEngine::new(48_000.0);
    push_full_part_state(&mut fresh, 0, true, 0.9, 24, 96, 10, 120, 0.25, 7);
    push_full_part_state(&mut fresh, 1, false, 0.5, 0, 127, 1, 127, 0.0, 0);

    for i in 0..2usize {
        let (a, b) = (&configured.parts[i], &fresh.parts[i]);
        assert_eq!(a.enabled, b.enabled, "part {i} enabled");
        assert_eq!(a.mute, b.mute, "part {i} mute");
        assert_eq!(a.volume, b.volume, "part {i} volume");
        assert_eq!((a.min_note, a.max_note), (b.min_note, b.max_note), "part {i} range");
        assert_eq!((a.vel_min, a.vel_max), (b.vel_min, b.vel_max), "part {i} vel range");
        assert_eq!(a.pan, b.pan, "part {i} pan");
        assert_eq!(a.transpose, b.transpose, "part {i} transpose");
        assert_eq!(a.macro_vals, b.macro_vals, "part {i} macros");
        assert_eq!(a.sf2_mode, b.sf2_mode, "part {i} sf2_mode");
    }
    assert!(
        configured.parts[0].voices.iter().any(|v| v.active),
        "configured engine keeps its sounding voice (re-send does not carry it)"
    );
    assert!(
        !fresh.parts[0].voices.iter().any(|v| v.active),
        "fresh engine starts silent until a note arrives"
    );
}

/// Exact event order of send_full_part_state (gui/mod.rs:1724-1739).
#[allow(clippy::too_many_arguments)]
fn push_full_part_state(
    e: &mut SynthEngine, part: usize, enabled: bool, volume: f32,
    min_note: u8, max_note: u8, vel_min: u8, vel_max: u8,
    pan: f32, transpose: i8,
) {
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part, enabled });
    let _ = e.handle_control(ControlEvent::SetPartMute { part, mute: false });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part, volume });
    let _ = e.handle_control(ControlEvent::SetPartRange { part, min_note, max_note });
    let _ = e.handle_control(ControlEvent::SetPartVelRange { part, vel_min, vel_max });
    let _ = e.handle_control(ControlEvent::SetPartPan { part, pan });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part, semitones: transpose });
    for index in 0..8usize {
        let _ = e.handle_control(ControlEvent::SetMacro { part, index, value: 0.0 });
    }
    // sf2_mode == false in both calls here: the Sf2Mode/Sf2Program pair is
    // pushed only when sf2_mode (gui/mod.rs:1734-1739) — TC25 pins the arm.
}

#[test]
fn absent_parts_are_disabled_by_tail_loop() {
    // Perf-load tail loop (gui/mod.rs:2197-2202): parts beyond the
    // performance's part list must not keep playing — replay pins the
    // idempotent SetPartEnabled(false) sweep.
    let mut e = SynthEngine::new(48_000.0);
    for i in 0..4usize {
        let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled: true });
    }
    // The "performance" carries 1 part -> tail loop disables 1..MAX_PARTS
    for i in 1..crate::synth::MAX_PARTS {
        if e.parts[i].enabled {
            let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled: false });
        }
    }
    assert!(e.parts[0].enabled, "part 0 stays enabled");
    for i in 1..4usize {
        assert!(!e.parts[i].enabled, "part {i} disabled by the tail loop");
    }
    // Idempotent: a second sweep changes nothing
    for i in 1..crate::synth::MAX_PARTS {
        let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled: false });
    }
    for i in 1..4usize {
        assert!(!e.parts[i].enabled);
    }
    assert!(e.parts[0].enabled);
}
```

(`MAX_PARTS` — проверить импорт: в `src/synth/tests.rs` доступен через `use super::*`; если нет — `crate::synth::MAX_PARTS`.)

- [ ] **Step 2: perf-файл контракт** — создать `tests/perf_load_seq.rs`:

```rust
//! Performance file load/migration contract (REFACTOR_AUDIT S0, prep for QW5).
//! Pins preset::load_performance / migrate_performance_parts behavior before
//! the GUI loader merge (audit quick-win #5).

use mini_midi_synth::preset::{self, PartConfig, Performance};

fn part(patch_name: &str, enabled: bool) -> PartConfig {
    PartConfig {
        patch_name: patch_name.to_string(),
        enabled,
        volume: 0.8,
        key_low: 0,
        key_high: 127,
        param_overrides: Default::default(),
        sf2_mode: false,
        sf2_program: 0,
        pan: 0.0,
        transpose: 0,
        mute: false,
        vel_min: 1,
        vel_max: 127,
        root_only: false,
    }
}

fn write_perf(dir: &tempfile::TempDir, name: &str, perf: &Performance) -> std::path::PathBuf {
    let path = dir.path().join(format!("{name}.json"));
    serde_json::to_writer(std::fs::File::create(&path).unwrap(), perf).unwrap();
    path
}

#[test]
fn legacy_8_part_performance_migrates_zone_b_to_8_12() {
    // Legacy 8-part layout kept zone B at indices 4..8; migration relocates
    // it to 8..12 (preset.rs:818-829).
    let perf = Performance {
        name: "legacy".into(),
        category: String::new(),
        split_point: None,
        parts: (0..8)
            .map(|i| part(&format!("P{i}"), i < 4 || i >= 6))
            .collect(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = write_perf(&dir, "legacy", &perf);
    let loaded = preset::load_performance(&path).unwrap();
    assert_eq!(loaded.parts.len(), 12, "8-part file migrates to 16-part layout slots");
    for i in 0..4 {
        assert_eq!(loaded.parts[i].patch_name, format!("P{i}"));
        assert!(loaded.parts[i].enabled, "zone A preserved enabled");
    }
    for i in 4..8 {
        assert!(!loaded.parts[i].enabled, "gap 4..8 must be disabled");
    }
    for i in 8..12 {
        assert_eq!(loaded.parts[i].patch_name, format!("P{}", i - 4), "zone B relocated");
    }
    assert!(loaded.split_point.is_none(), "None split_point survives");
}

#[test]
fn missing_optional_fields_get_defaults() {
    // Hand-written minimal JSON: vel_min/vel_max/mute/pan/transpose/
    // sf2_*/root_only/category/split_point omitted.
    let json = r#"{
        "name": "minimal",
        "parts": [ { "patch_name": "Init", "enabled": true, "volume": 0.8,
                     "key_low": 0, "key_high": 127 } ]
    }"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("minimal.json");
    std::fs::write(&path, json).unwrap();
    let loaded = preset::load_performance(&path).unwrap();
    let p = &loaded.parts[0];
    assert_eq!(p.vel_min, 1, "serde default (preset.rs:786)");
    assert_eq!(p.vel_max, 127);
    assert!(!p.mute);
    assert_eq!(p.pan, 0.0);
    assert_eq!(p.transpose, 0);
    assert!(!p.sf2_mode);
    assert_eq!(p.sf2_program, 0);
    assert!(!p.root_only);
}

#[test]
fn sixteen_part_performance_passes_through_unchanged() {
    let perf = Performance {
        name: "modern".into(),
        category: String::new(),
        split_point: Some(60),
        parts: (0..16)
            .map(|i| part(&format!("Q{i}"), i % 2 == 0))
            .collect(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = write_perf(&dir, "modern", &perf);
    let loaded = preset::load_performance(&path).unwrap();
    assert_eq!(loaded.parts.len(), 16, "16-part file must not be touched");
    for i in 0..16 {
        assert_eq!(loaded.parts[i].patch_name, format!("Q{i}"));
        assert_eq!(loaded.parts[i].enabled, i % 2 == 0, "order and flags preserved");
    }
    assert_eq!(loaded.split_point, Some(60));
}
```

- [ ] **Step 3: прогон** —
```bash
cargo test --lib part_control_events
cargo test --lib full_state_re_send
cargo test --lib absent_parts
cargo test --test perf_load_seq
```
Ожидание: зелёные сразу. Если RED: (a) `part_control_events_land_in_engine_fields` — поведение арма отличается от чтения (например, volume хранится с преобразованием) → пиновать фактическое значение с комментарием «pins current behavior: <что именно>»; (b) perf-тесты — сверить serde-вывод с фактической структурой PartConfig.

- [ ] **Step 4: гейты + коммит** —
```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
git add src/synth/tests.rs tests/perf_load_seq.rs
git commit -m "test: perf-load sequence + reconnect re-send contract — SetPart* to engine-field mapping, full-state re-send equivalence, tail-loop disable, performance JSON migration/defaults (audit S0/QW5 prep)"
```

---

## Финальные гейты (после всех тасков)

```bash
cargo clippy --all-targets -- -D warnings          # clean
cargo test                                         # all green (golden + новые suite'ы)
cargo test --features rt-assert --test rt_safety   # all green, no aborts (ignored known-RED не считаются; now 5+N tests, 2 ignored)
cargo deny check                                   # clean (pre-existing yanked clipboard-win — warn-level, не блокер)
```

## Ручной чек-лист

- [ ] `MINI_MIDI_GOLDEN_UPDATE=1` НЕ установлен в обычных прогонах (golden-тесты читают файл)
- [ ] `tests/golden/*.txt` закоммичены и сгенерированы на текущей машине/тулчейне
- [ ] `cargo test --features rt-assert --test rt_safety` включён в описание PR / чек-лист (CI-пункт: suite feature-gated, под обычным `cargo test` — 0 тестов)
- [ ] Отчёт плана перечисляет все known-RED находки (osc-семейства из Task 2 Step 4, если появились) как новые баги в backlog — НЕ чинить в этом плане

## Известные риски

1. **Task 2 (TC6): неизвестные аллокации в осц-семействах.** Таблица из 29 семейств впервые гоняет note-on под guard'ом — с высокой вероятностью всплывут 1–3 находки класса wavetable (KS/CommutedPiano буферы, PianoModel). Это НЕ сбой плана, а его цель: процедура изоляции в Step 4; находки документируются `#[ignore]` + TODO, чинятся отдельными quick-win'ами после S0.
2. **Golden-хэши платформо-зависимы.** На aarch64 (Orange Pi 5) пресеты могут дать другие хэши — регенерировать на месте (`MINI_MIDI_GOLDEN_UPDATE=1`) и коммитить per-platform при необходимости; TC1/TC2 остаются валидными везде. Любое обновление тулчейна/Rust может изменить codegen → регенерация с осознанным коммитом.
3. **Время прогона golden-сьютa** (~2–4 мин). Если станет болью — уменьшить `BLOCKS_PER_RENDER` один раз (Step 3 Task 1), не превращать в `#[ignore]`.
4. **Task 5: engine-поля могут отличаться от чтения** (clamps, преобразования). Политика — пиновать фактическое поведение с комментарием; любые неожиданности (например, volume не raw) — находка для QW5/S7, не правка движка.
5. **`LooperRestore`-payload.** Забытый `drop(spent)` в тестах task 3 уронит тест по `#[must_use]`/leak — в коде выше уже учтено.
6. **QW-план идёт первым или параллельно:** если QW1/QW2 уже влиты, `tests/rt_safety.rs` получит их тесты — настоящие Task 2 добавляет свои тесты в конец файла, конфликтов по смыслу нет; при merge-конфликте — rebase, не дедуплицировать (тесты разные).
7. **`init`-пресет отсутствует** в фабричном наборе — тесты падают с понятным `expect`; fallback на `patches.first()` применён только там, где семантика не критична (Task 3 engine-тест), для osc/fx-патчей init обязателен (нужен известный базовый osc_type).
