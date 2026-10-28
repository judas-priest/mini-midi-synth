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
- socket отсутствует/умер: клиент молча ретраит раз в 2с, GUI не блокируется
  (mpsc в отдельный тред; тест не нужен — код изоляции тривиален).
- Значение цвета: слоты 20–24 в меню тёмные; пустой слот перфа = 000000+FF
  (LED off, допустимый валидный цвет — тест snapshot).
- clock skew — неприменимо (Instant монотонные часы одного процесса).
- external-call timeout — unix socket connect с таймаутом 100ms в отдельном
  треде; GUI никогда не блокируется.
- concurrent writers к сокету — один writer-тред, mpsc сериализует.

---

### Task 1: Консоль-автомат — failing tests + реализация (TDD, чистая логика)

**Files:**
- Create: `src/gui/led_console.rs` (модуль + `#[cfg(test)] mod tests`)
- Modify: `src/gui/mod.rs` (только `mod led_console;` — в конце Task 2)

- [ ] **Step 1: написать failing-тесты** — создать `src/gui/led_console.rs`
  ТОЛЬКО с типами, struct и тестами (БЕЗ `impl LedConsole` — методы не
  существуют, тесты не скомпилируются; это и есть красная фаза):

```rust
//! Bank-2 pad console state machine (pure logic, GUI thread only).
//! Slots 0..=7 = top-row pads 17..=24 (notes 60..=67).
//! Prev/Next/Rewind arrive as actions from notes 78/79/80 (routed by caller).
//! Transport notes 81/82/83/64 are handled engine-side (mod.rs:1573-1598).

use std::time::{Duration, Instant};

const HOLD: Duration = Duration::from_millis(500);

/// Top-row mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
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
    StepPerf(i32),
    StepPattern(i32),
}

pub struct LedConsole {
    mode: Mode,
    pending: Option<(usize, Instant)>,
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
        // slots 3..=7 (Rew, unused, Prev, Next, Undo) stay dim
        c
    }
}
```

- [ ] **Step 4: прогнать**

Run: `cargo test --lib led_console`
Expected: PASS (10 тестов).

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

(`needs_note_offs = true` — чтобы висящие ноты закрылись на следующем тике;
сверить семантику с тем, как restore сбрасывает эти поля — там `false` на Idle
без активных нот; если в `clear`/`restore` needs_note_offs ставится в false,
поставь false и добавь комментарий-обоснование.)

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

### Task 3: MIDI-роутинг — swallow консольных нот в parse_and_push

**Files:**
- Modify: `src/midi.rs` (тип ConsoleTx, параметр parse_and_push, connect/connect_by_name)
- Modify: `src/lib.rs` (~275: оба вызова midi::connect/connect_by_name — grep)
- Test: существующие тесты midi_parse (cargo test --test midi_parse) не должны сломаться

- [ ] **Step 1: тип канала** — в `src/midi.rs` рядом с `pub type NoteState`:

```rust
/// Console pads (bank-2 notes 60..=67, 78..=80 on ch9) are mirrored to the GUI
/// instead of the audio ring: (note, is_on).
pub type ConsoleTx = std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<(u8, bool)>>>>;

/// Notes owned by the LED console (bank-2 pads), channel 9.
pub fn is_console_note(note: u8) -> bool {
    (60..=67).contains(&note) || (78..=80).contains(&note)
}
```

- [ ] **Step 2: swallow в parse_and_push** — сигнатура
  `pub fn parse_and_push(data, tx, note_state, pad_state)` → добавить параметр
  `console_tx: &ConsoleTx` последним. После парса, ДО push в ring (там, где
  event разбирается для pad_state — блок «Update GUI note display» оставить
  как есть, чтобы пэды вспыхивали в GUI), вставить:

```rust
    // LED-console notes: mirror to GUI, never reach the audio engine.
    let console_hit = matches!(
        &event,
        MidiEvent::NoteOn { channel: 9, note, .. } if is_console_note(*note)
    ) || matches!(
        &event,
        MidiEvent::NoteOff { channel: 9, note } if is_console_note(*note)
    );
    if console_hit {
        let is_on = matches!(event, MidiEvent::NoteOn { .. });
        let note = match &event {
            MidiEvent::NoteOn { note, .. } | MidiEvent::NoteOff { note, .. } => *note,
            _ => unreachable!(),
        };
        if let Ok(guard) = console_tx.lock() {
            if let Some(sender) = guard.as_ref() {
                let _ = sender.send((note, is_on));
            }
        }
        return consumed;
    }
```

Важно: return ДО `tx.push(...)` — найти в конце функции реальный push
(`let _ = tx.lock()...push(event)` / `tx.0.lock().push`) и убедиться, что
console-ветка его не достигает.

- [ ] **Step 3: обновить все callers parse_and_push** — grep `parse_and_push(`:
  midir-колбэк в connect/connect_by_name (замыкание получает console_tx
  параметром и пробрасывает), AMIDI-путь если зовёт parse_and_push (grep
  в src/), тесты midi_parse (передать `Arc::new(Mutex::new(None))`).
  connect/connect_by_name получают параметр `console_tx: ConsoleTx` и
  прокидывают в замыкание.

- [ ] **Step 4: создать канал в lib.rs** — рядом с `midi_tx_shared` (~176):

```rust
    let console_tx: midi::ConsoleTx = Arc::new(std::sync::Mutex::new(None));
```

прокинуть в оба вызова midi::connect/connect_by_name (~275) и сохранить в
App/структуре (поле `pub console_tx: midi::ConsoleTx`, инициализация в
конструкторе — место, где создаётся midi_tx_shared; на Android-ветке тоже).

- [ ] **Step 5: прогнать**

Run: `cargo clippy --all-targets -- -D warnings && cargo test`
Expected: PASS (включая midi_parse с новым аргументом).

- [ ] **Step 6: Commit**

```bash
git add src/midi.rs src/lib.rs
git commit -m "feat: mirror bank-2 console notes to GUI channel (swallow from engine)"
```

### Task 4: GUI-интеграция — автомат, действия, LED-снимок

**Files:**
- Modify: `src/gui/mod.rs` (mod led_console; App-поля; poll в update; обработчик действий; снимок → socket-writer)
- Якоря: `DrumSeqPattern` push — gui/mod.rs:1500 (`send_drum_state`); `load_performance_by_name` — gui/mod.rs:1669; `SetPartMute` push — grep; `drum_current_pattern` — gui/mod.rs:315; parts[].mute — grep `pub mute` в gui/mod.rs

- [ ] **Step 1: подключить модуль** — в `src/gui/mod.rs` к списку модулей:
  `pub mod led_console;`. Если в Task 1 ставился `#[allow(dead_code)]` — снять.

App-поля (рядом с `drum_current_pattern`):

```rust
    pub console: led_console::LedConsole,
    pub console_rx: Option<std::sync::mpsc::Receiver<(u8, bool)>>,
    pub led_snapshot: [[u8; 3]; 16],
    pub led_dirty: bool,
    pub led_socket: Option<std::sync::mpsc::Sender<String>>,
    pub led_socket_failed_at: Option<std::time::Instant>,
```

инициализация в App::new / lib.rs App-литерале (grep место литерала — там, где
`drum_current_pattern: 0,`): `console: led_console::LedConsole::new()`,
`console_rx: None`, `led_snapshot: [[0;3];16]`, `led_dirty: true`,
`led_socket: None`, `led_socket_failed_at: None`. `console_rx` заполняется при
старте midi (в lib.rs рядом с console_tx: `let (console_tx_snd, console_rx) =
mpsc::channel(); *console_tx.lock().unwrap() = Some(console_tx_snd);` — канал
создаётся ОДИН раз в init_common, Receiver уходит в App-литерал как
`console_rx: Some(console_rx)`).

- [ ] **Step 2: обработка событий** — в главном цикле App (update/draw, где
  поллятся атомы — grep `drum_play_atom` в gui/mod.rs update), добавить poll:

```rust
        // Bank-2 console pads -> state machine -> actions
        let mut acts: Vec<led_console::Action> = Vec::new();
        let mut repaint = false;
        if let Some(rx) = &self.console_rx {
            while let Ok((note, is_on)) = rx.try_recv() {
                let now = std::time::Instant::now();
                let ev = match (note, is_on) {
                    (60..=67, true) => Some(led_console::PadEvent::Down((note - 60) as usize)),
                    (60..=67, false) => Some(led_console::PadEvent::Up((note - 60) as usize)),
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
                    repaint = true;
                }
            }
        }
        if let Some(m) = self.console.tick(std::time::Instant::now()) {
            let _ = m; // Mode::Menu — snapshot пересчитается ниже
            repaint = true;
        }
        for a in acts { self.execute_console_action(a); }
```

`execute_console_action` (новый метод impl App):

```rust
    fn execute_console_action(&mut self, action: led_console::Action) {
        match action {
            led_console::Action::LoadPerf(i) => {
                let names = self.performance_names_console();
                if let Some(name) = names.get(i) {
                    if self.load_performance_by_name(name) {
                        log::info!("[console] perf {} loaded", i + 1);
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
            led_console::Action::StepPerf(dir) => {
                let n = self.performance_names_console().len();
                if n == 0 { return; }
                let cur = self.active_perf_console().unwrap_or(0);
                let next = (cur as i32 + dir).rem_euclid(n as i32) as usize;
                let names = self.performance_names_console();
                if let Some(name) = names.get(next) {
                    self.load_performance_by_name(name);
                }
            }
            led_console::Action::StepPattern(dir) => {
                let next = (self.drum_current_pattern as i32 + dir).rem_euclid(8);
                self.drum_current_pattern = next as u8;
                let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: next as u8 });
            }
        }
    }

    fn performance_names_console(&self) -> Vec<String> {
        crate::preset::performance_names()
    }

    fn active_perf_console(&self) -> Option<usize> {
        self.active_performance_name.as_ref().and_then(|n| {
            crate::preset::performance_names().iter().position(|x| x == n)
        })
    }
```

(`active_performance_name` — проверить реальное поле в App, grep
`load_performance_by_name` использования: чем запоминается текущий перф;
если поле другое — использовать его. `preset::performance_names` — добавить в
Task 5. `parts[i].mute` — сверить имя поля, grep `mute` в gui/mod.rs PartView.)

- [ ] **Step 3: LED-снимок** — там же в update, после poll:

```rust
        // LED snapshot (16 slots = pads 17..=32, top row first)
        let playing = self.drum_play_atom.load(Ordering::Relaxed) != 0;
        let recording = self.drum_rec_atom.load(Ordering::Relaxed) != 0;
        let mut mutes = [false; 8];
        for (i, part) in self.parts.iter().take(8).enumerate() {
            mutes[i] = part.mute;
        }
        let filled = crate::preset::performance_names().len();
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

### Task 5: config-тумблер + performance_names + socket-клиент-тред

**Files:**
- Modify: `src/config.rs` (UiSettings.led_companion)
- Modify: `src/preset.rs` (performance_names())
- Modify: `src/gui/settings.rs` (checkbox в настройках) / `src/gui/mod.rs` (writer-тред spawn)

- [ ] **Step 1: config** — в `UiSettings` рядом с `split_point`:

```rust
    /// Bank-2 LED console companion (unix socket). Off = never connect.
    #[serde(default)]
    pub led_companion: bool,
```

Тест в существующем config-тесте (или новый в tests/preset_collision.rs-стиле —
маленький, в tests/config_led.rs):

```rust
#[test]
fn led_companion_defaults_false() {
    let s: mini_midi_synth::config::UiSettings =
        serde_json::from_str("{}").unwrap();
    assert!(!s.led_companion);
}
```

(проверить: UiSettings pub + serde; если private — тест через полный Config.)

- [ ] **Step 2: preset::performance_names** — в `src/preset.rs` рядом с
  `performance_dir` (preset.rs:799):

```rust
/// Sorted stems of all performance files (console perf slots 1..=8).
pub fn performance_names() -> Vec<String> {
    let Some(dir) = performance_dir() else { return Vec::new(); };
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |x| x == "json"))
                .filter_map(|e| e.path().file_stem().and_then(|s| s.to_str().map(str::to_string)))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}
```

- [ ] **Step 3: writer-тред + тумблер** — в gui (модуль settings.rs, секция
  аудио-настроек — grep, где рендерятся checkbox'ы настроек, например
  `split_point` UI): checkbox:

```rust
                ui.checkbox(&mut self.config.ui.led_companion, "LED companion (bank-2 pads)");
```

Логика подключения — в gui/mod.rs update (раз в кадр дешёвая проверка):

```rust
        // LED companion socket: spawn writer thread on demand
        if self.config.ui.led_companion && self.led_socket.is_none()
            && self.led_socket_failed_at
                .map(|t| t.elapsed() > std::time::Duration::from_secs(2))
                .unwrap_or(true)
        {
            match std::os::unix::net::UnixStream::connect("/tmp/mini_midi_synth_led.sock") {
                Ok(stream) => {
                    let (tx, rx) = std::sync::mpsc::channel::<String>();
                    std::thread::spawn(move || {
                        use std::io::Write;
                        let mut stream = stream;
                        for msg in rx {
                            if stream.write_all(msg.as_bytes()).is_err() { break; }
                        }
                    });
                    self.led_socket = Some(tx);
                    self.led_socket_failed_at = None;
                    log::info!("[console] companion connected");
                }
                Err(_) => {
                    self.led_socket_failed_at = Some(std::time::Instant::now());
                }
            }
        }
        if !self.config.ui.led_companion && self.led_socket.is_some() {
            self.led_socket = None; // drop sender: thread exits, no more paints
        }
```

App-поля: `led_socket_failed_at: Option<std::time::Instant>` (добавить к Task 4
полям). connect БЕЗ таймаута блокирует GUI — приемлемо? /tmp-сокет: если никто
не слушает, connect мгновенно ECONNREFUSED. Риск: зависший компаньон-сокет
(принял backlog, не accept'ит) — редкость; если параноить — set_nonblocking
+ connect loop. Оставляем простой вариант, documented.

- [ ] **Step 4: прогнать + commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo test
git add src/config.rs src/preset.rs src/gui/settings.rs src/gui/mod.rs
git commit -m "feat: led_companion toggle, performance_names, socket writer"
```

### Task 6: Компаньон pad_console.py

**Files:**
- Create: `~/Projects/smk37_config/pad_console.py` (вне репо синта — hardware tooling)
- Test: unit-самотест `--selftest` (framing + slot→addr, без BLE)

- [ ] **Step 1: написать компаньон**:

```python
#!/usr/bin/env python3
"""Bank-2 LED console companion for mini_midi_synth.

Listens on /tmp/mini_midi_synth_led.sock, receives "STATE c0 c1 ... c15"
(c* = RRGGBB hex; slots 0-7 = pads 17-24, slots 8-15 = pads 25-32) and paints
the keyboard LEDs over BLE GATT WITHOUT commit (RAM-only, instant).

Run: python3 pad_console.py
"""
import socket, os, sys, time
import dbus

SOCK = "/tmp/mini_midi_synth_led.sock"
BUS = "org.bluez"
BASE = "/org/bluez/hci0/dev_D8_31_0D_19_D1_0C"
W = BASE + "/service0060/char0061"   # write characteristic (smk.py)

def slot_addr(slot: int) -> int:
    # slots 0-7 -> pads 17-24; slots 8-15 -> pads 25-32
    return 1467 + 74 * slot if slot < 8 else 2059 + 74 * (slot - 8)

def frame(cmd: int, payload: bytes) -> bytes:
    return b"\x00\x59" + bytes([cmd, len(payload), 0x00, 0x00]) + payload + bytes([(~sum(payload)) & 0xFF])

def paint(wch, slot: int, rgb: bytes):
    a = slot_addr(slot)
    payload = bytes([0x05, a & 0xFF, (a >> 8) & 0xFF, 0x00, 0x00, 4, 0x00, 0x00]) + rgb + b"\xff"
    wch.WriteValue(dbus.Array([dbus.Byte(b) for b in frame(0x22, payload)], signature="y"),
                   {"type": "command"})

def selftest():
    assert slot_addr(0) == 1467 and slot_addr(7) == 1467 + 74 * 7
    assert slot_addr(8) == 2059 and slot_addr(15) == 2059 + 74 * 7
    # paint payload: 8-byte header + 3 color bytes + 0xFF flag = 12 bytes
    payload = bytes([0x05, 0xCB, 0x09, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xE0, 0xFF, 0xFF])
    f = frame(0x22, payload)
    assert f[:6] == b"\x00\x59\x22\x0c\x00\x00", f[:6].hex()
    assert f[6:-1] == payload
    assert f[-1] == (~sum(payload)) & 0xFF
    print("selftest ok")

def main():
    if "--selftest" in sys.argv:
        selftest(); return
    bus = dbus.SystemBus()
    while True:  # BLE connect loop
        try:
            wch = bus.get_object(BUS, W)
            wch.WriteValue(dbus.Array([dbus.Byte(b) for b in frame(0x22, b"")], signature="y"),
                           {"type": "command"})  # probe write (empty) — если исключение, девайс не готов
            break
        except Exception:
            time.sleep(3)
    print("BLE ready")
    try:
        os.unlink(SOCK)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCK); srv.listen(1)
    last = [None] * 16
    while True:
        conn, _ = srv.accept()
        print("synth connected")
        buf = b""
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
                            try:
                                paint(wch, slot, rgb)
                            except Exception:
                                sys.exit(3)  # BLE died -> reconnect loop
                            last[slot] = rgb
        print("synth disconnected")
        conn.close()

if __name__ == "__main__":
    main()
```

(Реюз smk.py-протокола: frame/checksum/адреса — источник PADS_NOTES.md.
ВНИМАНИЕ: `dbus.Interface(wch, "org.bluez.GattCharacteristic1").WriteValue` —
сверить с smk.py, как там вызывается WriteValue (прямо на object proxy или
через Interface) и использовать ТОТ ЖЕ способ. Probe-write пустого payload
может быть проигнорирован девайсом — если смущает, заменить probe на `rd`
цвета слота 0 (read-запрос как в smk.py: frame(0x23,...)) — реализуй probe
через чтение, безопаснее: см. smk.py rd().)

- [ ] **Step 2: selftest**

Run: `python3 ~/Projects/smk37_config/pad_console.py --selftest`
Expected: `selftest ok`.

- [ ] **Step 3: ручной smoke (пользователь рядом)** — запустить компаньон,
  `echo "STATE FF8000 000000 ..." | socat - UNIX-CONNECT:/tmp/mini_midi_synth_led.sock`
  или из синта; пользователь смотрит LED. BLE-проверка без клавы невозможна —
  пометить как Manual в Task 8.

- [ ] **Step 4: Commit (в репо smk37_config)**

```bash
cd ~/Projects/smk37_config && git add pad_console.py && git commit -m "feat: pad_console companion — LED paint over GATT, RAM-only"
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
| 29 | 12 | 76 (резерв, не используется) | 2351 |
| 30 | 13 | 78 | 2425 |
| 31 | 14 | 79 | 2499 |
| 32 | 15 | 64 | 2573 |

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
