# Background CPU Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-02 (база = origin/master после #51, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** Снизить фоновое потребление CPU GUI: ×6 утечка repaint при включённом pitch-seq, idle 10 fps → 4 fps, осциллограф при тишине, покадровые клоны имён патчей, pactl-fork из GUI-потока.

**Architecture:** Все правки — ритм repaint и guard-ветки (research 2026-11-02 подтверждён с уточнениями: egui сам репейнтит на mouse/key input — интервалы безопасно увеличивать; атомики аудио НЕ input — их изменения при idle-интервале отображаются с задержкой до 250 мс, принятый компромисс; выгода freeze — пропуск рисования, не «статичная линия»; fork в GUI-потоке дорог на большом RSS — вынести в фоновый поток).

**Tech Stack:** Rust, egui 0.31.1.

---

## Test Cases (from the requirement)

GUI-runtime — юнит-тестов мало; проверка экраном (Task 5). Один юнит-тест: TC-S (ScopeBuffer: gate по амплитуде с гистерезисом не теряет последнюю позицию записи).

- TC-S: тишина → push пропущен (write pos не растёт); всплеск → push возобновился (гистерезис, без мерцания).
- Pitch-seq включён + drum-транспорт остановлен — огонёк продолжает шагать (seq независим; repaint = step rate).
- pactl отсутствует на Linux — поток молча продолжает цикл (Err → continue).

## Negative Matrix

- MIDI-нота при idle-интервале 250 мс — индикатор клавиатуры отобразится с задержкой ≤250 мс: принятый компромисс (юзер одобрил).
- Pitch-seq играет → repaint 33 мс как раньше (гейт по playing).
- pactl отсутствует (не Linux) — thread-гейт по cfg(target_os = "linux").
- duplicate/concurrent — pactl-thread один; GUI читает последнее значение. Остальное N/A (GUI-ритм).

---

### Task 1: Pitch-seq — bounded repaint (ревью: pitch-seq играет НЕЗАВИСИМО от drum-транспорта)

**Files:** Modify: `src/gui/drums.rs` (~:918-921)

⚠ РЕВЬЮ-CRITICAL: pitch-seq — самостоятельный секвенсор (step_seq.rs, свой фазовый аккумулятор, тикает всегда при enabled) — гейт по `drum_playing` ЛОМАЕТ бегущий огонёк. Огонёк — пошаговая подсветка: частота обновления = длительность шага. Заменить безусловный `request_repaint()` (неограниченная частота) на `request_repaint_after(длительность шага)`:

```rust
            // Огонёк — пошаговый: repaint на смене шага, не каждый кадр.
            // Шаг = 1/16: 60000/(bpm*4) мс. Неограниченный request_repaint()
            // гонял GUI на 60+ fps даже в паузе.
            let step_ms = (60000.0 / (self.drum_bpm.max(1.0) * 4.0)).max(20.0);
            ui.ctx().request_repaint_after(Duration::from_millis(step_ms as u64));
```

(Duration импортирован в drums.rs? — сверить; при отсутствии `crate::gui::Duration` или полный путь `std::time::Duration`.)

Семантика: огонёк прыгает по шагам как сейчас (рисование по current_step из атомика; атомик меняется движком независимо от GUI-ритма — между repaint'ами шаг мог перескочить, подсветка догонит на следующем кадре ≤20+ мс). CPU: bounded ~30 fps вместо неограниченного; idle-вклад ( enabled всегда когда вкладка видна и seq включён — это цена живого огонька, осознанно).

- [ ] Step 1: правка, clippy + cargo test + экран (огонёк шагает ровно)
- [ ] Step 2: commit `perf: pitch sequencer repaints at step rate (bounded, was unbounded)`

### Task 2: Idle-интервал 100 → 250 мс

**Files:** Modify: `src/gui/mod.rs` (~:622)

```rust
        } else {
            // Полная тишина: интерфейс статичен; egui сам репейнтит на мышь/клаву.
            // 250 мс — компромисс: индикаторы клавиатуры из аудио-атомиков
            // отображаются с задержкой до 250 мс (принято юзером 2026-11-02).
            ctx.request_repaint_after(Duration::from_millis(250));
        }
```

(заменить `request_repaint_after(Duration::from_millis(100))`; 33-мс активная ветка без изменений.)

- [ ] Step 1: правка, clippy
- [ ] Step 2: commit `perf: idle repaint interval 100 → 250 ms`

### Task 3: Осциллограф — freeze при тишине

**Files:**
- Modify: `src/synth/scope.rs` ИЛИ место определения `ScopeBuffer` (найти: `grep -rn "struct ScopeBuffer" src/`) — gate пуша
- Modify: `src/gui/mod.rs` — skip рисования при неизменном write-индексе

3a. Аудио-сторона (push, synth/mod.rs:3826): гистерезис-состояние — в SynthEngine рядом с `scope_write` (mod.rs:1352), НЕ в ScopeBuffer (Arc через потоки, push(&self) — Cell/bool не Sync-безопасно):

```rust
    /// Freeze-гистерезис осциллографа: true = тишина (пуш пропускается)
    scope_frozen: bool,
```

На push (synth/mod.rs:3826): если `sample.abs() < 1e-4 && self.scope_frozen` — пропустить пуш (write-индекс замирает — цель); если `sample.abs() > 1e-4` — `scope_frozen = false`, пуш; иначе (был сигнал, стал тихим) — запушить этот тихий сэмпл и выставить `scope_frozen = true` (линия доходит до нуля и замирает — без мерцания).

3b. GUI (mod.rs:1245-1270): `scope_write` — уже pub AtomicUsize (ревью: accessor не нужен, читать атомик); если значение не изменилось с прошлого кадра (`self.last_scope_write: usize` в App + lib.rs init 0) — ранний return до чтения 256 атомиков и построения Vec; иначе обновить `last_scope_write` и рисовать как сейчас.

TC-S (unit, ScopeBuffer + состояние гейта вынесено так, чтобы тест мог гнать): тишина → pos стоит; сигнал → идёт; пограничная амплитуда — без мерцания (гистерезис).

- [ ] Step 1: аудио-gate + accessor, TC-S тест на ScopeBuffer (тишина → pos стоит; сигнал → идёт, без мерцания при пограничной амплитуде)
- [ ] Step 2: GUI skip, clippy + cargo test
- [ ] Step 3: commit `perf: oscilloscope freezes at silence (no per-frame push/draw)`

### Task 4: Убрать покадровый clone имён патчей

**Files:** Modify: `src/gui/layers.rs` (~:334-337)

Было (каждая DSP-строка парта, каждый кадр):

```rust
                let patch_names: Vec<String> =
                    self.patches.iter().map(|p| p.name.clone()).collect();
                let cur_idx = self.parts[i].patch_idx;
                let cur = patch_names.get(cur_idx).cloned().unwrap_or_else(|| "---".into());
```

Стало (нужно только имя текущего патча):

```rust
                let cur = self.patches.get(self.parts[i].patch_idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "---".into());
```

(комментарий выше по коду про копирование имён — обновить/удалить, он протухнет.)

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `perf: no per-frame clone of all patch names in part rows`

### Task 5: pactl poll → фоновый поток (ревью: полный скоуп удаления)

**Files:**
- Modify: `src/gui/mod.rs` — УДАЛИТЬ: блок poll (:634-662 целиком, включая audio_poll_at-шедулинг :635-636), поле `audio_sink_name` (:572), init `audio_sink_name: None` (lib.rs ~:924)
- Modify: `src/gui/settings.rs:723` — `self.audio_sink_name = None;` (сломается компиляцией — удалить строку)
- Modify: `src/lib.rs` — спавн watch-потока при создании аудио (рядом с `audio_disconnected`, desktop+linux cfg)
- Также: константа `AUDIO_POLL_INTERVAL` (gui/mod.rs:40) и поле `audio_poll_at` (:569) — удалить (мёртвые после ухода poll-блока)

В `run_gui` (lib.rs, после создания аудио-хендла, где доступен `c.audio_disconnected`):

```rust
    // #18-резидуал: watch default-sink в фоновом потоке (fork в GUI-потоке
    // дорог на большом RSS; pactl сам по себе 10-50 мс). Смена sink =
    // audio_disconnected → штатный reconnect-цикл.
    #[cfg(all(not(target_os = "android"), target_os = "linux"))]
    {
        let disconnected = c.audio_disconnected.clone();
        std::thread::Builder::new()
            .name("audio-sink-watch".into())
            .spawn(move || {
                let mut last: Option<String> = None;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let out = std::process::Command::new("pactl")
                        .args(["get-default-sink"])
                        .output();
                    if let Ok(out) = out {
                        if out.status.success() {
                            let cur = String::from_utf8_lossy(&out.stdout).trim().to_string();
                            if let Some(prev) = &last {
                                if *prev != cur {
                                    log::info!("[audio] default sink changed: {prev:?} -> {cur:?}");
                                    disconnected.store(true, Ordering::Relaxed);
                                }
                            }
                            last = Some(cur);
                        }
                    }
                }
            })
            .expect("spawn audio-sink-watch");
    }
```

(допиши `use std::sync::atomic::Ordering;` если не в скоупе; `out` вынести: `let out = Command...output()` перед `if ok`.) GUI: удалить блок poll (gui/mod.rs:643-646), поле `audio_sink_name` и его init — больше не нужны.

- [ ] Step 1: правки, clippy + cargo test + rt_safety
- [ ] Step 2: commit `perf: pactl sink watch moves to a background thread`

### Task 6: Верификация

- [ ] clippy/test/rt_safety; `cargo run --release`
- [ ] Экран: idle CPU (top) снизился; pitch-seq: включён и молчит → CPU как idle; играет → 33 мс ритм; осциллограф живёт при сигнале, замирает в тишине; звук не изменился
- [ ] PR → squash merge → перезапуск release

## Out of scope

- Drum grid 256 клеток → один painter-пасс (большой выигрыш, переписывание ввода) — отдельный план при необходимости.
- hover format!-аллокации — после пп. 1-3 замерить, чинить по остатку.
- libpulse event-based watch — длинная замена pactl-poll (TECHDEBT 18а).
