# Part Spawn Desync Fixes (#40-43) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-01 (база 8614aa4 = origin/master, worktree `~/Projects/mini_midi_synth-verify`)

**Goal:** Закрыть #40-43: спавн/включение парта не пушит полное состояние в движок (sf2_mode/pan/transpose/vel-range наследуются от прежнего жильца), LoadPatch перетирает диапазон парта, SF2-тоггл не высылает программу.

**Architecture:** GUI (`src/gui/layers.rs`, `src/gui/mod.rs`) + однострочный движковый фикс (`src/synth/mod.rs` LoadPatch). Root-cause расследование 2026-11-01: (A) remove_part → SetPartEnabled{false} не чистит слот движка; add_part_to_zone сбрасывает 5 полей из 11 и пушит 3 события; (B) LoadPatch (synth/mod.rs:2346-2347) переписывает min/max из параметров патча (0..127); (C) SF2-тоггл не пушит SetPartSf2Program — rustysynth-канал стартует с piano.

**Tech Stack:** Rust, egui. Существующий `send_full_part_state` (gui/mod.rs:1813) — образец пула; параметризуем.

---

## Test Cases (from the requirement)

Парт-спавн не собирается вне аудио-пути юнит-тестами — проверка кодом + на экране:
- TC1 (#40): use→remove→add парт: тумблер показывает DSP — движок в DSP (один клик тумблера переключает).
- TC2 (#41): включённый Split → сменить патт любого парта → зона играет по сплиту (диапазон не слетел в 0..127).
- TC3 (#43a): новый парт: лейбл патча 0 — звучит патч 0 (Init-параметры, не прежнего жильца).
- TC4 (#43b): SF2-парт, выбрать program N → тумблер DSP→SF2→DSP→SF2 — звучит program N, не piano.

## Negative Matrix

- zero/empty — свежий (никогда не использованный) слот: движок и так в дефолтах; повторный пуш идемпотентен.
- duplicate/concurrent — двойной toggle Split: каждый проход заново пушит полное состояние, идемпотентно. ОСОЗНАННОЕ ИЗМЕНЕНИЕ: выключение Split обнуляет B-слоты в движке (было), повторное включение через reset_and_push_part сбрасывает инструмент B-парта в патч 0 (раньше наследовался) — консистентно с семантикой «слот ждёт нового жильца» (текст Remove-диалога).
- clock skew / external timeout / missing field — N/A: кольцо ControlEvent.
- gcd: патч с осознанным key_low/high (performance restore) — LoadPatch-фикс меняет поведение: диапазон из ПАТЧА больше не применяется нигде; проверить `grep -n "min_note" src/synth/patch_params.rs` — если поля используются только в LoadPatch, пометить `#[allow(dead_code)]`-путь в отчёте.

---

### Task 1: Единый пуш полного состояния парта (root A)

**Files:** Modify: `src/gui/mod.rs:1813` (разбить `send_full_part_state`), Modify: `src/gui/layers.rs` (`add_part_to_zone`, toggle Split, `remove_part`)

1a. Вынести тело цикла в метод (mod.rs, рядом с `send_full_part_state`):

```rust
    /// Пуш полного состояния ОДНОГО парта в движок (enabled + все параметры).
    /// Используется при спавне/включении парта и при аудио-реконнекте.
    fn push_full_part_state(&mut self, part: usize) {
        let p = &self.parts[part];
        let (enabled, mute, volume) = (p.enabled, p.mute, p.volume);
        let (min_note, max_note) = (p.min_note, p.max_note);
        let (vel_min, vel_max) = (p.vel_min, p.vel_max);
        let (pan, transpose) = (p.pan, p.transpose);
        let (sf2_mode, sf2_program) = (p.sf2_mode, p.sf2_program);
        let root_only = p.root_only;

        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part, enabled });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part, mute });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartVolume { part, volume });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartRange { part, min_note, max_note });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange { part, vel_min, vel_max });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part, pan });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part, semitones: transpose });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part, on: root_only });
        // Явный SetPartSf2Mode в ОБЕ стороны: слот движка наследует sf2_mode
        // прежнего жильца (remove не чистит) — фикс #40 (bug: «DSP, а звучит SF2»).
        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: sf2_mode });
        for (index, value) in p.macro_vals.iter().enumerate() {
            let _ = self.ctrl_tx.push(ControlEvent::SetMacro { part, index, value: *value });
        }
        if sf2_mode {
            // Фикс #43b: rustysynth-канал стартует с program 0 — высылаем хранимый.
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                part, program: sf2_program, bank: 0,
            });
        }
    }
```

`send_full_part_state` переписать через него:

```rust
    pub(super) fn send_full_part_state(&mut self) {
        for i in 0..self.parts.len() {
            self.push_full_part_state(i);
        }
    }
```

1b. Новый спавн-метод (рядом):

```rust
    /// Сброс GUI-слота в дефолты + полный пуш в движок (фикс #40-43a:
    /// прежние add_part_to_zone сбрасывал 5 полей из 11 и не пушил
    /// sf2_mode/pan/transpose/vel_range; send_edited_params уходил БЕЗ
    /// load_edited_params — движок получал параметры прежнего жильца).
    fn reset_and_push_part(&mut self, part: usize, enabled: bool) {
        let p = &mut self.parts[part];
        p.mute = false;
        p.volume = 0.8;
        p.patch_idx = 0;
        p.sf2_mode = false;
        p.sf2_program = 0;
        p.pan = 0.0;
        p.transpose = 0;
        p.vel_min = 1;
        p.vel_max = 127;
        p.root_only = false;
        p.enabled = enabled;
        self.load_edited_params(part);   // GUI: параметры патча 0
        self.send_edited_params(part);   // Движок: реальные параметры патча 0
                                         // (фикс #43a: раньше уходили параметры
                                         // прежнего жильца или Default)
        self.push_full_part_state(part);
    }
```

1c. `add_part_to_zone` (layers.rs:450-470): заменить сброс 5 полей + `SetPartEnabled` + `send_edited_params` + `send_part_volume` на:

```rust
        if let Some(i) = range.into_iter().find(|&i| !self.parts[i].enabled) {
            self.reset_and_push_part(i, true);
            let split_on = self.parts[ZONE_B].iter().any(|p| p.enabled);
            if split_on {
                self.apply_split_ranges();
            } else {
                self.parts[i].min_note = 0;
                self.parts[i].max_note = 127;
                self.send_part_range(i);
            }
            self.set_active_part(i);
            self.show_drums = false; self.show_midi_seq = false; self.show_fx_chain = false;
        }
```

1d. Toggle Split, включение B-слота (layers.rs:66-73): заменить блок «Enable the first free B slot» на:

```rust
                } else {
                    // Enable the first free B slot — полный сброс + пуш (#40/#42)
                    if let Some(i) = ZONE_B.into_iter().find(|&i| !self.parts[i].enabled) {
                        self.reset_and_push_part(i, true);
                    }
                    self.apply_split_ranges();
                }
```

1e. `remove_part` (layers.rs:476-486): после `SetPartEnabled{false}` — явный сброс легаси-полей в движке (слот ждёт нового жильца):

```rust
    fn remove_part(&mut self, part: usize) {
        self.parts[part].enabled = false;
        self.parts[part].mute = false;
        self.parts[part].pan = 0.0;
        self.parts[part].transpose = 0;
        self.parts[part].sf2_mode = false;
        self.parts[part].root_only = false;
        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part, enabled: false });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: false });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part, pan: 0.0 });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part, semitones: 0 });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part, on: false });
        self.set_active_part(if part >= PARTS_PER_ZONE { PARTS_PER_ZONE } else { 0 });
        // find first enabled in same zone
        let zone = if part >= 4 { ZONE_B } else { ZONE_A };
        if let Some(i) = zone.into_iter().find(|&i| self.parts[i].enabled) {
            self.set_active_part(i);
        }
    }
```

- [ ] Step 1: правки, `cargo clippy --all-targets -- -D warnings` + `cargo test`
- [ ] Step 2: commit `fix: part spawn/enabled pushes full state — fixes #40/#42/#43a`

### Task 2: LoadPatch не перетирает диапазон парта (root B, #41)

**Files:** Modify: `src/synth/mod.rs:2346-2347` (LoadPatch)

Удалить строки:

```rust
                l.min_note = params.min_note;
                l.max_note = params.max_note;
```

(диапазон парта — исключительная собственность `SetPartRange`/`apply_split_ranges`. Поля `min_note/max_note` в PatchParams НЕ удалять: они в JSON round-trip и парсере пресетов; после фикса становятся parse-store-only — dead_code warning НЕ возникнет, пометки не нужны. Golden FX не мигнёт — тесты грузят только Init (0..127).)

- [ ] Step 1: правка + grep-аудит patch min/max, clippy + `cargo test` (golden FX может мигнуть — если да, это intended: диапазон больше не сбрасывается сменой патта; регенерировать как в DSP-плане)
- [ ] Step 2: commit `fix: LoadPatch no longer overwrites the part key range (#41)`

### Task 3: SF2-тоггл высылает программу (#43b)

**Files:** Modify: `src/gui/layers.rs:326-340` (тумблер DSP/SF2)

При включении SF2 после `SetPartSf2Mode{true}` пушить хранимую программу:

```rust
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: !sf2 });
                if !sf2 && self.sf2_keys_soundfont.is_some() {
                    // Канал rustysynth свежий (program 0) — выслать хранимую
                    // программу, иначе звучит piano до перевыбора (#43b)
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                        part: i, program: self.parts[i].sf2_program, bank: 0,
                    });
                }
```

- [ ] Step 1: правка, clippy + `cargo test`
- [ ] Step 2: commit `fix: SF2 toggle re-sends the stored program (#43b)`

### Task 4: Верификация и экран

- [ ] `cargo clippy --all-targets -- -D warnings`; `cargo test`; `cargo test --features rt-assert --test rt_safety`; если golden мигнул — регенерировать с пометкой «intended: range no longer reset by patch change»
- [ ] `cargo run --release`; экран: TC1 (use→remove→add: тумблер однозначен), TC2 (Split держится при смене патча), TC3 (новый парт звучит патчем 0), TC4 (SF2↔DSP↔SF2 — program сохраняется)
- [ ] PR → squash merge → обновить verify → перезапуск release

## Out of scope

- Перф-restore диапазонов (mod.rs:2256 key_low/high) — отдельный вопрос согласования с split (#41 остальное).
- Двойная обработка активного легаси-слота — S3.
