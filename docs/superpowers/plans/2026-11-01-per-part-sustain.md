# Per-Part Sustain Pedal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-01 (база = origin/master после #29, worktree `~/Projects/mini_midi_synth-verify`)
status: executed 2026-11-01

**Goal:** Чекбокс «Sus» в строке парта: педаль сустейна действует только на парты с включённым приёмом (off by design: «бас без сустейна, пэд с сустейном»). Плюс lift педали при загрузке перфа (анти-stuck).

**Architecture:** Движок: `Part.sustain_enabled: bool` (default true); гейт в `Part::note_off` (DSP) и per-channel CC64 в `Sampler::set_part_sustain` (SF2); `ControlEvent::SetPartSustain`. GUI: чекбокс «Sus» в парт-строке, персист по образцу root_only (parts_state + PartConfig перфа). Research 2026-11-01: отраслевой стандарт workstation (Fantom HOLD per zone / Yamaha FC receive / Nord per section).

**Tech Stack:** Rust, rustysynth (vendored, per-channel CC64 инфраструктура есть).

---

## Test Cases (from the requirement)

- TC1 (аудио-тест в src/synth/tests.rs): парт с sustain_enabled=false: note-on → CC64 on → note-off → нота ЗАМОЛЧАЛА (не висит); парт с true — висит до CC64 off. SF2-путь: минимум — маска доходит до set_part_sustain (юнит на состояние, звук вручную).
- TC2: чекбокс Sus персистится в parts_state (рестарт сохраняет) и в перфе (save/load perf сохраняет).
- TC3 (вручную): загрузка перфа при зажатой педали — ноты не залипают (SetSustain{false} в начале применения перфа).

## Negative Matrix

- zero/empty — все парты Sus=on: поведение идентично текущему (маска пуста → глобальный путь).
- duplicate/concurrent — SetPartSustain во время зажатой педали: handler при !on разжимает pedal-held ноты этого парта немедленно (явный код).
- clock skew / missing field / timeout — N/A.
- залипшая педаль при смене перфа: SetSustain{false} в начале load_performance (явный код; AllNotesOff НЕ добавлять — ломает seamless).

---

### Task 1: Движок — флаг, гейты, событие

**Files:** Modify: `src/synth/mod.rs` (Part, note_off, set_sustain, ControlEvent, handle_control), `src/synth/sampler.rs` (set_part_sustain)

1a. `Part` (mod.rs:532, рядом с root_only): `pub sustain_enabled: bool,` — `Part::new`: `sustain_enabled: true,`.

1b. Гейт DSP — `Part::note_off` (mod.rs:893): условие `if self.sustain_pedal > 0.5` → `if self.sustain_pedal > 0.5 && self.sustain_enabled`.

1c. Sampler (sampler.rs): добавить `part_sustain: [bool; 16]` (инициализация **true** — до прихода SetPartSustain поведение прежнее), метод:

```rust
    /// Per-part hold-pedal: CC64 на ОДИН melodic-канал (drums структурно
    /// недостижим — part_channel только MELODIC_CHANNELS).
    pub fn set_part_sustain(&mut self, part: usize, on: bool) {
        self.part_sustain[part] = on;
        let Some(ch) = self.part_channel_of(part) else { return; };
        let v = if on { 127 } else { 0 };
        self.keys.midi(ch as i32, 0xB0, 64, v);
    }
```

`apply_sustain` (:457) — цикл по маске вместо глобального. Старый `Sampler::sustain_pedal` (:437-443) — ЕДИНСТВЕННЫЙ вызов заменяется (mod.rs:1593) → метод удалить (иначе dead_code = ошибка при -D warnings); `apply_sustain` оставить (нужен после rebuild, читает маску).

1d. `set_sustain` (mod.rs:1593-1595): заменить `self.sampler.sustain_pedal(on)` на цикл (iter_mut — пишет `part.sustain_pedal`):

```rust
            for (i, part) in self.parts.iter_mut().enumerate() {
                part.sustain_pedal = if on { 1.0 } else { 0.0 };
                self.sampler.set_part_sustain(i, on && part.sustain_enabled);
            }
```

(сверить с фактическим телом :1594-1595 — сохранять их семантику обнуления.) Pedal-up ветка (!on, :1596-1635) без изменений — у выключенного парта `sustained_notes` пуст.

1e. `ControlEvent::SetPartSustain { part: usize, on: bool },` (рядом с SetPartRootOnly :183). Handler (рядом :2426) — ОБЕ ветки (ревью: re-enable при зажатой педали тоже):

```rust
            ControlEvent::SetPartSustain { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.sustain_enabled = on;
                    if l.sustain_pedal > 0.5 {
                        // Педаль внизу: немедленно применить маску к SF2-каналу
                        self.sampler.set_part_sustain(*part, on);
                    }
                    if !on && l.sustain_pedal > 0.5 {
                        // Разжать pedal-held ноты этого парта — паттерн
                        // pedal-up из set_sustain (:1611-1635, включая
                        // sustain_release Some-ветку и !keys_down)
                        ...тот же релизный паттерн для одного парта...
                        l.sustained_notes = [false; 128];
                    }
                }
            }
```

(`part` — перемещённый usize из деконструкции, БЕЗ `*`.)

- [ ] Step 1: реализация + TC1 аудио-тест в src/synth/tests.rs (DSP-путь; по образцу root_only-тестов)
- [ ] Step 2: clippy + `cargo test` + rt_safety
- [ ] Step 3: commit `feat: per-part sustain enable — engine gate + per-channel CC64`

### Task 2: GUI — чекбокс «Sus», персист, пуши

**Files:** Modify: `src/gui/mod.rs` (PartState, persist_parts_state, push_full_part_state, restart restore, load_performance_by_name), `src/gui/layers.rs` (чекбокс), `src/config.rs` (PartUiState), `src/preset.rs` (PartConfig)

1. `PartState` (mod.rs:159): `pub sustain_enabled: bool,` — `new_empty`: `true`.
2. layers.rs: после чекбокса «Root»:

```rust
            let mut s = self.parts[i].sustain_enabled;
            if ui.add_sized([52.0, 22.0], egui::Checkbox::new(&mut s, "Sus")).on_hover_text(
                "Sustain pedal affects this part").changed()
            {
                self.parts[i].sustain_enabled = s;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSustain { part: i, on: s });
                self.persist_parts_state();
            }
```

3. `push_full_part_state` (mod.rs:1932): рядом с SetPartRootOnly — `SetPartSustain { part, on: p.sustain_enabled }`.
4. Персист: `persist_parts_state` (mod.rs:2335) — добавить `sustain_enabled` в PartUiState (config.rs:83-87, `#[serde(default = "default_true")]`, default_true уже есть/создать); restore в `send_initial_presets`/рестарт-восстановлении (mod.rs:1566-1571) — читать и пушить SetPartSustain.
5. Перфы — ДВА пути применения (ревью), оба: (a) `load_performance_by_name` (gui/mod.rs:2351-2427; mute применяется :2398/:2407, root_only там НЕ применяется — не ориентироваться на него), (b) панельный «Load...» в `draw_perf_row` (layers.rs:243-285, save-литерал PartConfig — layers.rs:199). В ОБОИХ: применить `sustain_enabled` из PartConfig + пушить `SetPartSustain`; в начале цикла применения: `let _ = self.ctrl_tx.push(ControlEvent::SetSustain { on: false });` (анти-stuck; это НЕ AllNotesOff — seamless-хвосты целы; побочки: latch отпустится, F1-кнопка покажет OFF — приемлемо). `PartConfig` (preset.rs:743-771): `#[serde(default = "default_true")] pub sustain_enabled: bool,` — `default_true` СОЗДАТЬ (в src нет; образец default_vel_min preset.rs:764-774); struct-литералы preset.rs:829 (init_performance) и layers.rs:199 (save перфа) дополнить полем.
6. `send_full_part_state` уже покрывается через `push_full_part_state`. `default_true` для PartUiState (config.rs:83-88) — там plain `#[serde(default)]` (=false): создать fn default_true и использовать.

- [ ] Step 1: GUI + персист, clippy + `cargo test` + TC2-проверка (config.json/перф-файл содержат поле)
- [ ] Step 2: commit `feat: per-part sustain checkbox «Sus» — persist + perf integration`

### Task 3: Верификация и экран

- [ ] clippy/test/rt_safety; `cargo run --release`; экран: сплит бас(Sus off)+пэд(Sus on) — педаль держит только пэд; TC3; чекбокс переживает рестарт
- [ ] PR → squash merge → перезапуск release

## Out of scope

- Half-pedal фильтрация значений; master_stop лифтует педаль (семантика «Stop = отпустить педаль» — решить отдельно).
- #24 целиком (SF2 re-key под педалью) — гейт закрывает основную часть.
