# Per-Part Ignore PitchBend / ModWheel (TECHDEBT #55) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-05 (база = master 1f41214 + #54 docs d7d6546; факты вычитаны из кода; практики — Tavily 2026-11-03: multitimbral RX-свитчи (Motif/Fantom-класс), DAW input filters (Logic), идиом bend-range-0; прецедент в коде — per-part `sustain_enabled`/Sus)

**Goal:** Парт может игнорировать Pitch Bend и Mod Wheel (per-part флаги
`ignore_pb` / `ignore_mw`) — как уже умеет игнорировать сустейн (`Sus`).
Мотивация: чужие контроллеры шлют мусорный PB/MW, слои не должны дёргаться.

**Architecture:** Два новых bool на Part (движок) + два ControlEvent
(`SetPartIgnorePb/SetPartIgnoreMw`) по образцу `SetPartSustain`. DSP: маска на
двух точках сборки pitch-offset (PB-слагаемое и MW-слагаемое в
lfo_pitch_offset_base) + в `mods.pitch_bend` (mod-matrix источник). SF2: на
форвардинг-сайтах PB/MW (парты 0..2) игнорируемый парт не получает событие;
при ВКЛЮЧЕНИИ игнора каналу немедленно сбрасываются PB/MW (центр/ноль), при
выключении — пересылаются текущие значения движка. GUI: чекбоксы PB/MW в строке
парта рядом с Sus; перф-персист через PartConfig (serde-default, канон #49).
Undo — НЕТ (флаг приёма, не звук; как Mute-чекбокс вне undo).

**Tech Stack:** Rust/egui; юнит-тесты (TDD) на хелперы/handlers/serde;
маскировка DSP-сборки и SF2-форвардинга — экранная верификация (TC5/TC6),
не юнит (внутренности tick_block).

**Проверенные факты (вычитано; не пересматривать):**
- PB: handler MidiEvent::PitchBend (synth/mod.rs:2294-2315): глобальный
  `self.pitch_bend_semitones` (asymmetric range) + форвардинг `for i in 0..2`
  `sampler.pitch_bend(i, value)` ТОЛЬКО sf2-партам (:2311-2314).
- MW: handler MidiEvent::ModWheel (mod.rs:2315-2323): `self.mod_wheel` +
  форвардинг `for i in 0..2` sf2-партам `sampler.mod_wheel(i, value)`.
- DSP-потребители: сборка `lfo_pitch_offset_base` (mod.rs:3484-3491) —
  `self.pitch_bend_semitones` + `self.mod_wheel * 0.5 * vibrato_val`;
  `mods.pitch_bend` (mod.rs:3480) — mod-matrix источник.
- Part-поля и дефолты: `sustain_enabled` :586, дефолт :634; ControlEvent
  `SetPartSustain { part, on }` handler :2572-2590 — образец (в т.ч.
  немедленный re-apply на канал).
- sampler: `pitch_bend(part, value)` (sampler.rs:562), `mod_wheel(part, value)`
  — channel-guarded (None → return).
- GUI: чекбоксы Sus в строке парта (layers.rs, draw_part_row, паттерн Mute/Sus
  чекбоксов); per-part persist-канон: PartConfig serde-default (preset.rs:742+)
  + литерал save (layers.rs draw_perf_row) + restore в apply_performance
  (mod.rs:2833-2845) — канон macro_names (#49).
- `sampler.mod_wheel/pitch_bend` НЕ вызываются для партов 2..16 (существующее
  ограничение 0..2 — вне скоупа; только добавить ignore-чек в существующий цикл).

---

## Test Cases (from the requirement)

Требование: парт с включённым игнором не реагирует на PB / MW; остальные парты
реагируют; SF2-канал игнорируемого парта сбрасывается в центр/ноль немедленно.

- TC1: `effective(ignore=true, x)` == 0.0; `effective(false, x)` == x — для
  `effective_pb` и `effective_mw` (pub(crate)-хелперы модуля synth).
- TC2: handler `SetPartIgnorePb { part: 0, on: true }` → `parts[0].ignore_pb`;
  `SetPartIgnoreMw` аналогично; часть 1 не тронута.
- TC3: serde-roundtrip PartConfig c ignore_pb/ignore_mw = true → поля живы.
- TC4: legacy PartConfig JSON без новых ключей → false/false.
- TC5 (экран): PB качает НЕ-игнорируемый парт, игнорируемый стоит; MW-вибрато
  на игнорируемом не появляется; SF2-парт с игнором PB не едет при качании
  колеса.
- TC6 (не-регрессия): без флагов поведение идентично текущему (PB/MW работают
  как раньше) — tc8-канон #53.

## Negative Matrix

- Оба флага у SF2-парта при смене SF2-режима: флаги персистятся (свойство
  парта, не движка) — при toggle движка канал пересоздаётся, обработчик
  SetPartSf2Mode (#52-фикс) ре-апплаивает volume/pan; PB/MW-состояния канала
  свежего = центр/ноль — совпадает с «игнор»? НЕТ: если флаги ВЫКЛЮЧЕНЫ, а
  колесо качнуто — новый канал получит значения при следующем событии колеса;
  до события — центр. ДокументируемоеKnown поведение (как сегодня после
  reconnect).
- Un-ignore при зажатом колесе: форвард текущего `self.mod_wheel` /
  `pitch_bend_norm` (новое поле движка-зеркала, см. Task 2) — иначе парта
  «зависает» в центре до следующего шевеления колеса.
- Part index вне 0..16 — существующий `parts.get_mut(part)` guard.
- perf со старым форматом — serde default false (TC4).
- Two PB ranges (per-patch asymmetric) — не зависит от ignore: маска на
  входе сборки offset.
- RT: все правки — булевы флаги/поля, без аллокаций.

---

### Task 1: Флаги на Part + события + хелперы + тесты (TDD)

**Files:**
- Modify: `src/synth/mod.rs` — Part поля (:586 рядом с sustain_enabled,
  дефолт :634), enum ControlEvent два варианта (рядом SetPartSustain :209-215
  область), handlers (рядом SetPartSustain :2572), хелперы + тесты в
  `src/synth/tests.rs`
- Modify: `src/synth/tests.rs`

- [ ] **Step 1: падающие тесты** — в src/synth/tests.rs добавить:

```rust
#[test]
fn tc_ignore_flags_effective_masking() {
    // pub(crate) хелперы: маска на входе сборки offset
    assert_eq!(super::effective_pb(true, 0.7), 0.0);
    assert_eq!(super::effective_pb(false, 0.7), 0.7);
    assert_eq!(super::effective_mw(true, 0.5), 0.0);
    assert_eq!(super::effective_mw(false, 0.5), 0.5);
}

#[test]
fn tc_ignore_events_set_part_fields() {
    let mut e = SynthEngine::new(48_000.0);
    e.set_patches(preset::load_factory_patches());
    let _ = e.handle_control(ControlEvent::SetPartIgnorePb { part: 0, on: true });
    let _ = e.handle_control(ControlEvent::SetPartIgnoreMw { part: 1, on: true });
    assert!(e.parts[0].ignore_pb);
    assert!(!e.parts[0].ignore_mw);
    assert!(e.parts[1].ignore_mw);
    assert!(!e.parts[1].ignore_pb);
    let _ = e.handle_control(ControlEvent::SetPartIgnorePb { part: 0, on: false });
    assert!(!e.parts[0].ignore_pb);
}
```

(импорты сверить с существующими тестами файла; `e.parts` — pub? если нет —
сравнить по образцу соседних тестов, обращающихся к внутренностям — tests.rs
внутри crate, доступ есть.)

- [ ] **Step 2: красный**

Run: `cargo test --lib tc_ignore 2>&1 | tail -5`
Expected: FAIL — полей/функций нет.

- [ ] **Step 3: реализация:**

```rust
// Part (рядом с sustain_enabled :586):
    pub ignore_pb: bool,
    pub ignore_mw: bool,
```

```rust
// Поля движка (рядом с pitch_bend_semitones :1277 / mod_wheel :1278):
    pitch_bend_norm: f32, // сырое нормализованное значение колеса -1..1 (зеркало для re-assert)
```
дефолт `pitch_bend_norm: 0.0` (литерал :1587 рядом с pitch_bend_semitones),
в PitchBend handler (:2294) первой строкой: `self.pitch_bend_norm = value;`
дефолты `ignore_pb: false, ignore_mw: false` (:634 литерал). Хелперы на
уровне модуля synth:

```rust
/// Per-part ignore: PB/MW-вклад конкретного парта (TECHDEBT #55).
pub(crate) fn effective_pb(ignore: bool, pb: f32) -> f32 { if ignore { 0.0 } else { pb } }
pub(crate) fn effective_mw(ignore: bool, mw: f32) -> f32 { if ignore { 0.0 } else { mw } }
```

ControlEvent (рядом SetPartSustain):

```rust
    SetPartIgnorePb { part: usize, on: bool },
    SetPartIgnoreMw { part: usize, on: bool },
```

Handlers (рядом SetPartSustain :2572; SF2-канальный immediate-reset —
объяснение в комментарии):

```rust
            ControlEvent::SetPartIgnorePb { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.ignore_pb = on;
                    // SF2-канал: сброс в центр немедленно; при un-ignore —
                    // СЫРОЕ нормализованное значение (sampler.pitch_bend
                    // ждёт -1..1: sampler.rs:565 (v+1)*0.5*16383; pitch_
                    // bend_semitones содержит range-масштаб — НЕ годится)
                    if l.sf2_mode && part < 2 {
                        let norm = if l.ignore_pb { 0.0 } else { self.pitch_bend_norm };
                        self.sampler.pitch_bend(part, norm);
                    }
                }
            }
            ControlEvent::SetPartIgnoreMw { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.ignore_mw = on;
                    if l.sf2_mode && part < 2 {
                        let v = if l.ignore_mw { 0.0 } else { self.mod_wheel };
                        self.sampler.mod_wheel(part, v);
                    }
                }
            }
```

(`norm * 0.5` — та же нормализация -1..1, что в mods.pitch_bend :3480;
если у sampler.pitch_bend другая семантика value — реализатор сверяет с
форвардинг-сайтом :2311 и использует ту же.)

- [ ] **Step 4: зелёный**

Run: `cargo test --lib tc_ignore 2>&1 | tail -3`
Expected: 2 passed.

- [ ] **Step 5: коммит**

```bash
git add src/synth/mod.rs src/synth/tests.rs && git commit -m "feat: per-part ignore_pb/ignore_mw flags + events + masking helpers (#55, red-tested)"
```

### Task 2: Маскировка в DSP-сборке и SF2-форвардинге

**Files:**
- Modify: `src/synth/mod.rs` — сборка offset (:3484-3491), mods.pitch_bend
  (:3480), форвардинг PB (:2311-2314) и MW (:2317-2325)

- [ ] **Step 1: DSP-сборка** — в tick_block-функции сборки offset; ОДНОВРЕМЕННО
  замаскировать mod-matrix источники: `mods.pitch_bend` (:3473) и
  `mod_sources.mod_wheel` (:3463) — иначе matrix-маршруты PB/MW обходили бы
  игнор:

```rust
            // Per-part ignore (TECHDEBT #55): маска PB/MW-вкладов
            let pb_eff = effective_pb(part.ignore_pb, self.pitch_bend_semitones);
            let mw_eff = effective_mw(part.ignore_mw, self.mod_wheel);
            let lfo_pitch_offset_base = pb_eff
                + lfo1_val * part.params.lfo_pitch_depth * 2.0
                + lfo2_val * part.params.lfo2_pitch_depth * 2.0
                + mw_eff * 0.5 * vibrato_val
                + self.aftertouch_smooth * 0.3 * lfo1_val
                + seq_pitch;
```

`mods.pitch_bend` (:3473): `pitch_bend: effective_pb(part.ignore_pb, self.pitch_bend_semitones) / 2.0,`;
`mod_sources.mod_wheel` (:3463): `mod_wheel: effective_mw(part.ignore_mw, self.mod_wheel),`
(биндинг `part` — существующий цикл :3451+).

- [ ] **Step 2: SF2-форвардинг** — в обоих циклах `for i in 0..2` добавить
  ignore-чек:

```rust
                for i in 0..2 {
                    if self.parts.get(i).map(|l| l.sf2_mode && !l.ignore_pb).unwrap_or(false) {
                        self.sampler.pitch_bend(i, value);
                    }
                }
```

(MW — аналогично с `!l.ignore_mw`.)

- [ ] **Step 3: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --lib 2>&1 | tail -3`
Expected: чисто, зелёные.

- [ ] **Step 4: коммит**

```bash
git add src/synth/mod.rs && git commit -m "feat: ignore_pb/ignore_mw mask DSP pitch-offset and SF2 forwarding (#55)"
```

### Task 3: GUI-чекбоксы + перф-персист

**Files:**
- Modify: `src/gui/layers.rs` — draw_part_row чекбоксы (рядом Sus), литерал
  PartConfig в draw_perf_row (:199-220)
- Modify: `src/preset.rs` — PartConfig два поля (после sustain_enabled, канон
  macro_names):
  `#[serde(default, skip_serializing_if = "bool_is_false")] pub ignore_pb: bool,`
  `#[serde(default, skip_serializing_if = "bool_is_false")] pub ignore_mw: bool,`
  (`fn bool_is_false(v: &bool) -> bool { !**v }` рядом с существующими
  хелперами preset.rs:801-806 — файловый канон)
- Modify: `src/gui/mod.rs` — apply_performance restore (рядом с macro-restore
  :2846-2850; SetPartSustain-пуш :2872), push_full_part_state (:2344-2375),
  GUI Part struct (~:160-200) + дефолты (:192), reset_and_push_part (:2381),
  PartUiState persist/restore (gui/mod.rs:2725 / :1928-1936)
- Modify: `src/config.rs` — PartUiState два serde-default поля (:90-104)
- Modify: `src/gui/layers.rs` — draw_perf_row PartConfig литерал (:220-233),
  чекбоксы в draw_part_row (рядом Sus)
- Test: `tests/perf_load_seq.rs` — TC3/TC4; литералы в tests/sustain_audit.rs

- [ ] **Step 1: падающие serde-тесты** (канон macro_names-тестов):

```rust
#[test]
fn ignore_flags_roundtrip() {
    let mut p = part("Init", true);
    p.ignore_pb = true;
    p.ignore_mw = true;
    let json = serde_json::to_string(&p).unwrap();
    let back: PartConfig = serde_json::from_str(&json).unwrap();
    assert!(back.ignore_pb && back.ignore_mw);
}

#[test]
fn legacy_part_without_ignore_flags_gets_defaults() {
    let legacy = r#"{"patch_name":"Init","enabled":true,"volume":0.8,"key_low":0,"key_high":127}"#;
    let p: PartConfig = serde_json::from_str(legacy).unwrap();
    assert!(!p.ignore_pb && !p.ignore_mw);
}
```

- [ ] **Step 2: красный → поля PartConfig → зелёный** (все 4 литерала
  дополнить `ignore_pb: false, ignore_mw: false,`; компилятор перечислит).

- [ ] **Step 3: чекбоксы в строке парта** — рядом с Sus (draw_part_row,
  паттерн существующего Sus-чекбокса: `ui.checkbox(&mut self.parts[i].ignore_pb, "PB")` в
  том же horizontal, с hover-текстом «Ignore pitch bend for this part» /
  «Ignore mod wheel»; изменение → пуш `ControlEvent::SetPartIgnorePb/
  SetPartIgnoreMw` + `self.persist_parts_state()` — флаг персистится в
  PartUiState (см. Step 4, симметрия с Sus).

- [ ] **Step 4: перф save/restore + reset + reconnect:**
  - литерал draw_perf_row (layers.rs:220-233): `ignore_pb: l.ignore_pb, ignore_mw: l.ignore_mw,`;
  - apply_performance: восстановить оба поля И запушить события
    `SetPartIgnorePb/SetPartIgnoreMw` (рядом с SetPartSustain-пушем :2872);
  - **push_full_part_state** (gui/mod.rs:2344-2375, спавн + audio-reconnect:
    добавить пуш обоих событий рядом с SetPartSustain :2362 — иначе флаги
    молча откатывались бы в false при реконнекте (класс stale-бага #40);
  - GUI Part struct (gui/mod.rs ~:160-200): поля `ignore_pb/ignore_mw` +
    дефолты false в литерале :192 (компилятор перечислит);
  - reset_and_push_part (gui/mod.rs:2381): поля false + пуш событий;
  - **PartUiState** (config.rs:90-104): добавить оба флага с
    `#[serde(default)]` + persist_parts_state (gui/mod.rs:2725) и restore
    (gui/mod.rs:1928-1936) — СИММЕТРИЯ с Sus (иначе флаг живёт только в
    перфе, а Sus/mute переживают рестарт без Save).

- [ ] **Step 5: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`
Expected: чисто; perf_load_seq оба новых теста зелёные; всё остальное зелёное.

- [ ] **Step 6: коммит**

```bash
git add src/gui/layers.rs src/gui/mod.rs src/preset.rs tests/perf_load_seq.rs tests/sustain_audit.rs
git commit -m "feat: per-part PB/MW ignore — checkboxes in part row, perf persistence, engine reset on spawn (#55)"
```

### Task 4: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): слой-парт с PB-игнором стоит при качании колеса, соседний
  едет; MW-вибрато не появляется на игнорируемом; SF2-парт с PB-игнором не
  едет; флаги сохраняются в перфе (Save → reload)
- [ ] TECHDEBT #55: пометить ГОТОВО

## Out of scope

- Форвардинг PB/MW на SF2-партах 2..16 (существующее ограничение `for i in 0..2`;
  игноры корректно маскируют в тех же границах).
- Undo для флагов — приём-фильтры, вне undo-домена (как Mute).
- Aftertouch/breath/expression игноры — отдельные флаги по запросу.
