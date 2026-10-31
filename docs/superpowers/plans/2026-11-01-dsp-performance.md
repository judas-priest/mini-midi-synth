# DSP Performance Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-01 (база c97537e = origin/master, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** Устранить причины просадки DSP: LTO в release, залипание легаси-FX, per-sample log10/powf в мастер-компрессоре, libm sin в осцилляторах, врущая метрика dsp%; idle-сценарий в бенче.

**Architecture:** Аудио-путь (`src/synth/`, `src/audio.rs`, `src/metrics.rs`) + сборка. База измерений (аудит 2026-11-01): release-движок здоров (обычно <5% бюджета, худший патт ~30-36%), debug ×3.5-12 — пользовательская просадка была debug-сборкой; реальный рост нагрузки дают залипшие легаси-FX и безусловный компрессор. Research (Tavily 2026-11-01): gain computer компрессора на control-rate (Ramos, DAFx 2011) с per-sample только peak-детектором и линейным ramp'ом; метрика — avg + peak-hold (практика REAPER), бюджет от фактического буфера.

**Tech Stack:** Rust, cpal (vendored), criterion (benches/dsp.rs уже есть).

---

## Test Cases (from the requirement)

- TC1 (тест в compressor.rs): новая реализация tick даёт тот же steady-state gain, что старая формула, в пределах ±0.35 dB для тишины/ниже порога/в колене/выше порога.
- TC2 (проверка по коду — юнит-теста нет: движок не собирается вне аудио-пути в тестах): после удаления/выключения Phaser-слота флаг-скан обнуляет `self.phaser.mix` на следующем блоке (было — 1.0 навсегда).
- TC3 (бенч): `cargo bench --bench dsp` — цифры не хуже базы (idle ~8 мкс, grand piano 8v ~180 мкс/блок); компрессор не добавляет >5%.
- TC4 (GUI): dsp-индикатор показывает avg и peak; бюджет считается от фактического буфера (после negotiate).
- TC5 (rt_safety): `cargo test --features rt-assert --test rt_safety` зелёный после всех правок аудио-пути.

## Negative Matrix

- zero/empty — envelope=0 (тишина): старый код давал env_db=-100, gain=makeup; блок-rate путь сохраняет это (target из зажатого env_db). Явный тест в TC1.
- duplicate/concurrent — вызов tick дважды на сэмпл (мастер+слот, известный S3-долг): НЕ меняется этим планом, документируется.
- clock skew / missing field / external timeout — N/A: замкнутый DSP-цикл.
- переполнение sum_callback_us: u64 мкс — 500k лет колбэков; N/A.

---

### Task 1: LTO в release-профиле

**Files:** Modify: `Cargo.toml:70-80`

```toml
[profile.release]
opt-level = 3
# fat LTO + codegen-units=1: ship-профиль (аудит DSP 2026-11-01: кросс-крейт
# инлайнинг oscillator→voice даёт заметный % на aarch64). Сборка ~50-80 c —
# для итераций использовать `cargo check` / dev-профиль.
lto = "fat"
codegen-units = 1
panic = "abort"
# `debuginfo` rather than `symbols`: the function name table survives, so a
# native crash in logcat is readable via ndk-stack, while the DWARF sections
# that make the .so huge are still gone.
strip = "debuginfo"
```

- [ ] Step 1: правка; `cargo build --release` проходит (долгое, ~1-2 мин)
- [ ] Step 2: commit `build: enable fat LTO + single codegen unit in release`

### Task 2: Сброс залипших легаси-FX

**Files:** Modify: `src/synth/mod.rs` — в `tick_block`, сразу после `let fx_chain = ep.map(|p| p.fx_chain).unwrap_or_default();` (~:3276)

```rust
        // Легаси-FX инстансы шарятся мастером и 16-слотовой цепочкой: слот
        // ставит mix=1.0, но никто не сбрасывал — после первого использования
        // Phaser/Flanger/Overdrive/Tremolo/Bitcrusher тот тикал на мастере
        // вечно (аудит DSP 2026-11-01, находка 1). Полное разделение — S3.
        {
            use fx_chain::FxSlotType;
            let (mut od, mut ph, mut fl, mut tr, mut bc) = (false, false, false, false, false);
            if let Some(p) = ep {
                for slot in &p.fx_chain.slots {
                    // Гейт как в аплайере (mod.rs:3565): выключенный или
                    // нулевой слот НЕ держит легаси-инстанс включённым.
                    if !slot.enabled || slot.mix <= 0.001 { continue; }
                    match slot.slot_type {
                        FxSlotType::Overdrive => od = true,
                        FxSlotType::Phaser => ph = true,
                        FxSlotType::Flanger => fl = true,
                        FxSlotType::Tremolo => tr = true,
                        FxSlotType::Bitcrusher => bc = true,
                        _ => {}
                    }
                }
            }
            if !od { self.overdrive.mix = 0.0; }
            if !ph { self.phaser.mix = 0.0; }
            if !fl { self.flanger.mix = 0.0; }
            if !tr { self.tremolo.mix = 0.0; }
            if !bc { self.bitcrusher.mix = 0.0; }
        }
```

Ограничение (документировать комментарием там же): двойная обработка, ПОКА слот активен (мастер тикает тот же инстанс), этим фиксом не убирается — это перестройка FX-графа (S3). Поведенческая дельта компрессора (ревью): с makeup>0 первый gain-глид 1.0→makeup за 64 сэмпла после создания/смена SR (~1.3 мс) — прокомментировать в коде.

- [ ] Step 1: правка; TC2-ручная проверка логики по коду; `cargo test` + rt_safety
- [ ] Step 2: commit `fix: legacy master FX unstick — reset mix when the chain has no such slot`

### Task 3: Компрессор — control-rate gain computer (research: Ramos DAFx 2011)

**Files:** Modify: `src/synth/compressor.rs`
- Test: `#[cfg(test)]` в том же файле

Per-sample остаётся только peak one-pole + линейный ramp гейна; dB-математика (log10/powf) — раз в 64 сэмпла (750 Гц @48k), libm там допустим. Заменить `tick` и добавить поля:

```rust
pub struct Compressor {
    envelope: f32,
    sample_rate: f32,
    pub threshold: f32,
    pub ratio: f32,
    attack: f32,
    release: f32,
    attack_coeff: f32,
    release_coeff: f32,
    pub makeup: f32,
    pub mix: f32,
    /// Control-rate gain computer (Ramos DAFx 2011): dB-математика раз в
    /// 64 сэмпла, между обновлениями — линейный ramp гейна.
    gain: f32,
    gain_step: f32,
    blocks_left: u8,
}

const GAIN_UPDATE_INTERVAL: u8 = 64;

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        let attack = 0.001_f32;
        let release = 0.08_f32;
        let mut s = Self {
            envelope: 0.0,
            sample_rate,
            threshold: -6.0,
            ratio: 20.0,
            attack,
            release,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            makeup: 0.0,
            mix: 1.0,
            gain: 1.0,
            gain_step: 0.0,
            blocks_left: 0,
        };
        s.update_coeffs();
        s
    }

    /// Пересчёт целевого гейна (control-rate): dB-математика здесь и только здесь.
    fn update_gain_target(&mut self) {
        let env_db = if self.envelope > 0.00001 {
            20.0 * self.envelope.log10()
        } else {
            -100.0
        };
        let knee_width: f32 = 6.0;
        let knee_half = knee_width * 0.5;
        let over = env_db - self.threshold;
        let gain_reduction_db = if over < -knee_half {
            0.0
        } else if over < knee_half {
            let t = over + knee_half;
            (1.0 / self.ratio - 1.0) * t * t / (2.0 * knee_width)
        } else {
            (1.0 / self.ratio - 1.0) * over
        };
        let target = 10.0_f32.powf((gain_reduction_db + self.makeup) / 20.0);
        // Линейный ramp от текущего гейна к целевому за интервал.
        self.gain_step = (target - self.gain) / GAIN_UPDATE_INTERVAL as f32;
        self.blocks_left = GAIN_UPDATE_INTERVAL;
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        // Peak detection + one-pole envelope — per-sample (дёшево: abs + fma)
        let input_level = in_l.abs().max(in_r.abs());
        let coeff = if input_level > self.envelope { self.attack_coeff } else { self.release_coeff };
        self.envelope += coeff * (input_level - self.envelope);

        // Control-rate gain computer
        if self.blocks_left == 0 {
            self.update_gain_target();
        }
        self.blocks_left -= 1;
        self.gain += self.gain_step;

        let wl = in_l * self.gain;
        let wr = in_r * self.gain;
        let m = self.mix;
        (in_l * (1.0 - m) + wl * m, in_r * (1.0 - m) + wr * m)
    }
}
```

(`set_sample_rate`/`update_coeffs` без изменений, добавив `self.gain = 1.0; self.gain_step = 0.0; self.blocks_left = 0;` в set_sample_rate. Удалить из struct старых полей ничего — меняется только перечисленное.)

Тест (в конец файла):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// TC1: новая реализация держит тот же steady-state гейн, что старая
    /// формула (±0.35 dB), ниже порога / в колене / выше порога.
    #[test]
    fn steady_state_gain_matches_reference() {
        let cases: [(f32, f32); 4] = [
            (0.0,  0.0),          // тишина: envelope→0, target = makeup dB
            (0.05, 0.0),          // ниже порога: gain = makeup dB
            (0.5,  0.0),          // в колене (env_db ≈ −6.02, over ≈ −0.02)
            (0.90, 0.0),          // выше порога
        ];
        for (level, _label) in cases {
            let mut c = Compressor::new(48000.0);
            // Прогрев: держим уровень, ждём установления ramp'ов
            for _ in 0..48000 { c.tick(level, level); }
            let mut last = 1.0;
            for _ in 0..48000 { let (l, _) = c.tick(level, level); last = l / level.max(1e-9); }
            let got_db = 20.0 * last.log10();
            // Референс: старая формула на установившемся envelope≈level
            let env_db = 20.0 * level.log10();
            let over = env_db - c.threshold;
            let kw = 6.0;
            let red = if over < -kw / 2.0 { 0.0 }
                else if over < kw / 2.0 { (1.0 / c.ratio - 1.0) * (over + kw / 2.0).powi(2) / (2.0 * kw) }
                else { (1.0 / c.ratio - 1.0) * over };
            let want_db = red + c.makeup;
            assert!((got_db - want_db).abs() < 0.35, "level {level}: got {got_db:.2} dB, want {want_db:.2} dB");
        }
    }
}
```

- [ ] Step 1: реализация + тест, `cargo test compressor` — TC1 зелёный
- [ ] Step 2: `cargo bench --bench dsp` — TC3 (сверить с базой: idle ~8 мкс, grand piano ~180 мкс/блок)
- [ ] Step 3: commit `perf: compressor — control-rate gain computer, per-sample math reduced to peak+mul`

### Task 4: Sine-LUT в осцилляторах

**Files:** Modify: `src/synth/oscillator.rs` — `f32::sin()` на hot-path: Sine (~:1080), FM (~:1112-1113), Fm3 (~:2658-2660); ±5 строк дрейф — искать `.sin()` по файлу, менять только в этих структурах

В репо уже есть sine-LUT (для LFO, коммит 4accd68 — найти `grep -n "sine_lut\|SINE_LUT" src/synth/`): поднять её в `pub(crate)`-хелпер (если ещё не) и заменить вызовы. Шаблон (адаптировать под фактическую сигнатуру LUT):

```rust
// Было: (phase + depth * mod) * TAU |> sin()
// Стало: sine_lut((phase + depth * mod).rem_euclid(1.0))
// (6 выражений sin() в 3 структурах: Sine:1080, FM:1112-1113, Fm3:2658-2660;
// прочие .sin() в файле — additive/FM2/piano — НЕ трогать)
```

Точность: существующая `sine_lut` (lfo.rs:24-28) — 2048 точек, БЕЗ интерполяции (nearest) → макс. ошибка ≈1.5e-3; тест с допуском 1e-2.

КРИТИЧНО (ревью): аргумент может быть ОТРИЦАТЕЛЬНЫМ (FM-модуляция ±1 в turns) — `as usize` от отрицательного f32 насыщается в 0 (выход схлопнется в sin(0) на половине цикла модуляции). Перед lookup фаза обязана быть свёрнута: `(x).rem_euclid(1.0)` (как в lfo.rs:296).

И форма на сайтах: модуляция УЖЕ в turns — `(phase + depth * sin_out) * TAU`, НЕ радианы. Замена: `sine_lut((phase + depth * mod).rem_euclid(1.0))`, никакой /TAU.

Тест:

```rust
#[test]
fn sine_lut_matches_libm() {
    for i in 0..1000 {
        let p = i as f32 / 1000.0;
        assert!((sine_lut(p) - (p * TAU).sin()).abs() < 1e-2, "phase {p}");
    }
}
```

- [ ] Step 1: найти LUT, поднять хелпер, заменить 3-4 call-site, тест
- [ ] Step 2: `cargo bench --bench dsp` — сверить Sine/патт-сценарии (не хуже базы)
- [ ] Step 3: commit `perf: sine LUT for oscillator hot paths (was per-sample libm sin)`

### Task 5: Честная метрика dsp% (avg + peak, фактический буфер)

**Files:**
- Modify: `src/metrics.rs` — avg-накопители + actual_buffer
- Modify: `src/audio.rs` — публиковать фактический буфер после negotiate
- Modify: `src/gui/mod.rs` (~:892-913) — показывать avg и peak; поле `dsp_peak: f32` в App + init в `src/lib.rs` (рядом с `dsp_load`)

metrics.rs — в `Metrics`:

```rust
    /// Longest callback duration seen since the last read, in microseconds.
    worst_callback_us: AtomicU32,
    /// Rolling average accumulators (micros sum + callback count).
    callback_us_sum: AtomicU64,
    callback_count: AtomicU32,
    /// Buffer size actually negotiated with the device (may differ from the
    /// requested one — negotiate_config silently falls back).
    actual_buffer_size: AtomicU32,
```

init: `callback_us_sum: AtomicU64::new(0), callback_count: AtomicU32::new(0), actual_buffer_size: AtomicU32::new(0),`

```rust
    #[inline]
    pub fn note_callback_time(&self, micros: u32) {
        self.worst_callback_us.fetch_max(micros, Ordering::Relaxed);
        self.callback_us_sum.fetch_add(micros as u64, Ordering::Relaxed);
        self.callback_count.fetch_add(1, Ordering::Relaxed);
    }

    /// Среднее время колбэка с последнего чтения (и сброс накопителей).
    pub fn take_avg_callback_us(&self) -> u32 {
        let sum = self.callback_us_sum.swap(0, Ordering::Relaxed);
        let n = self.callback_count.swap(0, Ordering::Relaxed);
        if n > 0 { (sum / n as u64) as u32 } else { 0 }
    }

    pub fn set_actual_buffer_size(&self, size: u32) {
        self.actual_buffer_size.store(size, Ordering::Relaxed);
    }

    pub fn actual_buffer_size(&self) -> u32 {
        self.actual_buffer_size.load(Ordering::Relaxed)
    }
```

audio.rs: после `negotiate_config(...)` (возврат `(sr, buf)`) — `crate::metrics::METRICS.set_actual_buffer_size(buf);` (найти точку вызова: `negotiate_config` в `AudioBackend::new`; её результат уже используется для стрима — добавить строку рядом).

gui/mod.rs (заменить блок ~:892-901):

```rust
                    // DSP load: avg (устойчивая) + peak-hold с медленным спадом
                    // (практика REAPER). Бюджет — от ФАКТИЧЕСКОГО буфера:
                    // negotiate может молча откатить запрошенный.
                    let actual_buf = crate::metrics::METRICS.actual_buffer_size();
                    let buf = if actual_buf > 0 { actual_buf } else { self.config.audio.buffer_size };
                    let budget_us = buf as f32 / self.sample_rate as f32 * 1e6;
                    let avg_us = crate::metrics::METRICS.take_avg_callback_us() as f32;
                    let peak_us = crate::metrics::METRICS.take_worst_callback_us() as f32;
                    if budget_us > 0.0 {
                        let avg_load = avg_us / budget_us * 100.0;
                        self.dsp_load = self.dsp_load * 0.8 + avg_load * 0.2;
                        let peak_load = peak_us / budget_us * 100.0;
                        self.dsp_peak = (self.dsp_peak * 0.98).max(peak_load);
                    }

                    ui.label(format!(
                        "{} | {}Hz | buf {} | dsp {:.0}% (peak {:.0}%)",
                        self.current_host,
                        self.sample_rate,
                        buf,
                        self.dsp_load,
                        self.dsp_peak,
                    ))
                    .on_hover_text(
                        "Audio callback time as a share of the buffer budget: \
                         average and decaying peak. Peak approaching 100% means crackle.",
                    );
```

(строку `buf {}` показываем от фактического буфера; поле `dsp_peak: f32` = 0.0 в App + lib.rs init.)

- [ ] Step 1: metrics.rs + audio.rs, clippy + `cargo test` + rt_safety
- [ ] Step 2: gui + поле, clippy
- [ ] Step 3: commit `fix: honest dsp load — avg + decaying peak, actual negotiated buffer`

### Task 6: Idle-сценарий в бенче

**Files:** Modify: `benches/dsp.rs`

По образцу существующих сценариев (init_1_voices и т.д.) добавить:

```rust
    group.bench_function("tick_block/idle", |b| {
        let mut engine = <как в соседних сценариях: init с Init-патчем, нот не класть>;
        b.iter(|| engine.tick_block(<те же аргументы>));
    });
```

(точная форма — по фактическому каркасу файла; смысл: база «движок без нот», чтобы отделять базовый cost от голосов.)

- [ ] Step 1: правка, `cargo bench --bench dsp -- idle` проходит
- [ ] Step 2: commit `bench: idle baseline scenario`

### Task 7: Верификация и запуск в release

- [ ] `cargo clippy --all-targets -- -D warnings`; `cargo test`; `cargo test --features rt-assert --test rt_safety`
- [ ] `cargo bench --bench dsp` — TC3
- [ ] PR → squash merge → в worktree: `git checkout origin/master && cargo run --release` (ТЕПЕРЬ ВСЕГДА ЗАПУСКАТЬ С --release)
- [ ] На экране: TC4 (индикатор avg/peak), общий звук без артефактов

## Out of scope

- Двойная обработка активного легаси-слота (мастер+слот тикают один инстанс) — перестройка FX-графа, REFACTOR_AUDIT S3.
- SIMD/SoA голосов, AoS-раскладка — S-пункты аудита, по замерам не требуются сейчас (движок <36% в худшем случае).
- Персист позиции окон в конфиг (#34 хвост) — сессии достаточно.
