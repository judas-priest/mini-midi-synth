# Metronome (drum-sequencer BPM) + Init Performance Reset — Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ ccdd519 чтением кода в этой сессии)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** (A) Слышимый клик-метроном, синхронный с BPM драм-секвенсора: тик на каждой
доли (step%4==0), акцент на step 0; галка в панели Drums, персист в config,
даунбит-акцент плейхеда + вспышка «beat 1» в GUI. (B) Reset-действие,
возвращающее встроенный Init-перформанс.

**Architecture:**
- Метроном живёт в `DrumEngine` (аудио-поток): при пересечении границы шага
  секвенсора с `step % 4 == 0` запускается короткий синтезированный клик
  (затухающая синусоида, per-sample математика, состояние — 4 plain-поля).
  Клик рендерится в обоих режимах (DSP и SF2) и масштабируется общим
  drum-Volume. Toggle — `ControlEvent::DrumSetMetronome { on: bool }` по образцу
  `DrumSetFixedVelocity`. BPM-источник — тот же `drum_engine.sequencer.bpm`
  (`DrumSeqBpm`); `looper_sync_bpm` синхронизирует лупер с тем же значением —
  отдельной работы не требует.
- Init-перформанс УЖЕ существует: `preset::init_performance()` (preset.rs:834),
  виртуальная запись `"(Init)"` в perf_list (lib.rs:811, layers.rs:241), загрузка
  из комбо (layers.rs:253), из pad-perf (gui/mod.rs:1897 через
  `load_performance_by_name`, вызывается при нажатии пэда gui/mod.rs:490 и из
  LED-консоли StepPerf gui/mod.rs:1837). Пресеты FACTORY_PRESETS на диск НЕ
  материализуются — они вливаются in-memory в `load_all_patches`
  (preset.rs:587); Init-перф следует той же модели (файл не нужен). Заявление
  techdebt #3 частично устарело: недостаёт только явной кнопки Reset —
  добавляем её в perf-строку Layers.

**Tech Stack:** Rust, egui, rtrb/ControlEvent, serde config — без новых зависимостей.

**Spec context:** docs/TECHDEBT.md п.1 и п.3; клик синтезируется (без сэмплов) —
в духе `ClickVoice` (drum.rs:554-620), но отдельным мини-состоянием в DrumEngine.

**Проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

Полные гейты перед финальным коммитом группы: добавить `cargo deny check`.

---

## Test Cases (from the requirement)

Требование A (techdebt #1):
1. Метроном включён + секвенсор играет на 120 BPM → периодические пики в
   аудиовыходе каждые 0.5 с (750 блоков по 32 сэмпла), в течение ≥3 бит.
2. Первый клик (step 0) громче последующих (акцент).
3. Метроном включён, секвенсор стоит → тишина (клик только при игре).
4. Toggle-off во время игры → клики прекращаются.
5. BPM секвенсора меняется → интервал кликов следует за BPM (косвенно: тест на
   240 BPM в RT-тесте пересекает границы; прямой тест интервалов — TC1 при
   120 BPM против формулы 750 блоков).
6. Галка Metronome в панели Drums тоглится, переживает рестарт (config
   `ui.drum_metronome`); старый config.json без поля → off (serde default).
7. Плейхед-колонка step 0 подсвечена акцентным цветом; при переходе
   step-атома 0→1 вспыхивает индикатор beat-1 (~150 ms).

Требование B (techdebt #3):
8. `preset::init_performance()` — 1 часть, Init-пресет, полный диапазон, не SF2,
   `split_point: None` (покрыто tests/sustain_audit.rs:306 +
   новое: patch_name ссылается на существующий фабричный пресет).
9. Кнопка Reset в perf-строке Layers загружает встроенный Init (GUI-код —
   ручная проверка; тестовый слой — данные, п.8).

## Negative Matrix

- Метроном on + transport стоит → 0 событий (TC3).
- Метроном off + transport играет → 0 событий (TC4 вторая половина).
- Toggle off в момент звучащего клика → хвост клика дотухает сам (env < 0.001 → 0),
  новых кликов нет (TC4).
- `pattern.length < 16` / не кратен 4 → клик на каждом step%4==0 внутри длины;
  на wrap секвенсора step 0 всегда акцент (документированное поведение, не тест).
- `length == 1` → current_step не меняется → prev==cur → кликов нет (edge,
  допустимо: 1-шаговый паттерн не имеет долей).
- Драм-канал `enabled == false` → `DrumEngine::tick` выходит рано → кликов нет
  (инвариант: выключенная ударка молчит целиком).
- SF2-режим ударки → клик всё равно слышен (рендер вне `!sf2_mode` ветки).
- Двойной boundary в одном сэмпле — невозможен: `samples_per_step` при 40..300 BPM
  ≥ 2400 сэмплов @48k.
- Swing не сдвигает доли: swing применяется только к нечётным шагам
  (drum.rs:793), доли (чётные шаги) всегда straight.
- `set_sample_rate` в callback → rebuild движка: флаг metronome сохраняется,
  клик-состояние сбрасывается (транзиент ≤30 ms, коэффициенты SR-зависимы).
- config без `ui.drum_metronome` / битый JSON → default false (serde default +
  Config::load fallback).
- clock skew — неприменимо (Instant монотонный); concurrent toggle — GUI-поток
  один, rtrb сериализует.
- Init-perf: файловый перф с именем "(Init)" на диске — shadowed виртуальной
  записью (perf_list.insert(0, ...) ставит её первой; комбо и pad-перф берут
  имя → виртуальная ветка срабатывает раньше файловой по имени). Не ошибка,
  документировано.

---

# Группа A: Метроном (независима от группы B)

### Task A1: Движок — клик-синтез + ControlEvent::DrumSetMetronome

**Files:**
- Modify: `src/synth/drum.rs` (DrumEngine: поля клика, триггер в `tick`, рендер)
- Modify: `src/synth/mod.rs` (ControlEvent вариант + handle_control arm)
- Create: `tests/metronome.rs` (аудио-наблюдаемые тесты)

- [ ] **Step 1: failing-тест** — создать `tests/metronome.rs`:

```rust
//! Metronome click synced to the drum step sequencer.
//! Plan: docs/superpowers/plans/2026-10-29-metronome-init-perf.md

use mini_midi_synth::synth::{ControlEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const CLICK_THRESHOLD: f32 = 0.05;

fn metronome_engine(bpm: f32, on: bool) -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on });
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm });
    e
}

/// Per-block peaks; consecutive loud blocks merge into one click event.
/// Returns (first_block_index, peak) per event.
fn click_events(e: &mut SynthEngine, blocks: usize) -> Vec<(usize, f32)> {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut events: Vec<(usize, f32)> = Vec::new();
    let mut loud_run = false;
    for b in 0..blocks {
        e.tick_block(&mut l, &mut r);
        let peak = l.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
            .max(r.iter().fold(0.0f32, |m, &s| m.max(s.abs())));
        if peak > CLICK_THRESHOLD {
            if loud_run {
                let last = events.last_mut().unwrap();
                last.1 = last.1.max(peak);
            } else {
                events.push((b, peak));
                loud_run = true;
            }
        } else {
            loud_run = false;
        }
    }
    events
}

#[test]
fn tc1_click_on_every_beat_at_120bpm() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    // 120 BPM: beat = 0.5 s = 24000 samples = 750 blocks of 32.
    // 2260 blocks (~3 s) → beats land at blocks ~750 / 1500 / 2250.
    let events = click_events(&mut e, 2260);
    assert!(events.len() >= 2, "expected >= 2 beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750");
    }
}

#[test]
fn tc2_accent_on_step_zero_is_louder() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let events = click_events(&mut e, 2260);
    assert!(events.len() >= 3, "expected >= 3 beats, got {events:?}");
    // Play resets step to 0 → first event is the step-0 accent (gain 0.9),
    // second is step 4 (gain 0.55).
    assert!(events[0].1 > events[1].1 * 1.2,
        "accent peak {} not louder than beat peak {}", events[0].1, events[1].1);
}

#[test]
fn tc3_metronome_silent_when_not_playing() {
    let mut e = metronome_engine(120.0, true);
    // ~1 beat worth of blocks; transport never started.
    assert!(click_events(&mut e, 1600).is_empty(), "click without playback");
}

#[test]
fn tc4_toggle_off_stops_clicks() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    assert!(!click_events(&mut e, 800).is_empty(), "no clicks while enabled+playing");
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    assert!(click_events(&mut e, 800).is_empty(), "clicks persist after toggle-off");
}

#[test]
fn tc5_config_missing_drum_metronome_defaults_off() {
    // Legacy config.json without the new UiSettings field must parse to off.
    let json = r#"{"audio":{"backend":"JACK","sample_rate":48000,"buffer_size":256},
"midi":{"port_name":null},"ui":{"window_width":720.0,"window_height":400.0,"last_preset":null}}"#;
    let cfg: mini_midi_synth::config::Config =
        serde_json::from_str(json).expect("legacy config must parse");
    assert!(!cfg.ui.drum_metronome);
    assert!(!mini_midi_synth::config::Config::default().ui.drum_metronome);
}
```

Запуск (ожидание: не компилируется — `DrumSetMetronome` не существует):

```bash
cargo test --test metronome
# error[E0599]: no variant or associated item named `DrumSetMetronome` found
```

- [ ] **Step 2: ControlEvent вариант** — `src/synth/mod.rs`, после
  `DrumSetFixedVelocity { fixed: bool },` (строка 216):

```rust
    /// Drums: metronome click on every beat while the sequencer plays
    DrumSetMetronome { on: bool },
```

- [ ] **Step 3: handle_control arm** — `src/synth/mod.rs`, сразу после arm
  `DrumSetFixedVelocity` (строки 2143-2145):

```rust
            ControlEvent::DrumSetMetronome { on } => {
                self.drum_engine.metronome = on;
            }
```

- [ ] **Step 4: DrumEngine — поля и константы** — `src/synth/drum.rs`.

Константы рядом с `NUM_CHOKE_GROUPS` (строка 37):

```rust
/// Metronome click: short decaying sine, synthesized (no samples).
const CLICK_FREQ_ACCENT: f32 = 1600.0; // Hz, beat 1
const CLICK_FREQ_BEAT: f32 = 1000.0;   // Hz, beats 2..4
const CLICK_GAIN_ACCENT: f32 = 0.9;
const CLICK_GAIN_BEAT: f32 = 0.55;
const CLICK_LEN_S: f32 = 0.03;         // 30 ms tail
```

Поля в `DrumEngine` после `pub pad_vel: [u8; NUM_DRUM_SLOTS],` (строка 885):

```rust
    /// Metronome click on every beat (step % 4 == 0) while playing.
    pub metronome: bool,
    // Click oscillator state — plain fields only (RT-safe, no alloc/lock).
    click_phase: f32,
    click_phase_inc: f32,
    click_env: f32,
    click_decay: f32,
```

В `DrumEngine::new` в литерале `Self { ... }` после `fixed_velocity: false,`
(строка 928):

```rust
            metronome: false,
            click_phase: 0.0,
            click_phase_inc: 0.0,
            click_env: 0.0,
            click_decay: 0.0,
```

В `set_sample_rate` (строки 980-1011): сохранить флаг и сбросить транзиент.
После `let fixed_velocity = self.fixed_velocity;` (строка 992) добавить:

```rust
        let metronome = self.metronome;
```

и после `self.fixed_velocity = fixed_velocity;` (строка 1006):

```rust
        self.metronome = metronome;
        // Click state is transient (<30 ms) and its coefficients are
        // SR-dependent — drop it rather than preserve stale values.
        self.click_env = 0.0;
```

- [ ] **Step 5: триггер + рендер в `DrumEngine::tick`** (строки 1138-1177).

Заменить начало обработки секвенсора (строки 1142-1146) — зафиксировать
prev_step ДО tick:

```rust
        // Process sequencer
        let prev_step = self.sequencer.current_step;
        let triggers = self.sequencer.tick(self.sample_rate);
        self.step_atom.store(self.sequencer.current_step, Ordering::Relaxed);
        self.play_atom.store(self.sequencer.playing as u8, Ordering::Relaxed);
        self.rec_atom.store(self.sequencer.recording as u8, Ordering::Relaxed);

        // Metronome: the sequencer has already advanced past the step that
        // just fired, so `prev_step` is the step that sounded. Fire a click on
        // every beat boundary (step % 4 == 0), accent on step 0.
        if self.metronome && self.sequencer.playing
            && self.sequencer.current_step != prev_step
        {
            let fired = prev_step;
            if fired % 4 == 0 {
                let accent = fired == 0;
                self.click_phase = 0.0;
                self.click_phase_inc = TAU
                    * if accent { CLICK_FREQ_ACCENT } else { CLICK_FREQ_BEAT }
                    / self.sample_rate;
                self.click_env = if accent { CLICK_GAIN_ACCENT } else { CLICK_GAIN_BEAT };
                self.click_decay = decay_coeff(CLICK_LEN_S, self.sample_rate);
            }
        }
```

Рендер клика — после блока микширования голосов `if !self.sf2_mode { ... }`
(строки 1164-1174), ПЕРЕД `let vol = ...` (строка 1175), чтобы клик звучал и в
SF2-режиме:

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

Возврат (строка 1176) — клик масштабируется общим drum-Volume:

```rust
        ((out_l * vol + click * vol, out_r * vol + click * vol), sf2_triggers)
```

(`decay_coeff` уже есть в drum.rs:74, `TAU` импортирован drum.rs:5.)

- [ ] **Step 6: зелёный** —

```bash
cargo test --test metronome
# running 5 tests ... test result: ok. 5 passed
```

Если TC1/TC2 красные: проверить, что `DrumSeqPlay` ресетит step (mod.rs:2060-2063
— да), что первый event действительно step 0, и порог/выигрыши.

- [ ] **Step 7: гейты + коммит** —

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
git add src/synth/drum.rs src/synth/mod.rs tests/metronome.rs
git commit -m "feat: synthesized metronome click synced to drum sequencer BPM"
```

### Task A2: RT-безопасность метронома

**Files:**
- Modify: `tests/rt_safety.rs`

- [ ] **Step 1: failing-тест** — добавить в `tests/rt_safety.rs` (импорты уже
  есть; тест создаёт клики внутри guarded-зоны: если в клик-пути появится
  аллокация, процесс abort'нется):

```rust
#[test]
fn metronome_click_does_not_allocate() {
    // 400 BPM → beat every 7200 samples; 800 blocks = 25600 samples
    // crosses 3 beat boundaries with click triggers inside the guarded region.
    let mut engine = SynthEngine::new(SR);
    let _ = engine.handle_control(ControlEvent::DrumSetMetronome { on: true });
    let _ = engine.handle_control(ControlEvent::DrumSeqBpm { bpm: 400.0 });
    let _ = engine.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        for _ in 0..800 {
            engine.tick_block(&mut l, &mut r);
        }
    });
}
```

- [ ] **Step 2: зелёный** (реализация уже per-sample: ожидаем green сразу;
  тест страхует регрессию) —

```bash
cargo test --features rt-assert --test rt_safety
# test metronome_click_does_not_allocate ... ok
```

Если red (abort "memory allocation ..."): искать аллокацию в клик-пути —
заменять Vec/format на массивы; `decay_coeff` аллокационно чист.

- [ ] **Step 3: коммит** —

```bash
git add tests/rt_safety.rs
git commit -m "test: metronome click path is RT-safe"
```

### Task A3: GUI — галка, персист, стартовый пуш

**Files:**
- Modify: `src/config.rs` (UiSettings)
- Modify: `src/gui/mod.rs` (App-поля, init-пуш, save_global_to_config)
- Modify: `src/lib.rs` (App construction)
- Modify: `src/gui/drums.rs` (checkbox)

- [ ] **Step 1: тест уже есть** — tc5 в tests/metronome.rs (красный до Step 2).

- [ ] **Step 2: config** — `src/config.rs`, в `UiSettings` после
  `drum_fixed_velocity` (строки 103-105):

```rust
    /// Metronome click synced to the drum step sequencer.
    #[serde(default)]
    pub drum_metronome: bool,
```

В `impl Default for Config`, в `ui: UiSettings { ... }` после
`drum_fixed_velocity: false,` (строка 161):

```rust
                drum_metronome: false,
```

- [ ] **Step 3: App-поля** — `src/gui/mod.rs`, после
  `pub drum_rec_atom: ...` (строка 319):

```rust
    pub drum_metronome: bool,
    /// Previous sequencer step (downbeat-flash edge detection)
    pub drum_prev_step: u8,
    /// Instant of the last beat-1 transition (GUI flash)
    pub drum_beat_flash: Option<std::time::Instant>,
```

(`drum_prev_step`/`drum_beat_flash` используются в Task A4 — добавить сразу,
чтобы не трогать инициализацию дважды.)

- [ ] **Step 4: инициализация в lib.rs** — в литерале App после
  `drum_rec_atom: c.drum_rec_atom,` (строка 779):

```rust
        drum_metronome: c.config.ui.drum_metronome,
        drum_prev_step: 0,
        drum_beat_flash: None,
```

- [ ] **Step 5: стартовый пуш** — `src/gui/mod.rs`, `send_initial_presets`,
  после `DrumSetFixedVelocity` пуша (строка 1224):

```rust
        let _ = self.ctrl_tx.push(ControlEvent::DrumSetMetronome { on: self.drum_metronome });
```

- [ ] **Step 6: персист** — `save_global_to_config` (строки 1293-1309), после
  `drum_fixed_velocity` (строка 1300):

```rust
        self.config.ui.drum_metronome = self.drum_metronome;
```

- [ ] **Step 7: checkbox** — `src/gui/drums.rs`, transport-строка, сразу после
  V127-checkbox (строки 72-77, до закрывающей `});` горизонтальной панели на
  строке 78):

```rust
            if ui.checkbox(&mut self.drum_metronome, "Metro")
                .on_hover_text("Metronome: click on every beat while the sequencer plays (accent on beat 1)").changed()
            {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSetMetronome { on: self.drum_metronome });
                self.save_global_to_config();
            }
```

- [ ] **Step 8: зелёный + гейты** —

```bash
cargo test --test metronome   # tc5 green
cargo clippy --all-targets -- -D warnings
cargo test
```

- [ ] **Step 9: коммит** —

```bash
git add src/config.rs src/gui/mod.rs src/lib.rs src/gui/drums.rs tests/metronome.rs
git commit -m "feat: metronome GUI toggle with config persistence"
```

### Task A4: Визуал — даунбит-акцент плейхеда + вспышка beat 1

**Files:**
- Modify: `src/gui/mod.rs` (edge-детекция при синке состояния)
- Modify: `src/gui/drums.rs` (акцентный цвет step-0 плейхеда, flash-индикатор)

GUI-код автотестами не покрывается — тестовый слой уже в A1/A3 (атом step
управляет обоими). Проверка вручную после Step 3.

- [ ] **Step 1: edge-детекция** — `src/gui/mod.rs`, блок синка (строки 531-533),
  дополнить:

```rust
        // Sync drum play/rec state from engine (for MIDI-triggered changes)
        self.drum_playing = self.drum_play_atom.load(Ordering::Relaxed) != 0;
        self.drum_recording = self.drum_rec_atom.load(Ordering::Relaxed) != 0;
        // Beat-1 flash: the step atom shows the NEXT step, so step 0 has just
        // sounded when the atom transitions 0 -> 1 while playing.
        let step_now = self.drum_step_atom.load(Ordering::Relaxed);
        if self.drum_playing && self.drum_prev_step == 0 && step_now == 1 {
            self.drum_beat_flash = Some(std::time::Instant::now());
        }
        self.drum_prev_step = step_now;
```

- [ ] **Step 2: flash-индикатор** — `src/gui/drums.rs`, transport-строка,
  сразу после Rec-кнопки (строки 30-35):

```rust
            // Downbeat flash indicator (lights ~150 ms after beat 1)
            let flashing = self.drum_beat_flash
                .map(|t| t.elapsed() < std::time::Duration::from_millis(150))
                .unwrap_or(false);
            let (flash_rect, _) =
                ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(
                flash_rect.center(), 5.0,
                if flashing { egui::Color32::from_rgb(255, 190, 60) }
                else { egui::Color32::from_gray(45) },
            );
```

- [ ] **Step 3: акцент плейхеда** — там же, блок "Current step playhead"
  (строки 415-423) заменить на:

```rust
                    // Current step playhead (amber + thicker on the downbeat)
                    if is_current {
                        let downbeat = step == 0;
                        let color = if downbeat {
                            egui::Color32::from_rgb(255, 190, 60)
                        } else {
                            egui::Color32::WHITE
                        };
                        let width = if downbeat { 3.0 } else { 2.0 };
                        painter.rect_stroke(
                            rect.shrink(0.5),
                            2.0,
                            egui::Stroke::new(width, color),
                            egui::StrokeKind::Inside,
                        );
                    }
```

(`is_current` определён на строке 380: `self.drum_playing && step == current_step`.)

- [ ] **Step 4: гейты + ручная проверка** —

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

Ручная проверка: `cargo run` → вкладка Drums → галка Metro → Play на 120 BPM:
клик каждые 0.5 с, первый громче, колонка 0 обведена янтарным толще, кружок
после Rec вспыхивает на каждой доле.

- [ ] **Step 5: коммит** —

```bash
git add src/gui/mod.rs src/gui/drums.rs
git commit -m "feat: metronome downbeat accent and beat-1 flash in drum grid"
```

---

# Группа B: Init-перформанс + Reset (независима от группы A)

### Фактическое состояние (аудит 2026-10-29, master @ ccdd519) — techdebt #3 частично устарел

| Заявление techdebt #3 | Факт |
|---|---|
| «No factory Init performance exists» | Существует: `preset::init_performance()` (preset.rs:834-855) — ровно заявленная семантика: 1 часть = пресет "Init", key 0..127, sf2_mode false, `split_point: None` |
| «pad perf slots start empty» | Пэды маппятся на имена; `"(Init)"` выбирается в попапе pad-perf (settings.rs:102-107, perf_list его содержит) и загружается при нажатии пэда (gui/mod.rs:487-495 → `load_performance_by_name`, ветка `"(Init)"` gui/mod.rs:1897) |
| «compiled into the binary like FACTORY_PRESETS» | Фабричные пресеты на диск НЕ пишутся — вливаются in-memory (`load_all_patches`, preset.rs:587). Init-перф — тоже in-memory (`init_performance()`), материализация файла НЕ нужна и НЕ делается |
| «reset action restores it» | Частично: выбор `"(Init)"` в комбо Load (layers.rs:253-258) и назначение на пэд работают; явной кнопки Reset нет — её добавляет этот план |

Покрытие тестами: tests/sustain_audit.rs:306 `init_performance_exists_and_loads`
(1 часть, enabled, не SF2, 0..127, serde-раундтрип).

### Task B1: Тест связи Init-перфа с фабричным пресетом + split_point

**Files:**
- Modify: `tests/preset_roundtrip.rs`

- [ ] **Step 1: failing-тест** — добавить в `tests/preset_roundtrip.rs`:

```rust
#[test]
fn init_performance_references_existing_factory_patch() {
    let perf = preset::init_performance();
    assert_eq!(perf.split_point, None, "Init perf must not force a split point");
    assert_eq!(perf.parts.len(), 1);
    // The part references a preset by NAME — a factory rename would silently
    // fall back to patch_idx 0 (gui/mod.rs:1928) and change the Init sound.
    let patches = preset::load_factory_patches();
    assert!(patches.iter().any(|p| p.name == perf.parts[0].patch_name),
        "Init perf part references '{}', which is not a factory preset",
        perf.parts[0].patch_name);
}
```

- [ ] **Step 2: зелёный** (тест фиксировал бы дрейф; сейчас должен проходить —
  "Init" есть в FACTORY_PRESETS, preset.rs:181):

```bash
cargo test --test preset_roundtrip
# test init_performance_references_existing_factory_patch ... ok
```

Если red — Init-перф ссылается на несуществующее имя: чинить
`init_performance()` (patch_name = фактическое имя фабричного init), не тест.

- [ ] **Step 3: коммит** —

```bash
git add tests/preset_roundtrip.rs
git commit -m "test: Init performance references a real factory preset, no split"
```

### Task B2: Кнопка Reset в perf-строке Layers

**Files:**
- Modify: `src/gui/layers.rs` (кнопка рядом с Load-комбо)

`load_performance_by_name` (gui/mod.rs:1896) — приватный метод `impl App` в
модуле `gui`; `gui::layers` — потомок модуля, приватные методы ему видны
(прецедент: settings.rs использует pub(super) методы того же impl). Компилятор
покажет, если понадобится `pub(super)` — тогда сменить сигнатуру на
`pub(super) fn load_performance_by_name`.

- [ ] **Step 1: реализация** — `src/gui/layers.rs`, perf-строка (строки 216-260),
  между `ui.separator();` (строка 245) и `let mut load_perf ...` (строка 246)
  вставить:

```rust
            if ui.button("Reset").on_hover_text("Load the built-in (Init) performance: single part A1 = Init preset, full keyboard, no SF2").clicked() {
                if self.load_performance_by_name("(Init)") {
                    self.show_toast("Reset to (Init)", ToastKind::Success);
                }
            }
            ui.separator();
```

Кнопка переиспользует существующий путь загрузки (AllNotesOff + split_point +
apply-цикл + sf2-режимы), а не дублирует его.

- [ ] **Step 2: гейты + ручная проверка** —

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```

Ручная проверка: загрузить любой перф из комбо → нажать Reset → части
схлопнулись в A1=Init, полный диапазон, toast показан, `perf_name` = "(Init)"
(консольные слоты подсвечивают его как активный — `active_perf_console`,
gui/mod.rs:1890).

- [ ] **Step 3: коммит** —

```bash
git add src/gui/layers.rs
git commit -m "feat: Reset button loads the built-in (Init) performance"
```

### Task B3: Документирование — techdebt #3 закрыт

**Files:**
- Modify: `docs/TECHDEBT.md`

- [ ] **Step 1:** заменить пункт 3 (строки 14-16) на:

```markdown
## 3. Default empty init performance — РЕАЛИЗОВАНО (уточнено 2026-10-29)
Заявление было частично устаревшим: `preset::init_performance()` (одна часть
A1 = Init-пресет, полный диапазон, без SF2, split_point None) встроен в бинарь,
подаётся виртуальной записью "(Init)" в perf_list и грузится из комбо Layers,
из pad-perf-слотов и LED-консоли. Файл на диске не нужен: фабричные пресеты
тоже in-memory (load_all_patches). Добавлена кнопка Reset в perf-строке
Layers (план docs/superpowers/plans/2026-10-29-metronome-init-perf.md).
```

- [ ] **Step 2: коммит** —

```bash
git add docs/TECHDEBT.md docs/superpowers/plans/2026-10-29-metronome-init-perf.md
git commit -m "docs: techdebt #3 init perf — already implemented, Reset button added"
```

---

## Финальные гейты (после обеих групп)

```bash
cargo clippy --all-targets -- -D warnings   # clean
cargo test                                  # all green
cargo test --features rt-assert --test rt_safety   # all green, no aborts
cargo deny check                            # clean (rtrb advisory — см. techdebt carried-over, не блокер этой работы)
```

## Известные риски

1. **Тайминг TC1/TC2:** клик стартует в начале блока, содержащего сэмпл 24000 —
   границы кластеров ±1 блок; допуск ±4 блока в ассерте покрывает.
2. **click * vol:** метроном масштабируется drum-Volume (Vol=0 → тишина).
   Если юзер попросит фиксированный уровень — убрать `* vol` из возврата tick
   (одна строка, Task A1 Step 5).
3. **`load_performance_by_name` видимость из layers.rs** — приватность модуля
   `gui` покрывает потомков; если clippy/compile скажет иное — `pub(super)`.
4. **Акцент при length не кратном 4** — акцент «плывёт» относительно музыкальных
   долей; задокументировано в Negative Matrix, не чинится (consistent с GUI-сеткой
   4×4: каждая 4-я колонка и так визуально выделена).
