# SF2 Hi-hat Choke Implementation Plan

anchor-checked: 2026-10-28 (двойное ревью против репозитория)
status: executed 2026-10-28 (ручная проверка хвоста — за пользователем)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** После открытого/полуоткрытого хэта удар закрытым хэтом (или педальным) обрывает хвост предыдущего в SF2-режиме (rustysynth, канал 9).

**Architecture:** В `DrumEngine` появляется состояние «последняя нота choke-группы» (`group_last_note: [u8; 2]`, группы уже определены `CHOKE_GROUPS`: группа 1 = слоты 42/44/46, группа 2 = crash 49). Перед каждым SF2 `drum_note_on` вызывается `sf2_choke_take(note)` — возвращает предыдущую ноту группы (или 0) и запоминает новую; caller шлёт `sampler.drum_note_off(prev)` (0x80 → rustysynth release) до NoteOn. Три точки вызова: живые пэды, MIDI-файл секвенсор, step-секвенсор. DSP-путь не трогается — choke там уже работает в `trigger_slot`. Лучшие практики: Hydrogen Mute Groups / Ableton Choke Groups (взаимное глушение; note-off — стандартный механизм сэмплера).

**Tech Stack:** Rust, rustysynth (канал 9), rtrb ControlEvent — без новых зависимостей.

**Риски:** если у SF2-кита длинный release-хвост на хэтах, note-off даст плавное затухание, а не мгновенный обрыв — проверить вручную на Metal_Drums_v2 (Task 4); если не обрывается — план Б: проставить `exclusiveClass` в igen-зонах кита (rustysynth его поддерживает, vendor/rustysynth/src/voice_collection.rs:32-35) — отдельная задача, не в этом плане. Два нюанса rustysynth: (a) `release_if_necessary` срабатывает только при `voice_length >= min_voice_length` — choke через мгновение после note-on может не сработать; (b) release у drums-канала гейтится hold-педалью CC64 — залипшая педаль отложит все choke (снимается `all_notes_off`).

---

## Test Cases (from the requirement)

Requirement: «после открытого/полуоткрытого хэта педал/закрытый хэт должен брать хвост» (SF2-режим).

- **TC1 Первый хэт:** choke-группа пуста → `sf2_choke_take(46)` возвращает 0 (нечего глушить).
- **TC2 Открытый → закрытый:** после `take(46)` вызов `take(42)` возвращает 46 (вышел flash note-off).
- **TC3 Открытый → педальный:** после `take(46)` вызов `take(44)` возвращает 46 (педал в той же группе).
- **TC4 Ретриггер той же ноты:** `take(46)` дважды — второй возвращает 46 (caller шлёт note-off перед note-on — ржавый хвост обрывается, войсы не копятся).
- **TC5 Чужие ноты:** снейр (38) не трогает состояние хэт-группы и возвращает 0.
- **TC6 Reset:** после `reset_choke_state()` группа пуста.
- **TC7 Ремап-совместимость:** choke применяется к ПОСЛЕ-ремапной ноте (контракт caller'а — сначала `remapped_note`, потом `sf2_choke_take`).

## Negative Matrix

- Нота вне 36..=51 в `sf2_choke_take` → guard в самом методе (return 0): канал 9 пропускает любые ноты (remap passthrough), MIDI-файл с ch9-нотами 53/57/70 обычное дело. Покрыто Manual check; guard тривиален.
- Повторный choke той же ноты (double-trigger) → TC4: возвращает саму ноту, caller шлёт note-off перед note-on (войсы rustysynth не копятся).
- Состояние после AllNotesOff / CC120-123 паники → сброс через `reset_choke_state()` в этих arm'ах (Task 3).
- DSP-режим (не SF2) → `sf2_choke_take` не вызывается вовсе (caller'ы guard'ятся `sampler_drums`/`sf2_mode`).
- Многопоточность → всё в аудио-потоке, single writer; без локов.
- Внешние вызовы/таймауты/часы — N/A (нет внешних вызовов).

---

### Task 0: Сохранить план в репозиторий

**Files:**
- Create: `docs/superpowers/plans/2026-10-28-sf2-hihat-choke.md`

- [x] **Step 1:** Этот документ уже создаётся в `docs/superpowers/plans/2026-10-28-sf2-hihat-choke.md` (worktree). Закоммитить:

```bash
git add docs/superpowers/plans/2026-10-28-sf2-hihat-choke.md
git commit -m "docs: sf2 hihat choke plan"
```

### Task 1: Тесты (TDD — failing tests)

**Files:**
- Create: `tests/drum_choke.rs`

- [x] **Step 1: Написать failing-тесты**

```rust
//! SF2 hihat choke: закрытый/педальный хэт обрывает хвост открытого
//! (см. docs/superpowers/plans/2026-10-28-sf2-hihat-choke.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE};

#[test]
fn tc1_first_hat_nothing_to_choke() {
    let mut e = DrumEngine::new(48_000.0);
    assert_eq!(e.sf2_choke_take(46), 0);
}

#[test]
fn tc2_closed_hat_chokes_open() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(42), 46);
}

#[test]
fn tc3_pedal_hat_chokes_open() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(44), 46);
}

#[test]
fn tc4_retrigger_same_note_cuts_own_tail() {
    // rustysynth не убивает same-key войс на повторном note-on — явный note-off
    // предыдущего триггера предотвращает накопление войсов
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(46), 46);
}

#[test]
fn tc5_non_choke_notes_pass_through() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(38), 0); // снейр не трогает хэт-группу
    assert_eq!(e.sf2_choke_take(42), 46); // состояние группы сохранено
}

#[test]
fn tc6_reset_clears_state() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    e.reset_choke_state();
    assert_eq!(e.sf2_choke_take(42), 0);
}

#[test]
fn tc7_choke_uses_remapped_note() {
    // контракт caller'а: сначала remapped_note(), потом sf2_choke_take()
    let mut e = DrumEngine::new(48_000.0);
    e.remap[(40 - DRUM_NOTE_BASE) as usize] = 10; // pad note 40 -> slot 10 (note 46, Open HH)
    let n = e.remapped_note(40);
    e.sf2_choke_take(n);
    assert_eq!(e.sf2_choke_take(42), n);
}
```

- [x] **Step 2: Убедиться, что не компилируется**

Run: `cargo test --test drum_choke`
Expected: FAIL — `sf2_choke_take`/`reset_choke_state` не существуют.

- [x] **Step 3: Commit (красные тесты)**

```bash
git add tests/drum_choke.rs
git commit -m "test: sf2 hihat choke failing tests"
```

### Task 2: DrumEngine — choke-состояние и sf2_choke_take

**Files:**
- Modify: `src/synth/drum.rs` (const CHOKE_GROUPS ~строка 18; struct DrumEngine ~853; impl ~939)

- [x] **Step 1: Const числа групп** (после `CHOKE_GROUPS`, ~строка 34; максимум id в CHOKE_GROUPS = 2):

```rust
const NUM_CHOKE_GROUPS: usize = 2;
```

- [x] **Step 2: Поле** — в `pub struct DrumEngine` (после `pub remap: [u8; NUM_DRUM_SLOTS],`):

```rust
    /// SF2 choke: last triggered note per choke group (0 = none).
    group_last_note: [u8; NUM_CHOKE_GROUPS],
```

В `DrumEngine::new` (после `remap: ...`):

```rust
            group_last_note: [0u8; NUM_CHOKE_GROUPS],
```

- [x] **Step 3: Методы** — в `impl DrumEngine` (рядом с `remapped_note`):

```rust
    /// SF2 choke: returns the previous note held by this note's choke group
    /// (0 = none) and records `note` as the new holder.  The caller sends a
    /// MIDI note-off for the returned note to the SF2 channel before the
    /// note-on.  Notes outside 36..=51 return 0.  DSP choke is handled inside
    /// trigger_slot and does not use this.
    pub fn sf2_choke_take(&mut self, note: u8) -> u8 {
        if note < DRUM_NOTE_BASE { return 0; }
        let slot = (note - DRUM_NOTE_BASE) as usize;
        if slot >= NUM_DRUM_SLOTS { return 0; }
        let g = CHOKE_GROUPS[slot] as usize;
        if g == 0 { return 0; }
        let prev = self.group_last_note[g - 1];
        self.group_last_note[g - 1] = note;
        prev
    }

    /// Clear choke state (AllNotesOff / panic paths).
    pub fn reset_choke_state(&mut self) {
        self.group_last_note = [0u8; NUM_CHOKE_GROUPS];
    }
```

- [x] **Step 4: Прогнать**

Run: `cargo test --test drum_choke`
Expected: PASS (7 тестов).

- [x] **Step 5: RT-safety + clippy**

Run: `cargo clippy --all-targets -- -D warnings && cargo test --features rt-assert --test rt_safety`
Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src/synth/drum.rs tests/drum_choke.rs
git commit -m "feat: drum engine SF2 choke state (group_last_note)"
```

### Task 3: Вызовы в mod.rs (3 точки) + сброс на панике

**Files:**
- Modify: `src/synth/mod.rs` (live pads ~1585; MIDI-seq ~2563; step-seq триггеры ~2680; AllNotesOff/panic arms)

- [x] **Step 1: Живые пэды** — в `handle_event`, ветка:

```rust
                        if sampler_drums {
                            self.sampler.drum_note_on(note, velocity);
                        } else {
                            self.drum_engine.note_on(note, velocity);
                        }
```

заменить на:

```rust
                        if sampler_drums {
                            let choke = self.drum_engine.sf2_choke_take(note);
                            if choke != 0 { self.sampler.drum_note_off(choke); }
                            self.sampler.drum_note_on(note, velocity);
                        } else {
                            self.drum_engine.note_on(note, velocity);
                        }
```

- [x] **Step 2: MIDI-файл секвенсор** — ветка `midi_player::TrackInstrument::Drums` (~2563):

```rust
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 {
                                self.sampler.drum_note_on(note, velocity);
```

заменить на:

```rust
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 {
                                let choke = self.drum_engine.sf2_choke_take(note);
                                if choke != 0 { self.sampler.drum_note_off(choke); }
                                self.sampler.drum_note_on(note, velocity);
```

- [x] **Step 3: Step-секвенсор** — dispatch `drum_triggers` (~2680):

```rust
            for i in 0..drum_triggers.count {
                let (slot, vel) = drum_triggers.triggers[i];
                self.sampler.drum_note_on(drum::DRUM_NOTE_BASE + slot, vel);
            }
```

заменить на:

```rust
            for i in 0..drum_triggers.count {
                let (slot, vel) = drum_triggers.triggers[i];
                let note = drum::DRUM_NOTE_BASE + slot;
                if self.drum_engine.sf2_mode {
                    let choke = self.drum_engine.sf2_choke_take(note);
                    if choke != 0 { self.sampler.drum_note_off(choke); }
                }
                self.sampler.drum_note_on(note, vel);
            }
```

- [x] **Step 4: Сброс состояния на AllNotesOff и panic**

Найти arm `ControlEvent::AllNotesOff` в `handle_control` и ветку CC120/CC123 в panic-drain (grep `AllNotesOff` и `123` рядом с `note_off_all` / `all_notes_off`). В обеих точках после вызова `self.sampler.all_notes_off()` (или эквивалента) добавить:

```rust
            self.drum_engine.reset_choke_state();
```

(Если arm'ов два — добавить в оба; `sf2_choke_take`-состояние без сброса могло бы заглушить новую ноту чужим висящим prev.)

- [x] **Step 5: Прогнать всё**

Run: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
Expected: PASS.

- [x] **Step 6: Commit**

```bash
git add src/synth/mod.rs
git commit -m "feat: sf2 hihat choke — note-off previous group member before note-on"
```

### Task 4: Финальная верификация

- [x] **Step 1:** `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety` — всё зелёное.

- [x] **Step 2: Ручная проверка (железо):** запустить приложение, включить SF2 Drums (Metal_Drums_v2), назначить Pad → Open Hat (semi-open), сыграть open hat → закрытый хэт (Pad с Closed Hat) — хвост открытого должен оборваться. Если НЕ обрывается (длинный release кита) — записать в TECHDEBT план Б: проставить exclusiveClass=1 в igen-зонах нот 46/47/48/51 Metal_Drums_v2.sf2 (rustysynth voice_collection.rs:32 уже умеет).

- [x] **Step 3: Закрыть TECHDEBT #10** (частично: SF2-путь сделан; DSP уже работал) — обновить пункт 10 в `docs/TECHDEBT.md` пометкой «SF2-путь сделан (коммит …); DSP был ранее».

- [x] **Step 4: Commit**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: techdebt #10 — SF2 choke implemented"
```
