# Note Flash Decay Keyboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-02 (база = origin/master после #55, worktree `~/Projects/mini_midi_synth-verify`)
status: executed 2026-11-02

**Goal:** Быстрые ноты видны на клавиатуре: каждое NoteOff оставляет затухающий след 120 мс на клавише, независимо от длительности нажатия.

**Architecture:** Атомики (note_state) остаются источником «held» — они точны для нот длиннее кадра. Проблема только в коротких нотах: on→off умещается в кадр, атомик возвращается в 0 до чтения GUI. Решение — события NoteOff: MIDI-input публикует `NoteFlash { note }` в глобальное кольцо (прецедент CONSOLE_TX — SameLock, SameThread, тот же файл), GUI каждый кадр дренирует кольцо и держит `until[note] = now + 120ms`; отрисовка: atom vel>0 → полная яркость; иначе затухание по until. Research 2026-11-02: decay подменяет min-hold (консенсус KeyCastr/keyviz/MIDIVisualizer).

**Tech Stack:** Rust, rtrb 0.3 (Producer НЕ Clone — поэтому глобальный `OnceLock<Mutex<Producer>>` по прецеденту CONSOLE_TX, push только из MIDI-input потока, НЕ из аудио — RT-инварианты), egui 0.31.1.

**Проверенные ревью/компиляцией факты (не пересматривать):** `Instant` не реализует Default → массив init через `std::array::from_fn(|_| Instant::now())`; `Window`/`Memory::any_popup_open()` naming — не относится; `parse_and_push` (midi.rs:138-153) живёт в MIDI-input потоке (midir callback :292-294 / JNI lib.rs:1085-1109 — не RT), blocking Mutex::lock в нём — прецедент; push размещать ДО console early-return (:153); Android-USB путь (audio.rs:244-251) разбирает в аудио-колбэке напрямую — туда push НЕ добавляем (Mutex в RT-зоне запрещён — задокументированное ограничение: на Android быстрые ноты на клавиатуре не флэшатся).

---

## Test Cases (from the requirement)

Структура NoteFlashState — std-only (без egui), юнит-тесты в её модуле:

- TC1: NoteOff событие → until = now+120ms, brightness(note, held=false) > 0 сразу после, 0 через 130 мс (тест со сдвигом времени — сделать until управляемым: структура хранит Instant, тест подставляет прошлое).
- TC2: held=true → brightness = 1.0 при любом until (атомик главный).
- TC3: переполнение кольца (push при полном) — событие теряется без паники; дрен опустошает без паники.
- TC4: note ≥128 (clamp в drain) — skip, без паники.

Экран (Task 4): быстрая гамма — каждая нота видна со следом; аккорды — все; затухание ~120 мс; без залипаний.

## Negative Matrix

- пустое кольцо — дрен no-op; NoteOff без NoteOn — хвост всё равно ставится (это и есть кейс быстрой ноты, событие одно); нота ≥128 — skip; переполнение кольца — потеря flash-события (косметика); двойной NoteOff — until перезаписан; atom/race — atom главный, кольцо только добавляет хвост (не может «выключить» held). Остальное N/A.

---

### Task 1: midi.rs NoteFlash + кольцо + GUI NoteFlashState + дрен

**Files:**
- Modify: `src/midi.rs` (тип, глобальный producer, init, push в parse_and_push)
- Create: `src/gui/note_flash.rs` (NoteFlashState — std-only, юнит-тесты)
- Modify: `src/gui/mod.rs` (`pub mod note_flash;`, App-поля, дрен в update())
- Modify: `src/lib.rs` (Common-поле, init)

**Ревью-исправления встроены:** тип `NoteFlash` определяется в midi.rs УЖЕ в этом таске (иначе Task 1 не компилируется); тесты — толеранс `> 0.99` вместо `== 1.0` (мс-трункейт); `pub mod note_flash;` (видимость из lib.rs); `impl Default` (clippy new_without_default); push — в ветке обновления note_state ПОСЛЕ console early-return (console notes ch9 — клавиатуру не флэшат).

1. `src/midi.rs` — рядом с CONSOLE_TX (:23-24):

```rust
/// Событие затухания для GUI-клавиатуры (NoteOff / NoteOn vel==0).
#[derive(Clone, Copy)]
pub struct NoteFlash {
    pub note: u8,
}

static NOTE_FLASH_TX: OnceLock<Mutex<rtrb::Producer<NoteFlash>>> = OnceLock::new();

/// Вызывается один раз в init_common (до подключения MIDI-инпута).
pub fn init_note_flash(producer: rtrb::Producer<NoteFlash>) {
    let _ = NOTE_FLASH_TX.set(Mutex::new(producer));
}

fn push_note_flash(note: u8) {
    if let Ok(tx) = NOTE_FLASH_TX.get().map(|m| m.lock()) {
        let _ = tx.push(NoteFlash { note: note & 127 });
    }
}
```

В `parse_and_push` (:118-184) — в ветке обновления note_state (ПОСЛЕ console early-return :152, console notes ch9 клавиатуру не флэшат) на NoteOff И NoteOn с vel==0 добавить `push_note_flash(note);` (имя переменной — по факту кода; `Mutex`/`rtrb::Producer` уже импортированы).

2. `src/gui/note_flash.rs` (новый):

```rust
//! Затухающие следы нот для клавиатуры. Held — из атомиков note_state,
//! здесь только хвост после NoteOff (быстрые ноты: атомик on→off в кадре).

use super::super::midi::NoteFlash;
use std::time::{Duration, Instant};

pub const FLASH_DECAY_MS: u64 = 120;

pub struct NoteFlashState {
    until: [Instant; 128],
}

impl Default for NoteFlashState {
    fn default() -> Self {
        Self { until: std::array::from_fn(|_| Instant::now()) }
    }
}

impl NoteFlashState {
    pub fn new() -> Self { Self::default() }

    pub fn flash(&mut self, note: u8) {
        let n = (note & 127) as usize;
        self.until[n] = Instant::now() + Duration::from_millis(FLASH_DECAY_MS);
    }

    /// Яркость хвоста 0..1; held (по атомику) всегда 1.0.
    pub fn brightness(&self, note: usize, atom_held: bool) -> f32 {
        if atom_held { return 1.0; }
        let left = self.until[note.min(127)]
            .saturating_duration_since(Instant::now());
        ((left.as_millis() as f32) / FLASH_DECAY_MS as f32).clamp(0.0, 1.0)
    }

    pub fn any_active(&self) -> bool {
        self.until.iter().any(|&t| t > Instant::now())
    }

    /// Дрен кольца. Возвращает true, если были события (repaint-хинт).
    pub fn drain(&mut self, rx: &mut rtrb::Consumer<NoteFlash>) -> bool {
        let mut seen = false;
        while let Ok(ev) = rx.pop() {
            self.flash(ev.note);
            seen = true;
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc1_off_sets_decay_tail() {
        let mut st = NoteFlashState::new();
        st.flash(60);
        assert!(st.brightness(60, false) > 0.99, "сразу после note-off след полный");
    }

    #[test]
    fn tc2_held_atom_wins() {
        let st = NoteFlashState::new();
        assert_eq!(st.brightness(60, true), 1.0);
    }

    #[test]
    fn tc5_decay_reaches_zero() {
        let mut st = NoteFlashState::new();
        st.flash(60);
        // Сдвиг в прошлое — имитация истечения
        st.until[60] -= Duration::from_millis(200);
        assert_eq!(st.brightness(60, false), 0.0);
    }

    #[test]
    fn tc3_ring_overflow_no_panic() {
        let (mut tx, mut rx) = rtrb::RingBuffer::<NoteFlash>::new(4);
        for _ in 0..16 { let _ = tx.push(NoteFlash { note: 60 }); }
        let mut st = NoteFlashState::new();
        assert!(st.drain(&mut rx));
        assert!(!st.drain(&mut rx));
    }

    #[test]
    fn tc4_note_clamped() {
        let mut st = NoteFlashState::new();
        st.flash(200);
        assert!(st.brightness(127, false) > 0.9);
    }
}
```

(тест tc5: `Instant - Duration` поддержан std; `until` — приватное поле того же модуля, доступ ок.)

3. `src/gui/mod.rs`: `pub mod note_flash;` (видимость из lib.rs — App-литерал там); App-поля:

```rust
    pub flash: note_flash::NoteFlashState,
    pub flash_rx: rtrb::Consumer<crate::midi::NoteFlash>,
```

lib.rs: Common-поле `flash_rx: rtrb::Consumer<crate::midi::NoteFlash>` (рядом с console_rx :99) + init `midi::init_note_flash(flash_producer)` в init_common (:186-204 рядом с CONSOLE_TX.set) + App-литерал (:766): `flash: gui::note_flash::NoteFlashState::new(), flash_rx: c.flash_rx,`.

4. Дрен в update() (gui/mod.rs, до отрисовки):

```rust
        // NoteFlash: быстрые ноты — атомик on→off в кадре, событие терялось
        let seen = self.flash.drain(&mut self.flash_rx);
        if self.flash.any_active() || seen {
            ctx.request_repaint_after(Duration::from_millis(33));
        }
```

(egui request_repaint_after — min-merge: конфликт с 250ms-idle веткой не страшен.)

- [ ] Step 1: midi.rs + note_flash.rs + перенос, clippy + cargo test (TC1-TC5 зелёные)
- [ ] Step 2: commit `feat: NoteFlash ring + decay state`

### Task 2: Клавиатура — blend затухания

**Files:** Modify: `src/gui/keyboard.rs` — белые (:92-101) и чёрные (:111-121) клавиши

Файл-level хелпер + цветовые правки (цвета — theme константы, имена сверены: KEY_WHITE_IDLE/KEY_WHITE_ACTIVE/BG_BLACK_KEY/KEY_BLACK_ACTIVE):

```rust
fn blend(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    egui::Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
```

Белые (вместо текущего `vel > 0 → активный / иначе idle`):

```rust
                let vel = self.note_state[note as usize].load(Ordering::Relaxed);
                let held = vel > 0;
                let tail = self.flash.brightness(note as usize, held);
                let color = blend(theme::KEY_WHITE_IDLE, theme::KEY_WHITE_ACTIVE, tail);
```

Чёрные — аналогично с BG_BLACK_KEY/KEY_BLACK_ACTIVE.

(Aтомик остаётся источником held; кольцо только добавляет затухающий хвост после NoteOff — гонки нет: atom-ветка побеждает при vel>0 независимо от порядка событий.)

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `feat: keyboard decay tails — fast notes visible`

### Task 3: Верификация

- [ ] clippy/test/rt_safety; `cargo run --release`
- [ ] Экран: быстрая гамма — каждая нота видна со следом ~120 мс; аккорды — все ноты; затухание без залипаний; SF2/DSP — звук не изменился
- [ ] PR → squash merge → перезапуск release

## Out of scope

- Android-USB flash events (Mutex в RT-зоне) — documented limitation.
- LED-консоль/пэды — тот же класс болезни, отдельный проход.
- Piano-roll/частицы — YAGNI.
