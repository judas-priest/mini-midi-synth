# Drum Grid Recording: slot from PAD note, not remapped target

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Удары пэдов банка 1 пишутся в сетку драм-секвенсора (кубики) при ЛЮБОМ ремапе кита: слот сетки = исходная нота пэда (36-51), ремап влияет только на звук.

**Architecture:** Однострочный фикс в `src/synth/mod.rs` (handle_event, драм-ветка): сейчас `record_hit` гейтится ремапнутой нотой `(36..=51).contains(&note)` — у кита Metal_Drums_v2 ремап-таргеты `[30,38,51,48,49,52,...]` почти все ВНЕ диапазона, запись молча скипается (репорт юзера: «не пишется в кубики»). Слот вычисляется из исходной ноты пэда до ремапа. Лупер-тег 254 (пишет ремапнутую ноту) не трогается — юзер подтвердил «ничего не убирать».

**Tech Stack:** Rust, tests через SynthEngine harness (как tests/console_transport.rs), `DrumSetRemap` событие.

---

## Test Cases (from the requirement)

Requirement (юзер, 2026-10-30): «ударка должна писаться в кубики секвенсора»; сейчас пишется только вправо (лупер), кубики пустые.

1. **TC1 Ремап-кит пишется в сетку:** ремап `[30,38,51,48,49,52,51,50,48,44,43,45,52,38,38,38]` (Metal_Drums_v2), MasterRecord (arm+autoplay), удар ch9 note 36 → `patterns[current].steps[*][*]` содержит velocity>0. Сейчас FAIL (ремапнутая 30 вне 36..=51).
2. **TC2 Слот = пэд:** с тем же ремапом удар note 40 (пэд 5) пишет в СТРОКУ 4 (slot 4) сетки, независимо от ремап-таргета (48).
3. **TC3 Без ремапа (identity) — регрессия:** без DrumSetRemap удар пишет в сетку как раньше.
4. **TC4 Чужие ноты не пишутся:** ch9 note 30 (не пэд 36-51) при записи — сетка не меняется.
5. **TC5 Лупер не сломан:** существующий tests/master_transport.rs tc2 (drum hit → looper len>0) остаётся зелёным.

## Negative Matrix

- **Velocity 0 (note-off)** — запись в сетку только в ветке velocity>0 (существующий код). N/A.
- **Remap-таргет вне 36..=51** — больше не влияет на запись сетки (это и есть фикс); на ЗВУК влияет как раньше.
- **Двойной удар в один шаг** — `record_hit` перезаписывает шаг последним velocity (существующая семантика). N/A.
- **`DrumSetRemap` возвращает Some (Box — мусор)** — в тесте `let _ = e.handle_control(...)`, как требует trash-контракт. Покрыто харнессом.
- **RT-safety** — путь не трогает аллокации (запись в существующий массив). Гейт `rt_safety` прогоняется.
- **Кит без ремапа** — `remap[i] = 36+i` по умолчанию; фикс не меняет поведение (TC3).
- **Пэды вне ch9 при drum_mode=0** — путь `is_drum_note && drum_mode`; гейт записи внутри ветки — не меняется. N/A.

---

### Task 1: TDD — тест ремап-кита + фикс слота

**Files:**
- Create: `tests/drum_record_remap.rs`
- Modify: `src/synth/mod.rs` (handle_event, драм-ветка ~1895-1916)

- [ ] **Step 1: Пишущий тест (RED).** Создать `tests/drum_record_remap.rs`:

```rust
//! Drum grid recording (#22): pad hits land in the sequencer grid under ANY
//! kit remap. The grid slot comes from the PAD note (36-51); the remap table
//! only changes the SOUND. Regression: with Metal_Drums_v2-style remaps
//! (targets outside 36..=51) recording was silently skipped.

use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};

const SR: f32 = 48_000.0;
const METAL_REMAP: [u8; 16] = [30, 38, 51, 48, 49, 52, 51, 50, 48, 44, 43, 45, 52, 38, 38, 38];

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

fn grid_has_hits(e: &SynthEngine, slot: usize) -> bool {
    e.drum_engine.sequencer.patterns[e.drum_engine.sequencer.current_pattern as usize]
        .steps[slot]
        .iter()
        .any(|s| s.velocity > 0)
}

#[test]
fn tc1_remapped_kit_records_into_grid() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::DrumSetRemap { table: Box::new(METAL_REMAP) });
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm rec + auto-play
    assert!(e.drum_engine.sequencer.recording);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 36, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 36 });
    assert!(grid_has_hits(&e, 0), "pad 36 must write grid slot 0 even though remap target is 30");
}

#[test]
fn tc2_slot_follows_pad_not_remap_target() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::DrumSetRemap { table: Box::new(METAL_REMAP) });
    let _ = e.handle_control(ControlEvent::MasterRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 40 });
    assert!(grid_has_hits(&e, 4), "pad 40 -> grid slot 4 (remap target 48 must not redirect)");
    assert!(!grid_has_hits(&e, 12), "no spill into other slots");
}

#[test]
fn tc3_identity_remap_still_records() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 38, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 38 });
    assert!(grid_has_hits(&e, 2), "no-remap kit records as before");
}

#[test]
fn tc4_non_pad_notes_do_not_record() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 30, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 30 });
    let any = (0..16usize).any(|s| grid_has_hits(&e, s));
    assert!(!any, "ch9 note 30 is not a pad (36-51) — grid untouched");
}
```

Run: `cargo test --test drum_record_remap` — tc1 и tc2 FAIL (запись скипается), tc3 PASS, tc4 PASS.

- [ ] **Step 2: Фикс.** В `src/synth/mod.rs`, драм-ветка (сейчас ~1895-1916):

```rust
                    if velocity > 0 {
                        let velocity = if self.drum_engine.fixed_velocity { 127 } else { velocity };
                        let velocity = self.drum_engine.pad_velocity(note, velocity);
                        let note = self.drum_engine.remapped_note(note);
                        // Record into sequencer if recording
                        // `note` here is the REMAPPED target (30..=55 legal). Only
                        // 36..=51 maps to sequencer slots; skip recording other
                        // targets (drop, don't clamp — clamping would corrupt
                        // a neighbouring slot's pattern).
                        if is_drum_note && (36..=51).contains(&note) {
                            let slot = (note - 36) as usize;
```

заменить на:

```rust
                    if velocity > 0 {
                        let velocity = if self.drum_engine.fixed_velocity { 127 } else { velocity };
                        let velocity = self.drum_engine.pad_velocity(note, velocity);
                        // Слот сетки = исходная нота ПЭДА (36-51): ремап меняет
                        // только звук. Гейт по ремапнутой ноте молча убивал
                        // запись для китов с таргетами вне 36..=51
                        // (Metal_Drums_v2 — TECHDEBT #22b).
                        let pad_note = note;
                        let note = self.drum_engine.remapped_note(note);
                        if is_drum_note {
                            let slot = (pad_note - 36) as usize;
```

  (дальше тело `record_hit` + `feedback_tx` + триггер — без изменений; `note` после ремапа используется звуком и лупером как раньше.)

- [ ] **Step 3: GREEN + регрессия.**

```bash
cargo test --test drum_record_remap && cargo test && cargo test --features rt-assert --test rt_safety && cargo clippy --all-targets -- -D warnings
```

Все зелёные; tests/master_transport.rs (tc2 drum hit → looper) не сломан — TC5.

- [ ] **Step 4: Commit**

```bash
git add src/synth/mod.rs tests/drum_record_remap.rs
git commit -m "fix: drum grid records pad slots under any kit remap (#22b)"
```

### Task 2: TECHDEBT #22b — закрыть репорт

**Files:**
- Modify: `docs/TECHDEBT.md` (#22)

- [ ] **Step 1:** В #22 после гипотез дописать:

```markdown
РАССЛЕДОВАНО И ЗАКРЫТО (2026-10-30, план 2026-10-30-drum-grid-record-remap.md):
корень (b) — НЕ конфликт банков, а гейт записи в сетку по РЕМАПНУТОЙ ноте
`(36..=51).contains(&remapped)`: у Metal_Drums_v2 таргеты [30,38,51,48,…] вне
диапазона → record_hit молча скипался, хотя звук играл. Фикс: слот сетки =
исходная нота пэда (36-51), ремап = только звук. Тесты: tests/drum_record_remap.rs.
Лупер при этом пишет удары с тегом 254 (решение юзера: пишем и в сетку, и в
лупер, «ничего не убирать» — см. память master_transport.md).
```

- [ ] **Step 2: Commit**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: #22b closed — remap gate killed grid recording, slot now from pad note"
```

### Task 3: Финал

- [ ] **Step 1:** Полные гейты + `cargo build --release`, перезапуск приложения.

- [ ] **Step 2: Ручной чеклист (юзер):** Master Rec → удары по банку 1 → кубики в строках сетки заполняются (Kick/Snare/...), стоп → плей — сетка переигрывает удары.
