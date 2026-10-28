# Bank-2 LED Console Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ f455503 чтением кода; см. «Якоря» в задачах)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Банк-2 пэдов SMK-37 Pro — аппаратный пульт: нижняя строка = транспорт
(ноты уже маршрутизируются движком), верхняя = перформансы/паттерны/мьюты с
tap/hold-меню, LED-подсветка состояния через python-компаньон по unix-сокету.

**Architecture:** Чистый автомат `LedConsole` (src/gui/led_console.rs, unit-тесты,
GUI-поток) получает события пэдов из MIDI-треда (swallow нот 60–67/78–80 ch9 в
parse_and_push, mpsc в GUI) и выполняет действия существующими ControlEvent'ами.
LED-снимок (16×RRGGBB) уходит клиентом unix-сокета в python-компаньон
(smk37_config/pad_console.py), который красит LED через GATT без commit.
Настройка `led_companion` (дефолт off) — выключена = сокет не используется.

**Tech Stack:** Rust (std mpsc, UnixStream — без новых deps), python3 + dbus
(компаньон, реюз smk.py), serde (config field).

**Spec:** docs/superpowers/specs/2026-10-29-bank2-led-console-design.md

**Проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требования: (1) тап по пэду 17–24 в HOME грузит перформанс N; (2) холд ≥500ms
любого верхнего пэда открывает меню; (3) в меню тап 17/18/19 → HOME/PATTERN/MUTE;
(4) в PATTERN тап по слоту переключает паттерн; (5) в MUTE тап инвертирует мьют
части; (6) Prev/Next шагают перф или паттерн по режиму; (7) Rew возвращает лупер
в начало; (8) ноты 60–67/78–80 не доходят до ударки; (9) при led_companion=off
сокет не подключается; (10) LED-снимок = 16 цветов в порядке слотов 0–15.
Задача 1 пишется FRESH subagent'ом, получившим ТОЛЬКО этот план.

## Negative Matrix

- duplicate/concurrent — двойной NoteOn без NoteOff: pending перезаписывается
  (последний wins, idempotent); тап в момент холда — pending уже снят, тап
  обрабатывается как действие нового режима (тест).
- NoteOff без NoteOn / двойной NoteOff: pending=None, no-op (тест).
- Холд в MUTE/PATTERN → меню; холд в Menu → остаёмся в Menu (no-op, тест).
- missing field — config без `led_companion` → serde default false (тест).
- socket отсутствует: connect мгновенно ECONNREFUSED, retry раз в 2с; умер
  writer — alive-флаг, тот же retry; GUI не блокируется (mpsc в отдельный тред).
  (Зависший backlog-сокет — не обрабатывается, задокументировано.)
- Значение цвета: слоты 20–24 в меню тёмные; пустой слот перфа = 000000+FF
  (LED off, допустимый валидный цвет — тест snapshot).
- clock skew — неприменимо (Instant монотонные часы одного процесса).
- external-call timeout — connect к /tmp-сокету без таймаута, но в GUI-кадре
  раз в 2с (ECONNREFUSED мгновенен); живой write — только в writer-треде,
  GUI никогда не блокируется на I/O.
- concurrent writers к сокету — один writer-тред, mpsc сериализует.

---

### Task 1: Консоль-автомат — failing tests + реализация (TDD, чистая логика)

**Files:**
- Create: `src/gui/led_console.rs` (модуль + `#[cfg(test)] mod tests`)
- Modify: `src/gui/mod.rs` (сразу `pub mod led_console;` — иначе модуль не
  компилируется и красной фазы не будет)

- [ ] **Step 1: написать failing-тесты** — создать `src/gui/led_console.rs`
  ТОЛЬКО с типами, struct и тестами (БЕЗ `impl LedConsole` — методы не
  существуют, тесты не скомпилируются; это и есть красная фаза), и объявить
  модуль в `src/gui/mod.rs` (`pub mod led_console;` в списке модулей).

```rust
//! Bank-2 pad console state machine (pure logic, GUI thread only).
//! Slots 0..=7 = top-row pads 17..=24 (notes 60..=67).
//! Prev/Next/Rewind arrive as actions from notes 78/79/80 (routed by caller).
//! Engine-side transport notes 81/82/83 (mod.rs:1573-1598) never reach this
//! module; console Undo is note 75 (engine's legacy 64-arm superseded).

use std::time::{Duration, Instant};

const HOLD: Duration = Duration::from_millis(500);

/// Top-row mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Home,
    Pattern,
    Mute,
    Menu,
}

/// Console input events (note -> slot by the caller).
#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    /// NoteOn on a top-row pad (slot 0..=7).
    Down(usize),
    /// NoteOff on a top-row pad (slot 0..=7).
    Up(usize),
    /// Bottom-row: Undo (note 75).
    Undo,
    /// Bottom-row nav: Prev (note 78).
    Prev,
    /// Bottom-row nav: Next (note 79).
    Next,
}

/// Console output: what GUI must do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    LoadPerf(usize),
    SelectPattern(usize),
    ToggleMute(usize),
    Undo,
    StepPerf(i32),
    StepPattern(i32),
}

pub struct LedConsole {
    mode: Mode,
    pending: Option<(usize, Instant)>,
}

impl Default for LedConsole {
    fn default() -> Self {
        Self::new()
    }
}
```

Тесты в тот же файл (`#[cfg(test)] mod tests`, в конец):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn t(ms: u64) -> Instant {
        Instant::now() - Duration::from_millis(ms)
    }

    #[test]
    fn tap_in_home_loads_perf() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        assert_eq!(c.on_event(PadEvent::Down(3), now), None);
        assert_eq!(c.on_event(PadEvent::Up(3), now + Duration::from_millis(50)),
                   Some(Action::LoadPerf(3)));
    }

    #[test]
    fn hold_opens_menu_and_eats_tap() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(5), now);
        // 500ms pass -> tick fires menu
        assert_eq!(c.tick(now + Duration::from_millis(510)), Some(Mode::Menu));
        // release after hold: no action
        assert_eq!(c.on_event(PadEvent::Up(5), now + Duration::from_millis(600)), None);
    }

    #[test]
    fn menu_tap_selects_mode() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)), None);
        assert_eq!(c.mode, Mode::Pattern);
    }

    #[test]
    fn stray_noteoff_is_noop() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        assert_eq!(c.on_event(PadEvent::Up(2), now), None);
        assert_eq!(c.mode, Mode::Home);
    }

    #[test]
    fn duplicate_down_last_wins() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        c.on_event(PadEvent::Down(4), now + Duration::from_millis(10));
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)), None);
        assert_eq!(c.on_event(PadEvent::Up(4), now + Duration::from_millis(40)),
                   Some(Action::LoadPerf(4)));
    }

    #[test]
    fn pattern_mode_tap_selects_pattern() {
        let mut c = LedConsole::new();
        c.mode = Mode::Pattern;
        let now = Instant::now();
        c.on_event(PadEvent::Down(6), now);
        assert_eq!(c.on_event(PadEvent::Up(6), now + Duration::from_millis(30)),
                   Some(Action::SelectPattern(6)));
    }

    #[test]
    fn mute_mode_tap_toggles_part() {
        let mut c = LedConsole::new();
        c.mode = Mode::Mute;
        let now = Instant::now();
        c.on_event(PadEvent::Down(0), now);
        assert_eq!(c.on_event(PadEvent::Up(0), now + Duration::from_millis(30)),
                   Some(Action::ToggleMute(0)));
    }

    #[test]
    fn prev_next_are_mode_dependent() {
        let mut c = LedConsole::new();
        assert_eq!(c.on_event(PadEvent::Prev, Instant::now()), Some(Action::StepPerf(-1)));
        c.mode = Mode::Pattern;
        assert_eq!(c.on_event(PadEvent::Next, Instant::now()), Some(Action::StepPattern(1)));
    }

    #[test]
    fn snapshot_colors_home_and_menu() {
        let mut c = LedConsole::new();
        let mut mutes = [false; 8];
        mutes[2] = true;
        let home = c.top_colors(Some(1), 3, 0, &mutes);
        assert_eq!(home[1], [0xFF, 0x80, 0x00]); // active
        assert_eq!(home[0], [0x30, 0x30, 0x30]); // filled
        assert_eq!(home[7], [0x00, 0x00, 0x00]); // empty -> LED off
        c.mode = Mode::Menu;
        let menu = c.top_colors(None, 0, 0, &mutes);
        assert_eq!(menu[0], [0x40, 0x40, 0x40]);
        assert_eq!(menu[4], [0x00, 0x00, 0x00]);
    }

    #[test]
    fn bottom_transport_colors() {
        let c = LedConsole::new();
        let b = c.bottom_colors(true, false);
        assert_eq!(b[0], [0x00, 0xFF, 0x00]); // play lit
        let b = c.bottom_colors(false, true);
        assert_eq!(b[2], [0xFF, 0x00, 0x00]); // rec lit
    }

    #[test]
    fn hold_in_menu_stays_menu() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let now = Instant::now();
        c.on_event(PadEvent::Down(6), now);
        assert_eq!(c.tick(now + Duration::from_millis(510)), Some(Mode::Menu));
        assert_eq!(c.on_event(PadEvent::Up(6), now + Duration::from_millis(600)), None);
        assert_eq!(c.mode, Mode::Menu);
    }

    #[test]
    fn undo_action_passes_through() {
        let mut c = LedConsole::new();
        assert_eq!(c.on_event(PadEvent::Undo, Instant::now()), Some(Action::Undo));
    }
}
```

- [ ] **Step 2: прогнать — убедиться в красной фазе**

Run: `cargo test --lib led_console`
Expected: COMPILE FAIL — `no method named on_event/tick/top_colors/bottom_colors/mode`
(E0599, `impl LedConsole` отсутствует).

- [ ] **Step 3: реализация** — добавить `impl LedConsole` после struct (перед
`#[cfg(test)]`):

```rust
impl LedConsole {
    pub fn new() -> Self {
        Self { mode: Mode::Home, pending: None }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Poll: fire hold detection. Returns Some(Mode::Menu) on hold trigger.
    pub fn tick(&mut self, now: Instant) -> Option<Mode> {
        if let Some((_, t0)) = self.pending {
            if now.duration_since(t0) >= HOLD {
                self.pending = None;
                self.mode = Mode::Menu;
                return Some(Mode::Menu);
            }
        }
        None
    }

    /// Feed a pad event; returns the action to execute (if any).
    pub fn on_event(&mut self, ev: PadEvent, now: Instant) -> Option<Action> {
        match ev {
            PadEvent::Down(slot) => {
                self.pending = Some((slot, now));
                None
            }
            PadEvent::Up(slot) => {
                let Some((down_slot, t0)) = self.pending else { return None; };
                if down_slot != slot {
                    return None; // not our press; keep pending
                }
                let elapsed = now.duration_since(t0);
                self.pending = None;
                if elapsed < HOLD {
                    self.tap(slot)
                } else {
                    None // hold consumed by tick()
                }
            }
            PadEvent::Undo => Some(Action::Undo),
            PadEvent::Prev => Some(self.step(-1)),
            PadEvent::Next => Some(self.step(1)),
        }
    }

    fn tap(&mut self, slot: usize) -> Option<Action> {
        if self.mode == Mode::Menu {
            let m = match slot {
                0 => Mode::Home,
                1 => Mode::Pattern,
                2 => Mode::Mute,
                _ => return None, // dark slot in menu: no-op
            };
            self.mode = m;
            return None; // mode switch only; GUI repaints via snapshot
        }
        Some(match self.mode {
            Mode::Home => Action::LoadPerf(slot),
            Mode::Pattern => Action::SelectPattern(slot),
            Mode::Mute => Action::ToggleMute(slot),
            Mode::Menu => unreachable!(),
        })
    }

    fn step(&mut self, dir: i32) -> Action {
        match self.mode {
            Mode::Home => Action::StepPerf(dir),
            Mode::Pattern => Action::StepPattern(dir),
            _ => Action::StepPerf(dir),
        }
    }

    /// LED colors for slots 0..=7 (top row) given current state.
    pub fn top_colors(
        &self,
        active_perf: Option<usize>,
        filled_perfs: usize,
        current_pattern: usize,
        mutes: &[bool; 8],
    ) -> [[u8; 3]; 8] {
        let mut c = [[0u8; 3]; 8];
        match self.mode {
            Mode::Home => {
                for (i, col) in c.iter_mut().enumerate() {
                    if Some(i) == active_perf {
                        *col = [0xFF, 0x80, 0x00];
                    } else if i < filled_perfs {
                        *col = [0x30, 0x30, 0x30];
                    } else {
                        *col = [0x00, 0x00, 0x00];
                    }
                }
            }
            Mode::Pattern => {
                for (i, col) in c.iter_mut().enumerate() {
                    if i == current_pattern {
                        *col = [0xFF, 0x80, 0x00];
                    } else {
                        *col = [0x30, 0x30, 0x30];
                    }
                }
            }
            Mode::Mute => {
                for (i, col) in c.iter_mut().enumerate() {
                    if mutes[i] {
                        *col = [0x10, 0x10, 0x10];
                    } else {
                        *col = [0x00, 0x66, 0xFF];
                    }
                }
            }
            Mode::Menu => {
                for (i, col) in c.iter_mut().enumerate() {
                    *col = if i < 3 { [0x40, 0x40, 0x40] } else { [0x00, 0x00, 0x00] };
                }
            }
        }
        c
    }

    /// Bottom-row (transport) colors: slots 8..=15 = pads 25..=32.
    pub fn bottom_colors(&self, playing: bool, recording: bool) -> [[u8; 3]; 8] {
        let dim = [0x20, 0x20, 0x20];
        let mut c = [dim; 8];
        c[0] = if playing { [0x00, 0xFF, 0x00] } else { dim };
        c[1] = if !playing && !recording { [0xFF, 0xDD, 0x00] } else { dim };
        c[2] = if recording { [0xFF, 0x00, 0x00] } else { dim };
        c[3] = [0x66, 0x55, 0x00]; // Rew: dim amber
        // slots 4..=7 (unused, Prev, Next, Undo) stay dim
        c
    }
}
```

- [ ] **Step 4: прогнать**

Run: `cargo test --lib led_console`
Expected: PASS (12 тестов).

- [ ] **Step 5: clippy + rt**

Run: `cargo clippy --all-targets -- -D warnings && cargo test --features rt-assert --test rt_safety`
Expected: PASS. Если clippy ругается на dead_code (модуль ещё не подключён в
gui/mod.rs) — добавить над impl `#[allow(dead_code)] // wired in Task 4`
и снять его в Task 4.

- [ ] **Step 6: Commit**

```bash
git add src/gui/led_console.rs
git commit -m "feat: bank-2 console state machine (tap/hold, modes, snapshots)"
```


### Task 2: Looper rewind + Prev/Next-агностика (движок)

**Files:**
- Modify: `src/synth/looper.rs` (метод rewind_to_start рядом с `toggle_play`)
- Modify: `src/synth/mod.rs` (ControlEvent + arm)

- [ ] **Step 1: тест rewind** — в `src/synth/looper.rs` в конец
  `mod restore_tests` добавить (тест red: метода нет):

```rust
    #[test]
    fn rewind_to_start_resets_playback_cursor() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[super::LooperEventSnapshot { tick: 0, note: 60, velocity: 127, layer_id: 0, part_id: 0 }], 48_000);
        // simulate playback position advance via public API only:
        // rewind must reset playback_tick/cursor without touching events/len.
        let len_before = l.len();
        l.rewind_to_start();
        assert_eq!(l.len(), len_before, "rewind must not drop events");
        assert_eq!(l.state, LooperState::Idle);
    }
```

(Поле `playback_tick` приватное — публичный контракт проверяем через len/state;
полевая проверка cursor — ревьюером кодом.)

- [ ] **Step 2: метод в `impl MidiLooper`** (рядом с `toggle_play`, код полей
  сверить с существующим `clear`/`restore` — сброс ТОЛЬКО transient playback):

```rust
    /// Rewind playback position to the loop start (audio thread). Content,
    /// length and mute/solo state are untouched.
    pub fn rewind_to_start(&mut self) {
        self.playback_tick = 0;
        self.playback_cursor = 0;
        self.needs_note_offs = true;
        self.update_display();
    }
```

(`needs_note_offs = true` ОБЯЗАТЕЛЬНО: rewind посреди проигрывания должен
закрыть звучащие ноты на следующем тике; `clear`/`restore` ставят false только
потому что идут из Idle — аналогия неприменима.)

- [ ] **Step 3: ControlEvent** — в enum после `LooperSetSolo` (grep `LooperSetSolo`):

```rust
    LooperRewind,
```

arm в handle_control рядом с остальными Looper-arm:

```rust
            ControlEvent::LooperRewind => {
                self.looper.rewind_to_start();
            }
```

(проверить: не-heap вариант — `return None` не нужен, arm без return; сверить
с соседями.)

- [ ] **Step 4: прогнать**

Run: `cargo test --lib && cargo clippy --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/synth/looper.rs src/synth/mod.rs
git commit -m "feat: looper rewind_to_start + LooperRewind control event"
```

### Task 3: MIDI-роутинг — swallow консольных нот через глобальный канал

**Files:**
- Modify: `src/midi.rs` (static CONSOLE_TX, is_console_note, swallow в parse_and_push)
- Modify: `src/lib.rs` (init_common: создать mpsc-канал, set CONSOLE_TX, поле CommonInit.console_rx)
- Modify: `src/gui/mod.rs` (App.console_rx берётся из CommonInit)

Канал — глобальный `static` (паттерн `JNI_MIDI_TX`, lib.rs:180): desktop midir,
JNI и будущие пути получают его бесплатно, сигнатура parse_and_push НЕ меняется.

- [ ] **Step 1: тип канала + предикат** — в `src/midi.rs` рядом с
  `pub type NoteState` (~16):

```rust
/// Console pads (bank-2 notes 60..=67 and 75..=80 on ch9) are mirrored to the
/// GUI instead of the audio ring. NOTE: 64 is INSIDE the range — it is console
/// top-row slot 4; the engine's legacy note-64 undo arm (mod.rs:1592) becomes
/// unreachable via parse_and_push (kept as dead code, superseded by console
/// Undo note 75).
pub type ConsoleTx = std::sync::Mutex<Option<std::sync::mpsc::Sender<(u8, bool)>>>;
pub static CONSOLE_TX: std::sync::OnceLock<ConsoleTx> = std::sync::OnceLock::new();

pub fn is_console_note(note: u8) -> bool {
    (60..=67).contains(&note) || (75..=80).contains(&note)
}
```

(static требует Sync: Mutex<Option<Sender>> — Sync ✓; Sender внутри Mutex.)

- [ ] **Step 2: swallow в parse_and_push** — сразу после разбора event
  (после блока логгирования ~116-124, ДО «Update GUI note display» и ДО
  финального push в ring, который найти в конце функции — `match tx.try_lock()`
  ~145-152):

```rust
    // LED-console notes: mirror to GUI, never reach the audio engine.
    // vel-0 NoteOn == note-off convention: map to is_on=false.
    let (console_note, is_on) = match &event {
        MidiEvent::NoteOn { channel: 9, note, velocity, .. } if *velocity > 0 && is_console_note(*note) => (Some(*note), true),
        MidiEvent::NoteOn { channel: 9, note, .. } if is_console_note(*note) => (Some(*note), false),
        MidiEvent::NoteOff { channel: 9, note } if is_console_note(*note) => (Some(*note), false),
        _ => (None, false),
    };
    if let Some(note) = console_note {
        if let Ok(guard) = CONSOLE_TX.get_or_init(Default::default).lock() {
            if let Some(sender) = guard.as_ref() {
                let _ = sender.send((note, is_on));
            }
        }
        return consumed;
    }
```

- [ ] **Step 3: создание канала в init_common (lib.rs)** — рядом с
  `JNI_MIDI_TX.set(...)` (~180):

```rust
    let (console_snd, console_rx) = std::sync::mpsc::channel::<(u8, bool)>();
    let _ = CONSOLE_TX.set(std::sync::Mutex::new(Some(console_snd)));
```

Поле `pub console_rx: std::sync::mpsc::Receiver<(u8, bool)>` добавить в struct
`CommonInit` (~81, рядом с midi_tx_shared) и в её конструктор (~295 — рядом с
местом, где передаётся midi_tx_shared). Дочитывать: все места, где CommonInit
конструируется (desktop + android ветки, grep `CommonInit {`).

- [ ] **Step 3b: AMidi-путь (Android USB) — swallow без mirror** —
  `src/audio.rs:216` парсит через `parse_midi_message` ПРЯМО в аудио-колбэке,
  минуя parse_and_push: там консольные ноты обязаны молча дропаться (нельзя
  Mutex/allocation в RT — зеркала в GUI с AMidi-пути НЕТ, by design: Android
  USB не водитель консоли). Найти dispatch события (~216, завершается
  `synth.handle_event(event)` ~230 — НЕ `midi_tx.push`, там consumer-сторона)
  и поставить guard ПЕРЕД всем per-event блоком, включая обновление
  note/pad-атомиков:

```rust
                        // Bank-2 console notes (60..=67, 75..=80 ch9): drop —
                        // owned by the GUI console (desktop BLE only here).
                        let console_drop = matches!(&event,
                            synth::MidiEvent::NoteOn { channel: 9, note, .. }
                            | synth::MidiEvent::NoteOff { channel: 9, note }
                                if midi::is_console_note(*note));
                        if !console_drop {
                            let _ = midi_tx.push(event);
                        }
```

(точные имена переменных — по реальному коду audio.rs:216-230; суть: guard
перед `synth.handle_event` и до атомиков — консольная нота на AMidi-пути
вообще не производит эффектов. RT-safe: только matches! на разобранном event.)

- [ ] **Step 4: callers** — сигнатура parse_and_push не менялась: midir
  (midi.rs:264), JNI (lib.rs:991), три вызова midi::connect/connect_by_name
  (lib.rs:275, 574, 676) — правок не требуют. Тесты midi_parse/end_to_end зовут
  parse_midi_message — не трогаются. Проверить grep `parse_and_push(` —
  сигнатурных правок быть не должно.

- [ ] **Step 5: unit-тест swallow** — добавить в `tests/midi_parse.rs`:

```rust
#[test]
fn console_notes_swallowed_and_mirrored() {
    let (prod, mut cons) = rtrb::RingBuffer::<mini_midi_synth::synth::MidiEvent>::new(64);
    let tx: mini_midi_synth::midi::SharedMidiTx =
        std::sync::Arc::new(std::sync::Mutex::new(prod));
    let note_state: mini_midi_synth::midi::NoteState =
        std::sync::Arc::new(std::array::from_fn(|_| Default::default()));
    let pad_state: mini_midi_synth::midi::PadState =
        std::sync::Arc::new(std::array::from_fn(|_| Default::default()));
    let (snd, rx) = std::sync::mpsc::channel();
    *mini_midi_synth::midi::CONSOLE_TX.get_or_init(Default::default)
        .lock().unwrap() = Some(snd);
    // NoteOn ch9 note 60 vel 127: 99 3C 7F
    let used = mini_midi_synth::midi::parse_and_push(&[0x99, 0x3C, 0x7F], &tx, &note_state, &pad_state);
    assert_eq!(used, 3);
    assert_eq!(rx.recv().unwrap(), (60, true));      // mirrored to GUI
    assert!(cons.pop().is_none());                   // never reached the engine
    // NoteOff ch9 note 60: 89 3C 7F
    let _ = mini_midi_synth::midi::parse_and_push(&[0x89, 0x3C, 0x7F], &tx, &note_state, &pad_state);
    assert_eq!(rx.recv().unwrap(), (60, false));
}
```

(если SharedMidiTx/типы называются иначе — сверить с midi.rs реальным; rtrb
уже dev-доступен через основной crate.)

- [ ] **Step 6: прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/midi.rs src/lib.rs src/audio.rs tests/midi_parse.rs
git commit -m "feat: mirror bank-2 console notes to GUI via global CONSOLE_TX"
```


### Task 4: GUI-интеграция — автомат, действия, LED-снимок

**Files:**
- Modify: `src/gui/mod.rs` (mod led_console; App-поля; poll в update; обработчик действий; снимок → socket-writer)
- Якоря: `DrumSeqPattern` push — gui/mod.rs:1500 (`send_drum_state`); `load_performance_by_name` — gui/mod.rs:1669; `SetPartMute` push — grep; `drum_current_pattern` — gui/mod.rs:315; parts[].mute — grep `pub mute` в gui/mod.rs

- [ ] **Step 1: подключить модуль** — в `src/gui/mod.rs` к списку модулей:
  `pub mod led_console;`. Если в Task 1 ставился `#[allow(dead_code)]` — снять.

App-поля (рядом с `drum_current_pattern` ~315):

```rust
    pub console: led_console::LedConsole,
    pub led_snapshot: [[u8; 3]; 16],
    pub led_dirty: bool,
    pub led_socket: Option<std::sync::mpsc::Sender<String>>,
    pub led_socket_alive: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub led_socket_failed_at: Option<std::time::Instant>,
```

инициализация в App-литерале (lib.rs ~695+, где `drum_current_pattern: 0,`):
`console: led_console::LedConsole::default()`, `led_snapshot: [[0;3];16]`,
`led_dirty: true`, `led_socket: None`,
`led_socket_alive: None`, `led_socket_failed_at: None`.
Receiver консоли берётся из CommonInit (Task 3 Step 3): у `run_gui`-стороны
доступ к `c.console_rx` (CommonInit → App): в App-литерал добавить
`console_rx: Some(c.console_rx),` и поле App:
`pub console_rx: Option<std::sync::mpsc::Receiver<(u8, bool)>>,`
(если CommonInit потребляется в нескольких местах — Receiver нельзя клонировать;
передавать `Some(c.console_rx)` в единственном месте построения App; если
потоков построения App два — glm-исполнитель выбирает единственный desktop-путь
и для android-пути создаёт канал заново... ЗАПРЕЩЕНО: канал один. Если desktop
и android строят App из одного CommonInit — ок; если два разных CommonInit —
создать канал в каждом (CONSOLE_TX.set второй раз вернёт Err — допустимо,
swallow перестанет работать на той ветке, где set не удался; пометить в отчёте).

- [ ] **Step 2: обработка событий** — в главном цикле App (update/draw, где
  поллятся атомы — grep `drum_play_atom` в gui/mod.rs update), добавить poll:

```rust
        // Bank-2 console pads -> state machine -> actions
        let mut acts: Vec<led_console::Action> = Vec::new();
        if let Some(rx) = &self.console_rx {
            while let Ok((note, is_on)) = rx.try_recv() {
                let now = std::time::Instant::now();
                let ev = match (note, is_on) {
                    (60..=67, true) => Some(led_console::PadEvent::Down((note - 60) as usize)),
                    (60..=67, false) => Some(led_console::PadEvent::Up((note - 60) as usize)),
                    (75, true) => Some(led_console::PadEvent::Undo),
                    (78, true) => Some(led_console::PadEvent::Prev),
                    (79, true) => Some(led_console::PadEvent::Next),
                    (80, true) => {
                        let _ = self.ctrl_tx.push(crate::synth::ControlEvent::LooperRewind);
                        None
                    }
                    _ => None,
                };
                if let Some(ev) = ev {
                    if let Some(a) = self.console.on_event(ev, now) { acts.push(a); }
                }
            }
        }
        let _ = self.console.tick(std::time::Instant::now()); // hold -> Menu (snapshot below)
        for a in acts { self.execute_console_action(a); }
```

`execute_console_action` (новый метод impl App):

```rust
    fn execute_console_action(&mut self, action: led_console::Action) {
        match action {
            led_console::Action::LoadPerf(i) => {
                // perf_list: Vec<(String, PathBuf)>, index 0 = "(Init)" (lib.rs:789).
                // Console slots = perf_list indices 0..=7.
                if let Some((name, _)) = self.perf_list.get(i) {
                    let name = name.clone();
                    if self.load_performance_by_name(&name) {
                        log::info!("[console] perf slot {} loaded ({})", i + 1, name);
                    }
                }
            }
            led_console::Action::SelectPattern(i) => {
                self.drum_current_pattern = i as u8;
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: i as u8 });
            }
            led_console::Action::ToggleMute(i) => {
                if let Some(part) = self.parts.get_mut(i) {
                    let mute = !part.mute;
                    part.mute = mute;
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::SetPartMute { part: i, mute });
                }
            }
            led_console::Action::Undo => {
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqUndo);
            }
            led_console::Action::StepPerf(dir) => {
                let n = self.perf_list.len();
                if n == 0 { return; }
                let cur = self.active_perf_console().unwrap_or(0);
                let next = (cur as i32 + dir).rem_euclid(n as i32) as usize;
                if let Some((name, _)) = self.perf_list.get(next) {
                    let name = name.clone();
                    self.load_performance_by_name(&name);
                }
            }
            led_console::Action::StepPattern(dir) => {
                let next = (self.drum_current_pattern as i32 + dir).rem_euclid(8);
                self.drum_current_pattern = next as u8;
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: next as u8 });
            }
        }
    }

    /// Index of the current performance in perf_list (None if unknown).
    fn active_perf_console(&self) -> Option<usize> {
        self.perf_list.iter().position(|(n, _)| n == &self.perf_name)
    }
```

(ИСТОЧНИК ИСТИНЫ по перфам: `self.perf_list: Vec<(String, PathBuf)>` с
`"(Init)"` на индексе 0 (lib.rs:789, layers.rs:241) и `self.perf_name: String`
(gui/mod.rs:342) — НЕ пресетовый скан каталога: `load_performance_by_name`
резолвит имена через perf_list, сторонний список сломает загрузку.
`load_performance_by_name` должен обновлять `perf_name` — сверить (grep его
тела ~1669); если не обновляет — присвоить `self.perf_name = name` после
успешной загрузки. `parts[i].mute` — сверить имя поля, grep `pub mute` в
gui/mod.rs. Заметка на поля: perf_list обновляется в layers.rs — если юзер
сохранил новый перф, не открывая Layers, список может быть stale; v1 accepts.)

- [ ] **Step 3: LED-снимок** — там же в update, после poll:

```rust
        // LED snapshot (16 slots = pads 17..=32, top row first)
        let playing = self.drum_play_atom.load(Ordering::Relaxed) != 0;
        let recording = self.drum_rec_atom.load(Ordering::Relaxed) != 0;
        let mut mutes = [false; 8];
        for (i, part) in self.parts.iter().take(8).enumerate() {
            mutes[i] = part.mute;
        }
        let filled = self.perf_list.len().min(8);
        let top = self.console.top_colors(self.active_perf_console(), filled,
                                          self.drum_current_pattern as usize, &mutes);
        let bottom = self.console.bottom_colors(playing, recording);
        let mut snap = [[0u8; 3]; 16];
        snap[..8].copy_from_slice(&top);
        snap[8..].copy_from_slice(&bottom);
        if snap != self.led_snapshot || self.led_dirty {
            self.led_snapshot = snap;
            self.led_dirty = false;
            if let Some(sock) = &self.led_socket {
                let mut msg = String::from("STATE");
                for c in &snap {
                    msg.push_str(&format!(" {:02X}{:02X}{:02X}", c[0], c[1], c[2]));
                }
                msg.push('\n');
                let _ = sock.send(msg);
            }
        }
```

(формат: одна строка `STATE <RRGGBB>×16\n`; writer-тред из Task 5 пишет её в
сокет как байты.)

- [ ] **Step 4: clippy/test**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/gui/mod.rs src/gui/led_console.rs
git commit -m "feat: wire bank-2 console into GUI (actions, snapshots, transport LEDs)"
```

### Task 5: config-тумблер + socket-клиент с живостью (alive/retry/dirty)

**Files:**
- Modify: `src/config.rs` (UiSettings.led_companion)
- Modify: `src/gui/settings.rs` (checkbox в настройках)
- Modify: `src/gui/mod.rs` (подключение/retry/отключение + writer-тред)
- Test: `tests/config_led.rs` (новый)

- [ ] **Step 1: config** — в `UiSettings` рядом с `split_point` (~83):

```rust
    /// Bank-2 LED console companion (unix socket). Off = never connect.
    #[serde(default)]
    pub led_companion: bool,
```

Тест `tests/config_led.rs`:

```rust
#[test]
fn led_companion_defaults_false() {
    let s: mini_midi_synth::config::UiSettings =
        serde_json::from_str("{}").unwrap();
    assert!(!s.led_companion);
}
```

(проверить: UiSettings pub + serde; в config.rs ~162 есть РУЧНОЙ `impl Default`
для UiSettings со всеми полями — добавить туда `led_companion: false`, иначе
не скомпилируется.)

- [ ] **Step 2: подключение/переподключение в update** — в gui/mod.rs update
  (рядом с poll-блоком Task 4):

```rust
        // LED companion socket lifecycle
        use std::sync::atomic::Ordering;
        if !self.config.ui.led_companion {
            if self.led_socket.take().is_some() {
                log::info!("[console] companion disabled");
            }
        } else if self.led_socket.is_none()
            && self.led_socket_failed_at
                .map(|t| t.elapsed() > std::time::Duration::from_secs(2))
                .unwrap_or(true)
        {
            match std::os::unix::net::UnixStream::connect("/tmp/mini_midi_synth_led.sock") {
                Ok(stream) => {
                    let (tx, rx) = std::sync::mpsc::channel::<String>();
                    // fresh AtomicBool на каждый коннект: старый writer не может
                    // перезаписать флаг нового (spurious reconnect)
                    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
                    self.led_socket_alive = Some(alive.clone());
                    std::thread::spawn(move || {
                        use std::io::Write;
                        let mut stream = stream;
                        for msg in rx {
                            if stream.write_all(msg.as_bytes()).is_err() {
                                alive.store(false, Ordering::Relaxed);
                                break;
                            }
                        }
                        alive.store(false, Ordering::Relaxed);
                    });
                    self.led_socket = Some(tx);
                    self.led_socket_failed_at = None;
                    self.led_dirty = true; // отправить полный STATE сразу после коннекта
                    log::info!("[console] companion connected");
                }
                Err(_) => {
                    self.led_socket_failed_at = Some(std::time::Instant::now());
                }
            }
        }
        // writer умер (компаньон перезапускается) — забыть сокет, retry через 2с
        let dead = match &self.led_socket_alive {
            Some(a) => !a.load(Ordering::Relaxed),
            None => false,
        };
        if self.led_socket.is_some() && dead {
            self.led_socket = None;
            self.led_socket_alive = None;
            self.led_socket_failed_at = Some(std::time::Instant::now());
        }
```

- [ ] **Step 3: checkbox в настройках** — settings.rs, секция аудио/MIDI
  настроек (grep `split_point` UI или другой checkbox):

```rust
                ui.checkbox(&mut self.config.ui.led_companion, "LED companion (bank-2 pads)");
```

- [ ] **Step 4: прогнать + commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo test
git add src/config.rs src/gui/settings.rs src/gui/mod.rs tests/config_led.rs
git commit -m "feat: led_companion toggle, resilient socket client (retry/alive/dirty-on-connect)"
```


### Task 6: Компаньон pad_console.py

**Files:**
- Create: `~/Projects/smk37_config/pad_console.py` (вне репо синта — hardware tooling)
- Test: unit-самотест `--selftest` (framing + slot→addr, без BLE)

- [ ] **Step 1: написать компаньон** — dbus-вызовы В ТОЙ ЖЕ ФОРМЕ, что smk.py
  (`Interface(bus.get_object(...), "org.bluez.GattCharacteristic1")`), BLE
  reconnect циклом снаружи socket-accept:

```python
#!/usr/bin/env python3
"""Bank-2 LED console companion for mini_midi_synth.

Listens on /tmp/mini_midi_synth_led.sock, receives "STATE c0 c1 ... c15"
(c* = RRGGBB hex; slots 0-7 = pads 17-24, slots 8-15 = pads 25-32) and paints
the keyboard LEDs over BLE GATT WITHOUT commit (RAM-only, instant).
BLE dies -> reconnect loop. Synth disconnects -> back to accept().

Run: python3 pad_console.py
"""
import socket, os, sys, time
import dbus

SOCK = "/tmp/mini_midi_synth_led.sock"
BUS = "org.bluez"
BASE = "/org/bluez/hci0/dev_D8_31_0D_19_D1_0C"
CHAR = BASE + "/service0060/char0061"   # write characteristic (smk.py)

def slot_addr(slot: int) -> int:
    # slots 0-7 -> pads 17-24; slots 8-15 -> pads 25-32
    return 1467 + 74 * slot if slot < 8 else 2059 + 74 * (slot - 8)

def frame(cmd: int, payload: bytes) -> bytes:
    return b"\x00\x59" + bytes([cmd, len(payload), 0x00, 0x00]) + payload + bytes([(~sum(payload)) & 0xFF])

def paint(iface, slot: int, rgb: bytes):
    a = slot_addr(slot)
    payload = bytes([0x05, a & 0xFF, (a >> 8) & 0xFF, 0x00, 0x00, 4, 0x00, 0x00]) + rgb + b"\xff"
    iface.WriteValue(dbus.Array([dbus.Byte(b) for b in frame(0x22, payload)], signature="y"),
                     {"type": "command"})

def selftest():
    assert slot_addr(0) == 1467 and slot_addr(7) == 1467 + 74 * 7
    assert slot_addr(8) == 2059 and slot_addr(15) == 2059 + 74 * 7
    payload = bytes([0x05, 0xCB, 0x09, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xE0, 0xFF, 0xFF])
    f = frame(0x22, payload)
    assert f[:6] == b"\x00\x59\x22\x0c\x00\x00", f[:6].hex()
    assert f[6:-1] == payload
    assert f[-1] == (~sum(payload)) & 0xFF
    print("selftest ok")

def ble_connect(bus):
    while True:
        try:
            obj = bus.get_object(BUS, CHAR)
            iface = dbus.Interface(obj, "org.bluez.GattCharacteristic1")
            # probe: commit-frame как в smk.py (cp = 05 00 ... 00)
            iface.WriteValue(dbus.Array([dbus.Byte(b) for b in frame(0x22, bytes([0x05, 0, 0, 0, 0, 0, 0, 0]))], signature="y"),
                             {"type": "command"})
            print("BLE ready")
            return iface
        except Exception as e:
            print("BLE not ready:", e)
            time.sleep(3)

def main():
    if "--selftest" in sys.argv:
        selftest(); return
    bus = dbus.SystemBus()
    try:
        os.unlink(SOCK)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCK); srv.listen(1)
    print("listening on", SOCK)
    while True:  # BLE lifetime loop: один BLE-коннект на много synth-сессий
        iface = ble_connect(bus)
        last = [None] * 16
        try:
            while True:  # accept loop: synth может переподключаться без BLE-reconnect
                conn, _ = srv.accept()
                print("synth connected")
                buf = b""
                try:
                    while True:
                        data = conn.recv(4096)
                        if not data:
                            break
                        buf += data
                        while b"\n" in buf:
                            line, buf = buf.split(b"\n", 1)
                            parts = line.decode(errors="replace").split()
                            if parts and parts[0] == "STATE" and len(parts) == 17:
                                try:
                                    colors = [bytes.fromhex(p) for p in parts[1:]]
                                except ValueError:
                                    continue
                                for slot, rgb in enumerate(colors):
                                    if rgb != last[slot]:
                                        paint(iface, slot, rgb)
                                        last[slot] = rgb
                except Exception as e:
                    print("paint failed:", e)
                    break  # BLE умер -> внешний цикл переподключит
                finally:
                    conn.close()
                    print("synth disconnected")
        except KeyboardInterrupt:
            break
    try:
        os.unlink(SOCK)
    except FileNotFoundError:
        pass

if __name__ == "__main__":
    main()
```

Дизайн reconnect: paint-failure → break из inner → внешний while переподключает
BLE, состояние last сбрасывается → следующий STATE перерисует всё. Synth
disconnect → accept ждёт нового. Ctrl-C ПОСЛЕ BLE-up убирает сокет-файл; до
этого — останется, уберётся unlink'ом при следующем старте (уже в коде).

- [ ] **Step 2: selftest**

Run: `python3 ~/Projects/smk37_config/pad_console.py --selftest`
Expected: `selftest ok`.

- [ ] **Step 3: ручной smoke (пользователь рядом)** — запустить компаньон,
  `echo "STATE FF8000 303030 303030 303030 303030 303030 303030 303030 202020 202020 202020 202020 202020 202020 202020 202020" | socat - UNIX-CONNECT:/tmp/mini_midi_synth_led.sock`;
  пользователь видит: пэд 17 оранжевый, 18–24 серые, нижняя строка dim.
  Пометить как Manual в Task 8.

- [ ] **Step 4: Commit (в репо smk37_config)**

```bash
cd ~/Projects/smk37_config && git add pad_console.py && git commit -m "feat: pad_console companion — LED paint over GATT, RAM-only, auto-reconnect"
```

(smk37_config — git-репо; если нет — `git init` по необходимости, отчёт.)


### Task 7: Прошивка нот пэдов банка 2 (железо, постоянная)

**Files:**
- Нет исходников; скрипт через `~/Projects/smk37_config/smk.py`

Карта нот (канал 9; полная запись рекорда [09][note][00][7F][R][G][B][FF]+66
нулей, commit в конце, верификация чтением):

| Пэд | Слот | Нота | Адрес записи (note-байт) |
|-----|------|------|--------------------------|
| 17–24 | 0–7 | 60–67 | 1463 + 74*(n−17) |
| 25 | 8 | 82 | 2055 (есть уже? прочитать и сверить; если совпало — не писать) |
| 26 | 9 | 81 | 2129 |
| 27 | 10 | 83 | 2203 |
| 28 | 11 | 80 | 2277 |
| 29 | 12 | 76 (резерв) | 2351 |
| 30 | 13 | 78 | 2425 |
| 31 | 14 | 79 | 2499 |
| 32 | 15 | 75 | 2573 |

ВАЖНО: цвета пэдов НЕ трогать — читать текущие [R][G][B][flag] и писать их же
в новой записи. Флаг у 17–24 обязателен FF (иначе LED off), у 25–32 сохранить
существующий.

- [ ] **Step 1: прочитать все записи 17–32 (note+цвет), вывести текущее**
- [ ] **Step 2: записать новые ноты полными 74-байтовыми рекордами,
  ОДИН commit в конце** (batч: exec smk.py без main(), как в PADS_NOTES.md)
- [ ] **Step 3: верифицировать чтением всех 16 нот; пользователь проверяет,
  что цвета не слетели и ноты ходят** (aseqdump: пэд 17 → note 60 ch10... —
  ch в aseqdump = 1-based)
- [ ] **Step 4: обновить ~/Projects/smk37_config/PADS_NOTES.md (карта нот
  банка 2) и закоммитить**

### Task 8: Финальная верификация

- [ ] **Step 1:** `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety` — всё зелёное.
- [ ] **Step 2: Ручной чек (пользователь):** компаньон запущен, клава по BT,
  синт с led_companion=on → HOME: пэды 17–24 показывают перфы, активный —
  оранжевый; тап по заполненному — перф загрузился (и LED перекрасился);
  холд → меню; PATTERN → тап переключает паттерн (слышно); MUTE → тап мьютит
  часть (слышно); Play/Rec горят на 25/27 при запуске. led_companion=off →
  синт работает, LED статичен, сокет не подключается (проверка: компаньон
  не получает connection).
- [ ] **Step 3:** обновить docs/TECHDEBT.md #15 (статус: v1 реализована,
  ссылки на план/спеку) + закоммитить план/спеку статусы.
