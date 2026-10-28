# Sustain pedal-up: keys_down fix Implementation Plan

anchor-checked: 2026-10-28 (независимое ревью на master @ 329faac; находки вшиты)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix High-регрессию #1 из docs/deep-review-2026-10-28-audit-remediation.md — pedal-up больше не обрывает ноты с зажатыми клавишами.

**Architecture:** Per-part массив `keys_down: [bool; 128]` (raw-нота → клавиша физически зажата), обновляется в `note_on`/`note_off`. `set_sustain` при pedal-up шлёт note_off только голосам, чья клавиша отпущена (`!keys_down[src_note]`); очистка `sustained_notes`/`note_stack`/`latch_held` сохраняется как есть. Индексация — raw-нотой, матчинг голоса — по `voice.src_note` (существующая конвенция).

**Tech Stack:** Rust, существующий тест-харнесс `tests/sustain_audit.rs` (render_peak/settle/pedal helpers).

**Обязательные проверки перед каждым коммитом** (см. CLAUDE.md):

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требование: «pedal-up отпускает только ноты, чей key-off уже пришёл; ноты с зажатыми клавишами продолжают звучать и освобождаются своим key-off». Тесты пишет FRESH subagent, получивший ТОЛЬКО этот план.

Дискриминатор: после pedal-up с зажатой клавишей сначала `settle()` (6000 блоков ≈ 6 с > release init-пресета 0.3 с) и ТОГДА сравнивать пик — убитый голос затухает ниже SILENCE, живой держится на sustain-уровне. Пик сразу после pedal-up (окно 20 блоков ≈ 21 мс) не различает — там хвост release ~0.94 амплитуды.

1. `held_key_survives_pedal_up` — NoteOn 60 → pedal-up (клавиша зажата!) → settle → нота звучит → NoteOff 60 → settle → тишина.
2. `repedal_articulation_keeps_held_key` — NoteOn 60 → pedal down/up/down/up (re-pedal, клавиша зажата всю дорогу) → settle → нота звучит → NoteOff → settle → тишина.
3. `released_key_still_released_by_pedal_up` — существующее поведение не сломано: pedal-down → NoteOn 60 → NoteOff 60 (pedal-held) → pedal-up → тишина (это `pedal_up_releases_held_notes`, уже есть — остаётся зелёным).
4. `multi_held_keys_pedal_up_then_individual_offs` — NoteOn 48, NoteOn 60 (обе зажаты), pedal-up → settle → обе звучат → NoteOff 48 → settle → 60 звучит → NoteOff 60 → settle → тишина.
5. `panic_after_held_pedal_up_kills_everything` — guard-тест: NoteOn 60, pedal-up с зажатой клавишей, CC123 → тишина, повторный pedal-цикл не воскрешает.
6. `duplicate_note_on_then_pedal_up_releases_once` — guard-тест: двойной NoteOn 60 → pedal-up → NoteOff → тишина.

## Negative Matrix

- ~~missing field~~ — нет serde/внешнего формата.
- null — нет Option в изменяемых структурах.
- zero/empty — `keys_down` изначально весь `false`: pedal-up на пустом движке не паникует (preliminary assert в тесте 1).
- clock skew — неприменимо (нет часов).
- duplicate/concurrent — повторный NoteOn той же ноты (retrigger) при зажатой педали: `keys_down[n] = true` идемпотентен (тест 6).
- latch-режим — pedal-up при зажатой latch-клавише: latch-голоса отпускаются безусловно (как и сейчас), иначе физический key-off по пустому `latch_held` оставляет вечный дрон (см. Task 3).
- DSP vs SF2 — `keys_down` касается только DSP-голосов `part.voices`; SF2-ноты ведёт rustysynth через `sampler.sustain_pedal` (mod.rs:1377) — семантика pedal-up там уже корректна, план её не трогает.
- sample-rate change — `Part::set_sample_rate` (mod.rs:871) обновляет только voices/lfo/mseg, структуру не пересоздаёт → `keys_down` переживает смену SR; зажатая клавиша остаётся защищённой после JACK SR-change.
- external-call timeout — неприменимо (нет I/O).

---

### Task 1: Regression tests (fresh subagent, plan-only context)

**Files:**
- Modify: `tests/sustain_audit.rs`

- [ ] **Step 1: Write the failing tests**

Добавить в конец `tests/sustain_audit.rs` (helpers `engine_with_init_patch`, `pedal_down`, `pedal_up`, `render_peak`, `settle` уже есть в файле):

```rust
#[test]
fn held_key_survives_pedal_up() {
    let mut e = engine_with_init_patch();
    // pedal-up on an idle engine must not panic
    pedal_up(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    // pedal-up while the KEY IS STILL HELD: note must keep sounding.
    // Discriminator: settle first — a killed voice decays below SILENCE
    // (release 0.3s), a held one stays at sustain level forever.
    pedal_up(&mut e);
    settle(&mut e);
    let held = render_peak(&mut e, 200);
    assert!(held > SILENCE, "pedal-up killed a held key (peak {held})");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "key-off after pedal-up left ringing (peak {after})");
}

#[test]
fn repedal_articulation_keeps_held_key() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    // re-pedal: down, up, down, up — key held the whole time
    pedal_down(&mut e);
    pedal_up(&mut e);
    pedal_down(&mut e);
    pedal_up(&mut e);
    settle(&mut e);
    let held = render_peak(&mut e, 200);
    assert!(held > SILENCE, "re-pedal killed a held key (peak {held})");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE);
}

#[test]
fn multi_held_keys_pedal_up_then_individual_offs() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 200) > SILENCE, "pedal-up killed held keys");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 48 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) > SILENCE, "note 60 should still sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE);
}

#[test]
fn duplicate_note_on_then_pedal_up_releases_once() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    pedal_up(&mut e);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE, "duplicate note-on left ringing");
}

#[test]
fn panic_after_held_pedal_up_kills_all() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    pedal_up(&mut e);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE, "CC123 left ringing");
    // a later pedal cycle must not resurrect anything
    pedal_down(&mut e);
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE, "pedal cycle resurrected a note");
}
```

- [ ] **Step 2: Run, verify red/green split**

Run: `cargo test --test sustain_audit -- --nocapture`
Expected: RED — `held_key_survives_pedal_up`, `repedal_articulation_keeps_held_key`, `multi_held_keys_pedal_up_then_individual_offs` (падают на «pedal-up killed a held key»). GREEN — `duplicate_note_on_then_pedal_up_releases_once` и `panic_after_held_pedal_up_kills_all` (guard-тесты: паника/panic-CC убивают голоса независимо от keys_down) и все 22 существующих теста. Это ожидаемое разделение, не ошибка.

- [ ] **Step 3: Commit (test-only, red)**

```bash
git add tests/sustain_audit.rs
git commit -m "test: held-key-through-pedal-up regression cases (red)"
```

### Task 2: `keys_down` field + note_on/note_off wiring

**Files:**
- Modify: `src/synth/mod.rs` (Part struct ~line 460, Part::new ~line 503, note_on ~line 568, note_off ~line 779)

- [ ] **Step 1: Add the field**

В struct `Part` после строки `sustained_notes: [bool; 128], // notes held by sustain pedal` (mod.rs:460):

```rust
    keys_down: [bool; 128],   // raw key physically held (pedal-up must spare these)
```

В `Part::new` после `sustained_notes: [false; 128],` (mod.rs:503):

```rust
            keys_down: [false; 128],
```

- [ ] **Step 2: Set/clear in note_on / note_off**

В `note_on` (mod.rs:568) после range-check `if note < self.min_note || note > self.max_note { return; }` (мод.rs:571) добавить:

```rust
        debug_assert!(raw_note < 128);
        self.keys_down[raw_note as usize] = true;
```

В `note_off` (mod.rs:779) в самом начале функции, ПЕРВОЙ строкой (до latch early-return на :784 — физический key-off должен снять флаг во всех режимах):

```rust
        self.keys_down[raw_note as usize] = false;
```

В `note_on`, в latch-ветке, в branch снятия лэтча (mod.rs:588-591) добавить снятие флага:

```rust
            if let Some(pos) = self.latch_held.iter().position(|&n| n == raw_note) {
                self.latch_held.remove(pos);
                self.keys_down[raw_note as usize] = false;
                for v in &mut self.voices { if v.active && v.src_note == raw_note { v.note_off(); } }
```

- [ ] **Step 3: Build + run the new tests**

Run: `cargo test --test sustain_audit`
Expected: компиляция зелёная; split как в Task 1 Step 2 (три regression-теста ещё красные — set_sustain ещё не изменён).

### Task 3: Conditional release in `set_sustain`

**Files:**
- Modify: `src/synth/mod.rs:1369-1393`

- [ ] **Step 1: Replace the unconditional sweep**

Заменить тело pedal-up ветки в `set_sustain` и обновить doc-комментарий:

```rust
    /// Apply sustain pedal state (CC64 from pedal/fader, or GUI toggle).
    /// On release, voices whose key is STILL HELD (`keys_down`) keep sounding —
    /// their own key-off will release them. Voices held by the pedal only
    /// (`sustained_notes`) and stale mono/latch entries are released/cleared;
    /// stale flags from a patch switch while the pedal was down would otherwise
    /// leave notes ringing forever.
    fn set_sustain(&mut self, value: u8) {
        let on = value >= 64;
        self.sustain.store(on, std::sync::atomic::Ordering::Relaxed);
        // SF2 voices: rustysynth applies hold-pedal per channel itself
        self.sampler.sustain_pedal(on);
        for part in &mut self.parts {
            part.sustain_pedal = if on { 1.0 } else { 0.0 };
            if !on {
                // Latch voices are released unconditionally (as before): a
                // latched note is defined by latch_held, not by a held key —
                // clearing latch_held below without releasing its keys_down
                // would leave a voice that its own key-off can never stop
                // (latch note_off early-returns).
                for &n in &part.latch_held {
                    part.keys_down[n as usize] = false;
                }
                for voice in &mut part.voices {
                    if voice.active && !voice.is_releasing()
                        && !part.keys_down[voice.src_note as usize]
                    {
                        voice.note_off();
                    }
                }
                part.sustained_notes = [false; 128];
                // Pedal-only holds are gone; stale mono/latch entries would
                // resurrect on the next key press (mono retrigger).
                part.note_stack.clear();
                part.latch_held.clear();
            }
        }
    }
```

(Mono корректен и без доп. правок: pedal-up чистит `note_stack`; при физическом key-off по пустому стеку голос отпускается по `src_note`, mod.rs:797-800.)

- [ ] **Step 2: Run sustain suite**

Run: `cargo test --test sustain_audit`
Expected: PASS — все 22 существующих + 5 новых тестов зелёные. Если `pedal_up_releases_held_notes` сломался — проверь, что в тесте клавиша отпущена ДО pedal-up (`keys_down[60] == false` → голос отпускается).

- [ ] **Step 3: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: pedal-up spares voices with keys still held (keys_down)"
```

### Task 4: Clear `keys_down` in panic/reset paths

**Files:**
- Modify: `src/synth/mod.rs` — 7 мест, где чистятся `sustained_notes`/`note_stack`/`latch_held`

- [ ] **Step 1: Add the clear next to every sustained_notes reset**

Grep-якоря (номера строк могут сдвинуться на ±3 после Task 3): `sustained_notes = [false; 128]` встречается на mod.rs:1722, 1778, 1865, 1929, 1952, 2129, 2172 (не в `set_sustain` — там чистить нельзя, клавиши ещё держатся; блок :1937-1938 — второй cleanup в том же SetPartEnabled-пути, отдельной чистки не требует). В каждом из 7 мест добавить рядом (после `sustained_notes`):

```rust
l.keys_down = [false; 128];
```

ВНИМАНИЕ на переменную цикла: места биндят по-разному. `part` (не `l`): mod.rs:1778 (panic CC120/123) и mod.rs:2129 (AllNotesOff) — там писать `part.keys_down = [false; 128];`. В остальных — `l.keys_down = [false; 128];`. Дословное применение `l.` к :1778/:2129 не скомпилируется.

Линт-проверка: после правки `grep -n "keys_down = \[false" src/synth/mod.rs` должен показать 7 строк + 1 в `Part::new` (`keys_down: [false; 128],` — другая форма, не считается).

- [ ] **Step 2: Run full gates**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```
Expected: всё зелёное (86+ тестов; rt_safety 4/4 — keys_down это фиксированный массив, аллокаций нет).

- [ ] **Step 3: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: clear keys_down on panic/reset paths"
```
