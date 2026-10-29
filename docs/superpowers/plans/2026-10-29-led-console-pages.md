# LED Console: Pages + Window Auto-follow + Undo LED Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ 7ed7f09 чтением кода в этой сессии)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Страницы перформансов/паттернов на пэдах 28/29 (⏪⏩, оттенки синего по
глубине страницы), окно автоподтягивается за активным (пурпур никогда не за
экраном), Undo (32) розовым и только когда есть что откатывать, Rec (27)
автоматически включает Play.

**Architecture:** Окно (start-индекс видимых целей) живёт в GUI (App.perf_window /
pat_window) — консоли длина списка не нужна. PAGE-ноты (80◀, 76▶) двигают окно
активного режима; Step/Load вызывают авто-подтягивание. Движок получает
`undo_atom` (есть ли снимок), GUI читает его для LED Undo. Оттенки синего и
розовый FF5080 — ЭКСПЕРИМЕНТАЛЬНЫЕ (не из validated-палитры): юзер проверяет
рендер, fallback — консистентные проверенные цвета (план фиксирует fallback).

**Tech Stack:** Rust, egui, rtrb/AtomicU8 — без новых зависимостей.

**Spec context:** docs/superpowers/specs/2026-10-29-bank2-led-console-design.md
(страницы — v2-бэклог techdebt #15).

**Проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требования: (1) 28/29 = страницы назад/вперёд; (2) оттенок 28/29 темнеет с
глубиной страницы; (3) при уходе активного перфа/паттерна за пределы окна окно
подтягивается так, что активный — в первом слоте и горит пурпуром; (4) Undo (32)
розовый только когда у секвенсора есть снимок, иначе off; (5) 30/31 НЕ меняют
цвет по режиму (снова фиксированный оранжевый); (6) Rec включает Play если тот
остановлен; (7) PAGE не действует, когда всё влезает в одну страницу.

## Negative Matrix

- PAGE◀ на первой странице → окно clamp до 0 (не паникует, saturating_sub).
- PAGE▶ на последней странице → clamp до len-7 (если len>7; иначе no-op).
- Auto-follow при активном индексе за концом списка — невозможен (индексы из GUI clamp по len).
- undo_atom при загрузке движка → false (нет снимка) — тест.
- undo после undo → снимок consumed → atom false (undo() делает take() — тест).
- Rec повторно при уже пишущем → auto-play не срабатывает (только при включении записи) — тест.
- window уже 0 и PAGE◀ → остаётся 0.
- clock skew — неприменимо (Instant монотонный).
- concurrent PAGE/Step — GUI-поток один, mpsc сериализует.

---

### Task 1: Движок — undo_atom + Rec auto-play

**Files:**
- Modify: `src/synth/drum.rs` (StepSequencer: atom; new/save_snapshot/clear_pattern/undo; accessor)
- Modify: `src/synth/mod.rs` (arm 83 auto-play; DrumSeqRecord arm auto-play; expose atom через SynthEngine)

StepSequencer атомы живут в DrumEngine (step/play/rec: drum.rs:875-878, accessor'ы :926-936, New :920-922). По образцу:

- [ ] **Step 1: failing-тест** — в `src/synth/drum.rs` добавить тест рядом с
  существующими drum-тестами (grep `mod tests` в drum.rs; если нет inline-тестов —
  тесты лежат в tests/drum_remap.rs, добавить туда):

```rust
#[test]
fn undo_atom_tracks_snapshot_lifecycle() {
    let mut e = DrumEngine::new(48_000.0);
    assert!(!e.undo_available(), "no snapshot at start");
    e.sequencer.save_snapshot();
    assert!(e.undo_available());
    e.sequencer.undo();
    assert!(!e.undo_available(), "snapshot consumed by undo");
    e.sequencer.clear_pattern();
    assert!(e.undo_available());
}

#[test]
fn rec_on_stopped_transport_auto_plays() {
    let mut e = engine_with_init_patch();
    assert!(!e.drum_engine.sequencer.playing);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing, "rec must auto-start play");
    assert!(e.drum_engine.sequencer.recording);
    // toggle off while playing: auto-play must NOT fire
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(!e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "play persists after rec-off");
}
```

(`undo_available`/`engine_with_init_patch` не существуют → red. Второй тест —
в tests/sustain_audit.rs, где живёт harness; импорты MidiEvent уже там.)

- [ ] **Step 2: atom на StepSequencer** (НЕ на DrumEngine: снимок мутируют
  напрямую в 6 местах, включая mod.rs:1594/2071-2081 — атом рядом со снимком
  покрывает все сайты автоматически):

В `StepSequencer` (рядом с полями drum.rs:726):

```rust
    undo_atom: Arc<AtomicBool>,
```

`use std::sync::atomic::AtomicBool` — проверить импорт в drum.rs (AtomicU8 уже
импортирован; AtomicBool добавить в тот же use). В `StepSequencer::new`
(drum.rs:740):

```rust
    undo_atom: Arc::new(AtomicBool::new(false)),
```

Синхронизация ВНУТРИ StepSequencer — в конце `save_snapshot` (~760),
`clear_pattern` (~746) и `undo` (~755):

```rust
        self.undo_atom.store(self.undo_snapshot.is_some(),
                             std::sync::atomic::Ordering::Relaxed);
```

Приватный хелпер не нужен — одна строка в трёх местах. `set_sample_rate`
НЕ трогать: sequencer переносится целиком через mem::replace (drum.rs:961,975),
атом переживает rebuild вместе со снимком.

Accessors — в `impl StepSequencer`:

```rust
    pub fn undo_atom(&self) -> Arc<AtomicBool> {
        self.undo_atom.clone()
    }
    pub fn undo_available(&self) -> bool {
        self.undo_atom.load(std::sync::atomic::Ordering::Relaxed)
    }
```

в `impl DrumEngine` (рядом с `rec_atom()` ~936, делегирование; тест из Step 1
зовёт `e.undo_available()`):

```rust
    pub fn undo_atom(&self) -> Arc<AtomicBool> {
        self.sequencer.undo_atom()
    }
    pub fn undo_available(&self) -> bool {
        self.sequencer.undo_available()
    }
```

- [ ] **Step 3: expose в GUI** — SynthEngine-обёртка НЕ нужна: settings.rs
  зовёт `engine.drum_engine.undo_atom()` напрямую (шаблон `drum_rec_atom`
  в settings.rs:250 зовёт `engine.drum_engine.rec_atom()`).

- [ ] **Step 4: Rec auto-play** — в `src/synth/mod.rs`:
  arm ноты 83 (~1581): после `set_recording(rec);` добавить
  `if rec && !self.drum_engine.sequencer.playing { self.drum_engine.set_playing(true); }`.
  arm `DrumSeqRecord` (~2071): после `sequencer.recording = recording;` добавить
  `if recording && !self.drum_engine.sequencer.playing { self.drum_engine.set_playing(true); }`.
  (сверить имена полей: `sequencer.playing` — прямое поле, mod.rs:2073.)

- [ ] **Step 5: прогнать**

Run: `cargo test --lib && cargo test --test drum_remap && cargo test --test sustain_audit && cargo clippy --all-targets -- -D warnings`
Expected: PASS (undo_atom тест зелёный; rec auto-play тест зелёный).

- [ ] **Step 6: Commit**

```bash
git add src/synth/drum.rs src/synth/mod.rs
git commit -m "feat: sequencer undo_available atom + Rec auto-plays stopped transport"
```

### Task 2: Консоль + GUI — окна, PAGE, auto-follow, undo LED (один атомарный таск)

Сигнатура top_colors меняется → консоль и GUI правятся ВМЕСТЕ, один коммит.

**Files:**
- Modify: `src/gui/led_console.rs` (top_colors окна; bottom palette; PageBack/Forward; тесты)
- Modify: `src/gui/mod.rs` (App-поля окон; PAGE в poll; auto-follow; snapshot с окнами/undo)
- Modify: `src/gui/settings.rs` (drum_undo_atom plumbing)
- Modify: `src/lib.rs` (App-литерал)

- [ ] **Step 1: failing-тесты консоли** — в `src/gui/led_console.rs` заменить
  тесты `snapshot_colors_home_and_menu` и `bottom_transport_colors` на:

```rust
    #[test]
    fn window_offsets_map_targets_to_slots() {
        let mut c = LedConsole::new();
        let mutes = [false; 8];
        // window 2: slot 1 = perf idx 2 (selected), slot 2 = idx 3 (filled)
        let home = c.top_colors(Some(2), 5, 0, &mutes, 2, 0);
        assert_eq!(home[1], [0xFF, 0x00, 0xFF]); // selected (idx 2)
        assert_eq!(home[2], [0x00, 0x66, 0xFF]); // available (idx 3)
        // slot 4 = idx 5 -> за концом списка (count 5) -> OFF
        assert_eq!(home[4], [0x00, 0x00, 0x00]);
        // pattern window 1: slot 1 = pattern 1 (selected при current=1)
        let pat = c.top_colors(None, 0, 1, &mutes, 0, 1);
        assert_eq!(pat[1], [0xFF, 0x00, 0xFF]);
        assert_eq!(pat[2], [0xFF, 0xFF, 0x00]); // pattern mode speaks yellow
    }

    #[test]
    fn bottom_page_shades_and_undo_pink() {
        let mut c = LedConsole::new();
        let b = c.bottom_colors(false, false, 0, false);
        assert_eq!(b[3], [0x00, 0x66, 0xFF]); // page 0: base blue
        assert_eq!(b[4], [0x00, 0x66, 0xFF]);
        assert_eq!(b[7], [0x00, 0x00, 0x00]); // undo off (no snapshot)
        let b = c.bottom_colors(false, false, 2, true);
        assert_eq!(b[3], [0x00, 0x32, 0x77], "page 2: darker blue");
        assert_eq!(b[7], [0xFF, 0x50, 0x80]); // undo pink when available
        // nav 30/31 — фиксированный оранжевый, НЕ зависит от режима
        c.mode = Mode::Pattern;
        let b = c.bottom_colors(false, false, 0, true);
        assert_eq!(b[5], [0xFF, 0x80, 0x00]);
        assert_eq!(b[6], [0xFF, 0x80, 0x00]);
    }
```

Run: `cargo test --lib led_console` — Expected: COMPILE FAIL (сигнатуры/варианты не существуют).

- [ ] **Step 2: top_colors с окнами** — заменить сигнатуру и маппинг:

```rust
    /// LED colors for slots 0..=7 (top row). `w_perf`/`w_pat` — first index of
    /// the visible window: slot i (1..=7) shows item window + i - 1.
    /// Slot 0 is the MENU button (white).
    pub fn top_colors(
        &self,
        active_perf: Option<usize>,
        filled_perfs: usize,
        current_pattern: usize,
        mutes: &[bool; 8],
        w_perf: usize,
        w_pat: usize,
    ) -> [[u8; 3]; 8] {
        const SEL: [u8; 3] = [0xFF, 0x00, 0xFF]; // selected: magenta
        const AVAIL: [u8; 3] = [0x00, 0x66, 0xFF]; // available: blue
        let mut c = [[0u8; 3]; 8];
        if self.mode != Mode::Menu {
            c[0] = [0xFF, 0xFF, 0xFF]; // MENU button (white)
        }
        match self.mode {
            Mode::Home => {
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    let idx = w_perf + i - 1;
                    if Some(idx) == active_perf {
                        *col = SEL;
                    } else if idx < filled_perfs {
                        *col = AVAIL;
                    } else {
                        *col = [0x00, 0x00, 0x00]; // empty: OFF
                    }
                }
            }
            Mode::Pattern => {
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    let idx = w_pat + i - 1;
                    if idx == current_pattern {
                        *col = SEL;
                    } else if idx < 8 {
                        *col = [0xFF, 0xFF, 0x00]; // available: yellow
                    } else {
                        *col = [0x00, 0x00, 0x00];
                    }
                }
            }
            Mode::Mute => {
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    if mutes[i - 1] {
                        *col = [0x00, 0x00, 0x00]; // muted: OFF
                    } else {
                        *col = [0x00, 0xFF, 0x00]; // sounding: green
                    }
                }
            }
            Mode::Menu => {
                for (i, col) in c.iter_mut().enumerate() {
                    *col = match i {
                        0 => AVAIL,              // HOME: blue (perfs)
                        1 => [0xFF, 0xFF, 0x00], // PATTERN: yellow
                        2 => [0x00, 0xFF, 0x00], // MUTE: green
                        _ => [0x00, 0x00, 0x00],
                    };
                }
            }
        }
        c
    }
```

- [ ] **Step 3: bottom_colors со страницами и undo** — заменить целиком:

```rust
    /// Bottom-row colors. `page_depth` — сколько PAGE-нажатий вглубь (0 = база);
    /// оттенки синего темнеют с глубиной (EXPERIMENTAL — юзер проверяет рендер;
    /// fallback: заменить все shades на [0x00, 0x66, 0xFF]).
    /// Undo (slot 15 = pad 32): розовый FF5080 (EXPERIMENTAL; fallback FF00FF)
    /// когда есть снимок, иначе OFF. Prev/Next — фиксированный оранжевый.
    pub fn bottom_colors(
        &self,
        playing: bool,
        recording: bool,
        page_depth: usize,
        undo_available: bool,
    ) -> [[u8; 3]; 8] {
        const PAGE_SHADES: [[u8; 3]; 4] = [
            [0x00, 0x66, 0xFF], // depth 0: validated blue
            [0x00, 0x48, 0xB5], // depth 1
            [0x00, 0x32, 0x77], // depth 2
            [0x00, 0x21, 0x4D], // depth 3+
        ];
        let shade = PAGE_SHADES[page_depth.min(PAGE_SHADES.len() - 1)];
        let mut c = [[0x00, 0x00, 0x00]; 8];
        c[0] = if playing { [0x00, 0xFF, 0x00] } else { [0x00, 0x00, 0x00] };
        c[1] = if !playing && !recording { [0xFF, 0xFF, 0x00] } else { [0x00, 0x00, 0x00] };
        c[2] = if recording { [0xFF, 0x00, 0x00] } else { [0x00, 0x00, 0x00] };
        c[3] = shade; // PAGE back (pad 28)
        c[4] = shade; // PAGE forward (pad 29)
        c[5] = [0xFF, 0x80, 0x00]; // Prev: fixed orange
        c[6] = [0xFF, 0x80, 0x00]; // Next: fixed orange
        c[7] = if undo_available { [0xFF, 0x50, 0x80] } else { [0x00, 0x00, 0x00] };
        c
    }
```

- [ ] **Step 4: PageBack/PageForward** — в `PadEvent` добавить:

```rust
    /// Bottom-row: PAGE back (note 80, pad 28).
    PageBack,
    /// Bottom-row: PAGE forward (note 76, pad 29).
    PageForward,
```

в `on_event`:

```rust
            PadEvent::PageBack => Some(Action::PageBack),
            PadEvent::PageForward => Some(Action::PageForward),
```

в `Action`:

```rust
    PageBack,
    PageForward,
```

ОБЯЗАТЕЛЬНО: `pub fn mode(&self) -> Mode { self.mode }` в impl LedConsole —
сейчас его НЕТ в файле (удалён как dead в прошлой итерации), GUI в Step 6/7
его зовёт. Добавить вместе с остальными правками.

- [ ] **Step 5: App-поля и plumbing** — в gui/mod.rs рядом с led-полями (~327):

```rust
    pub perf_window: usize,   // first perf_list idx visible on slots 1..=7
    pub pat_window: usize,    // first pattern idx visible on slots 1..=7
    pub drum_undo_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
```

lib.rs: `perf_window: 1, pat_window: 0` — в App-литерал (~781);
`drum_undo_atom` НЕ detached-атом: в init_common рядом с созданием
`midi_tx_shared` (~209, где engine доступен) — `let drum_undo_atom =
engine.drum_engine.undo_atom();` — прокинуть через struct CommonInit (поле
рядом с midi_tx_shared, lib.rs:110/329) → App-литерал берёт `drum_undo_atom:
c.drum_undo_atom,` (шаблон: как другие drum-атомы попадают в App).

settings.rs (~250, рядом с drum_rec_atom):

```rust
        self.drum_undo_atom = engine.drum_engine.undo_atom();
```

- [ ] **Step 6: PAGE в poll** — в poll-блоке update заменить arm `(80, true)`
  (сейчас пушит LooperRewind) и добавить 76:

```rust
                    (80, true) => Some(led_console::PadEvent::PageBack),
                    (76, true) => Some(led_console::PadEvent::PageForward),
```

- [ ] **Step 7: методы окна + arms** — методы impl App:

```rust
    /// Auto-follow: keep the active index inside [w, w+7); if it escapes past
    /// the right edge it becomes the FIRST visible slot (user-requested).
    fn follow_window(&mut self, idx: usize, is_perf: bool) {
        if is_perf {
            if !(self.perf_window..self.perf_window + 7).contains(&idx) {
                self.perf_window = idx.min(self.perf_list.len().saturating_sub(1));
            }
        } else if !(self.pat_window..self.pat_window + 7).contains(&idx) {
            self.pat_window = idx.min(7);
        }
    }

    /// Shift the active mode's window by `delta` slots, clamped to the list.
    fn page_window(&mut self, delta: i32) {
        match self.console.mode() {
            led_console::Mode::Home => {
                let n = self.perf_list.len();
                let max_w = n.saturating_sub(7);
                self.perf_window = (self.perf_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Pattern => {
                // 8 patterns, window shows 7: max start = 1
                self.pat_window = (self.pat_window as i32 + delta).clamp(0, 1) as usize;
            }
            _ => {}
        }
    }
```

Arms в `execute_console_action` — ВАЖНО: консоль возвращает SLOT (1..=7),
реальный индекс = окно + slot − 1; без перевода тапы грузят не те перфы:

```rust
            led_console::Action::LoadPerf(i) => {
                let idx = self.perf_window + i - 1; // slot -> real index
                if let Some((name, _)) = self.perf_list.get(idx) {
                    let name = name.clone();
                    if self.load_performance_by_name(&name) {
                        log::info!("[console] perf slot {} loaded ({})", i, name);
                        self.follow_window(idx, true);
                    }
                }
            }
            led_console::Action::SelectPattern(i) => {
                // консоль шлёт УЖЕ window-local 0-based (slot-1 из tap())
                let idx = self.pat_window + i;
                if idx < 8 {
                    self.drum_current_pattern = idx as u8;
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::DrumSeqPattern { pattern: idx as u8 });
                    self.follow_window(idx, false);
                }
            }
            led_console::Action::PageBack => {
                self.page_window(-7);
                // не оставлять пурпур за экраном (Important-фикс ревью)
                if let Some(i) = self.active_perf_console() { self.follow_window(i, true); }
                self.follow_window(self.drum_current_pattern as usize, false);
            }
            led_console::Action::PageForward => {
                self.page_window(7);
                if let Some(i) = self.active_perf_console() { self.follow_window(i, true); }
                self.follow_window(self.drum_current_pattern as usize, false);
            }
```

(старые arm'ы LoadPerf/SelectPattern из Task-2-плана консоли v1 ЗАМЕНИТЬ этими;
StepPerf/StepPattern arms работают в абсолютных индексах — оставаться как есть,
но добавить follow_window(next, true/false) как в Step 7.)

Undo arm остаётся без изменений (DrumSeqUndo).

- [ ] **Step 8: undo atom + окна в snapshot** — в LED-snapshot блоке перед
  вызовами top_colors/bottom_colors:

```rust
            let undo_avail = self.drum_undo_atom.load(Ordering::Relaxed);
            let page_depth = match self.console.mode() {
                led_console::Mode::Pattern => self.pat_window, // окна {0,1} — сама глубина
                _ => self.perf_window / 7,
            };
```

и вызовы:

```rust
            let top = self.console.top_colors(self.active_perf_console(), filled,
                                              self.drum_current_pattern as usize, &mutes,
                                              self.perf_window, self.pat_window);
            let bottom = self.console.bottom_colors(playing, recording, page_depth, undo_avail);
```

(`let filled` ИЗМЕНИТЬ: было `self.perf_list.len().min(8)` — теперь
`let filled = self.perf_list.len();` без cap, иначе страницы за 8-м перфом
рисуют чёрную стену.)

- [ ] **Step 9: прогнать + Commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
git add src/gui/mod.rs src/gui/led_console.rs src/gui/settings.rs src/lib.rs
git commit -m "feat: LED console pages, window auto-follow, undo LED"
```

### Task 3: Убрать мёртвый лупер-rewind с пэдов

**Files:**
- Modify: `src/synth/mod.rs` (удалить ControlEvent::LooperRewind + arm)
- Modify: `src/synth/looper.rs` (удалить rewind_to_start + 2 теста rewind)

Примечание: после Task 3 note 80 больше не пушит LooperRewind → вариант/arm/метод
мёртвые. Функция останется в git-истории (коммит f851dc1/6af24b7), вернуть легко.

- [ ] **Step 1:** удалить `LooperRewind` из enum + arm из handle_control;
  удалить `rewind_to_start` + тесты `rewind_to_start_resets_playback_cursor`,
  `rewind_to_start_mid_playback_drains_active_notes` + висячий doc-комментарий
  про rewind (looper.rs:504, внутри tick-Playing ветки комментарий
  «rewind_to_start mid-playback...» — заменить на нейтральный
  «close sounding notes once») из looper.rs.
- [ ] **Step 2: gates**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/synth/looper.rs src/synth/mod.rs
git commit -m "refactor: drop pad-routed looper rewind (28/29 repurposed to pages)"
```

### Task 4: Верификация (ручная, юзер)

- [ ] **Step 1:** gates — все зелёные.
- [ ] **Step 2: пользователь проверяет (ЭКСПЕРИМЕНТАЛЬНЫЕ ЦВЕТА):**
  1. PAGE▶ (29) при 8+ перфах → окно сдвинулось, 28/29 потемнели (оттенки синего); если оттенки не различимы/уродливы → откат на [0066FF;4] (fallback зашит в PAGE_SHADES).
  2. Next (31) с 7-го на 8-й перф → 8-й встал в первый слот (пэд 18) пурпуром и загрузился.
  3. Rec (27) на остановленном транспорте → бит сам стартовал, удары пишутся.
  4. Undo (32) — розовый когда есть снимок (после записи), тёмный после отката/на чистом.
  5. Розовый FF5080 — читается ли как розовый? Если нет — замена на FF00FF (validated magenta) по слову юзера.
- [ ] **Step 3:** обновить techdebt #15 (страницы реализованы) + commit docs.
