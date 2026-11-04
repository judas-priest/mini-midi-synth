# Console Paging (TECHDEBT #50) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-03 (база = master ef70cd0; сниппеты вычитаны из кода; аудит #50 от 2026-11-03 — findings 1-9; юзер-репорт: «стрелки 30/31 непонятно как работают, на другую страницу выходишь — нормально не возвращается»)

**Goal:** LED-консоль: пэды 30/31 листают СТРАНИЦУ активного режима (Pattern/Mute),
выбор паттерна больше не телепортирует окно в вырожденную страницу 7, ручное
листание не отменяется авто-follow, вход в режим открывает страницу с текущим
выбором.

**Architecture:** Чистые хелперы `window_max_start(len)` / `follow_start(idx,
start, len)` (led_console.rs, юнит-тесты) — единая арифметика окон для всех трёх
режимов (окно = 7 видимых слотов). В `step()` Pattern/Mute возвращают PageBack/
PageForward вместо StepPattern/StepPerf (юзер-модель: стрелки = страницы;
Action::StepPattern удаляется). PageBack/Forward больше НЕ следуют за выбором
паттерна (следуют только за перфом). Смена режима — синхронизация окна с
текущим выбором через новое поле `console_prev_mode`.

**Tech Stack:** Rust; pure-хелперы + юнит-тесты в существующем `#[cfg(test)] mod
tests` led_console.rs.

**Проверенные чтением кода факты (не пересматривать):**
- Пэд→событие: mod.rs:765-773 — 78=Prev, 79=Next, 80=PageBack, 76=PageForward.
- `step()` (led_console.rs:144-151): `Mode::Home => StepPerf(dir)`,
  `Mode::Pattern => StepPattern(dir)`, `_ => StepPerf(dir)` — поэтому в Mute
  30/31 молча ПЕРЕКЛЮЧАЮТ ПЕРФ (аудит finding 5).
- `Action::StepPattern` (mod.rs:2691-2696): `(current±1).rem_euclid(8)` +
  `follow_window(next,false)`; `follow_window` (mod.rs:2713-2721): паттерн-ветка
  `pat_window = idx.min(7)` — окно 7 = вырожденная страница (1 подсвеченный
  пэд), легальных стартов страницы только 0/1.
- `PageBack/Forward` (mod.rs:2697-2707): paging + perf-follow + ПАТТЕРН-follow —
  с выбранным паттерном 7 PageBack немедленно откатывается (finding 3);
  следуют в ЛЮБОМ режиме (finding 8).
- `page_window` (mod.rs:2724-2741): Pattern clamp 0..=1 — формула
  `len.saturating_sub(7)` одинакова для всех трёх режимов.
- `page_depth` (mod.rs:933-937): `Mode::Pattern => self.pat_window` без clamp —
  мусорная глубина на странице 7.
- Режимы меняются в `tap()` (led_console.rs:121-128) — App не получает
  уведомления; `console.mode()` (led_console.rs:270); тест-модуль
  led_console.rs:275+ существует.
- Пэды верхней строки: слоты 0..=7 = окно [w, w+7); SelectPattern по пэду
  (mod.rs:2655) выбирает видимый слот — follow там не нужен.
- Комментарий drums.rs:111 упоминает StepPattern — после удаления варианта
  обновить формулировку (в Files list не вынесено, реализатор поправит по
  месту).

---

## Test Cases (from the requirement)

Требование (юзер): стрелки листают страницы предсказуемо; уходишь со страницы —
возвращаешься; выбор не ломает страницу.

- TC1: `window_max_start(8)` = 1 (паттерны: 2 страницы), `window_max_start(16)`
  = 9 (мьюты), `window_max_start(30)` = 23 (перфы).
- TC2 (регрессия finding 2): `follow_start(7, 0, 8)` = `Some(1)` — выбор
  паттерна 7 ставит окно на страницу 1, НЕ на вырожденную 7.
- TC3: `follow_start(3, 0, 8)` = None — выбор в видимой зоне окно не двигает.
- TC4: `follow_start(0, 1, 8)` = Some(0) — уход выбора за ЛЕВЫЙ край
  возвращает окно.
- TC5: `follow_start(7, 1, 8)` = None — 7 виден со страницы 1.
- TC6 (регрессия finding 3, структурная): PageBack/Forward не содержат
  паттерн-follow — проверяется код-ревью диффа (строка удалена) + экран.

## Negative Matrix

- len < 7 (перфов меньше 7): `saturating_sub` → max_start 0 — одна страница.
- Стрелки в Menu-режиме: `page_window` плечо `_ => {}` — no-op; perf-follow
  в PageBack/Forward зажат `Mode::Home` — окно перфов не двигается из
  невидимых режимов.
- StepPattern удалён — Action::PageBack/Forward покрывают страницы; пошаговое
  перебирание паттернов исчезает с пэдов (остаётся в GUI-сетке и пэдами 18-24
  по слотам страницы) — осознанный trade по юзер-модели.
- `console_prev_mode` стартует None → первый кадр делает sync — идемпотентно
  (perf follow на существующий индекс).
- Follow при len=0 (пустой perf_list): `follow_start(idx,0,0)` — idx вне
  пустого диапазона → Some(idx.min(0)) = Some(0) → perf_window=0; безопасно.
- Двойной PageBack подряд — clamp держит в 0..=max.

---

### Task 1: Хелперы окон + тесты (TDD)

**Files:**
- Modify: `src/gui/led_console.rs` (pub(crate) const + 2 fn на уровне модуля;
  тесты в существующий `#[cfg(test)] mod tests`)

- [ ] **Step 1: падающие тесты** — в `mod tests` добавить:

```rust
    #[test]
    fn tc1_window_max_start() {
        assert_eq!(window_max_start(8), 1);   // паттерны: 2 страницы
        assert_eq!(window_max_start(16), 9);  // мьюты: 16 партов
        assert_eq!(window_max_start(30), 23); // перфы
        assert_eq!(window_max_start(3), 0);   // короче окна — одна страница
    }

    #[test]
    fn tc2_follow_clamps_to_last_page_start() {
        // Регрессия finding 2: выбор паттерна 7 ставил окно на вырожденную
        // страницу 7; обязан — на старт последней легальной страницы
        assert_eq!(follow_start(7, 0, 8), Some(1));
    }

    #[test]
    fn tc3_follow_inside_view_noop() {
        assert_eq!(follow_start(3, 0, 8), None);
        assert_eq!(follow_start(6, 0, 8), None);
    }

    #[test]
    fn tc4_follow_left_escape() {
        assert_eq!(follow_start(0, 1, 8), Some(0));
    }

    #[test]
    fn tc5_follow_right_edge_visible() {
        assert_eq!(follow_start(7, 1, 8), None);
    }
```

- [ ] **Step 2: красный**

Run: `cargo test --lib tc1_window 2>&1 | tail -4` (и tc2)
Expected: FAIL — функций нет.

- [ ] **Step 3: реализация** — на уровне модуля led_console.rs (вне impl):

```rust
/// Видимых слотов верхней строки (слоты 0..=6, 7-й — MENU).
pub(crate) const CONSOLE_VISIBLE: usize = 7;

/// Максимальный ЛЕГАЛЬНЫЙ старт окна для списка длиной len.
pub(crate) fn window_max_start(len: usize) -> usize {
    len.saturating_sub(CONSOLE_VISIBLE)
}

/// Auto-follow: если idx вне видимого окна [start, start+7) — вернуть новый
/// старт (зажатый легальными страницами), иначе None (не двигать).
/// Регрессия #50: паттерн 7 ставил окно в 7 — вырожденная страница.
pub(crate) fn follow_start(idx: usize, start: usize, len: usize) -> Option<usize> {
    if (start..start + CONSOLE_VISIBLE).contains(&idx) {
        None
    } else {
        Some(idx.min(window_max_start(len)))
    }
}
```

- [ ] **Step 4: зелёный**

Run: `cargo test --lib led_console 2>&1 | tail -4`
Expected: новые 5 тестов зелёные (плюс существующие); dead_code на хелперах
допустим до Task 2 (или `#[allow(dead_code)]` — снимать в Task 2).

- [ ] **Step 5: коммит**

```bash
git add src/gui/led_console.rs && git commit -m "feat: window_max_start/follow_start — console paging arithmetic (red-tested, #50)"
```

### Task 2: Стрелки = страницы; follow без телепортации

**Files:**
- Modify: `src/gui/led_console.rs:144-151` (step), enum `Action` (удалить
  `StepPattern`), тест :376 (см. Step 2b)
- Modify: `src/gui/mod.rs:2691-2707` (StepPattern handler + PageBack/Forward),
  `:2713-2721` (follow_window), `:2724-2741` (page_window), `:933-937`
  (page_depth), struct-поле (mod.rs:461, рядом с led_spawn_backoff)
- Modify: `src/lib.rs:873` — инициализация `console_prev_mode: None` в
  struct-литерале App (там же, где `led_spawn_backoff: None`)

- [ ] **Step 1: step()** — заменить тело:

```rust
    fn step(&mut self, dir: i32) -> Action {
        match self.mode {
            // Стрелки 30/31 в режиме перфов шагают перф (live-смена);
            // в остальных режимах = листание страницы (юзер-модель #50)
            Mode::Home => Action::StepPerf(dir),
            _ => {
                if dir < 0 {
                    Action::PageBack
                } else {
                    Action::PageForward
                }
            }
        }
    }
```

- [ ] **Step 2: удалить Action::StepPattern** — вариант из enum в
  led_console.rs и обработчик `Action::StepPattern { .. }` (mod.rs:2691-2696)
  целиком (пошаговое перебирание паттернов с пэдов исчезает: выбор — пэдами
  18-24 по слотам страницы).
  **Step 2b: существующий тест** led_console.rs:372-377
  (`prev_next_are_mode_dependent`) ждёт Pattern Next →
  `Some(Action::StepPattern(1))` — обновить на `Some(Action::PageForward)`
  (Home Prev → StepPerf(-1) остаётся валидным). Изменение существующего
  теста осознанное: семантика стрелок изменилась юзером. (TC6 — структурная
  проверка диффа, не юнит-тест.)

- [ ] **Step 3: PageBack/PageForward** — удалить паттерн-follow (регрессия
  finding 3: ручное листание больше не отменяется); perf-follow оставить:

```rust
            led_console::Action::PageBack => {
                self.page_window(-7);
                // perf-follow только в HOME: в Pattern/Mute стрелки листают
                // окно СВОЕГО режима и не должны молча двигать perf_window
                // (#50 finding 8); паттерн-follow удалён — ручное листание не
                // отменяется (finding 3)
                if self.console.mode() == led_console::Mode::Home {
                    if let Some(i) = self.active_perf_console() { self.follow_window(i, true); }
                }
            }
            led_console::Action::PageForward => {
                self.page_window(7);
                if self.console.mode() == led_console::Mode::Home {
                    if let Some(i) = self.active_perf_console() { self.follow_window(i, true); }
                }
            }
```

- [ ] **Step 4: follow_window через хелпер**:

```rust
    fn follow_window(&mut self, idx: usize, is_perf: bool) {
        if is_perf {
            if let Some(w) =
                led_console::follow_start(idx, self.perf_window, self.perf_list.len())
            {
                self.perf_window = w;
            }
        } else if let Some(w) = led_console::follow_start(idx, self.pat_window, 8) {
            self.pat_window = w;
        }
    }
```

- [ ] **Step 5: page_window через хелпер** — Pattern-плечо и общую формулу:

```rust
    fn page_window(&mut self, delta: i32) {
        match self.console.mode() {
            led_console::Mode::Home => {
                let max_w = led_console::window_max_start(self.perf_list.len());
                self.perf_window = (self.perf_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Pattern => {
                let max_w = led_console::window_max_start(8); // = 1
                self.pat_window = (self.pat_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            led_console::Mode::Mute => {
                let max_w = led_console::window_max_start(self.parts.len());
                self.mute_window = (self.mute_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
            _ => {}
        }
    }
```

- [ ] **Step 6: page_depth честный** (mod.rs:933-937): плечо Pattern →

```rust
                led_console::Mode::Pattern => self.pat_window.min(led_console::window_max_start(8)),
```

- [ ] **Step 7: sync при смене режима** (finding 4) — поле в struct App
  (mod.rs:461, рядом с `led_spawn_backoff`) и init (lib.rs:873):

```rust
    pub console_prev_mode: Option<led_console::Mode>,
```

```rust
            console_prev_mode: None,
```

Инициализация — в lib.rs:873 (struct-литерал App в init_common, рядом с
`led_spawn_backoff: None`): `console_prev_mode: None,`. Затем в
кадровом коде после `for a in acts { self.execute_console_action(a); }`
(mod.rs:783-785) вставить:

```rust
        // Смена режима консоли: окно режима открывается на текущем выборе
        // (finding 4: stale-страница при входе; идемпотентно, раз в смену)
        let m = self.console.mode();
        if self.console_prev_mode != Some(m) {
            self.console_prev_mode = Some(m);
            match m {
                led_console::Mode::Pattern => {
                    self.follow_window(self.drum_current_pattern as usize, false);
                }
                led_console::Mode::Mute => self.mute_window = 0,
                led_console::Mode::Home => {
                    if let Some(i) = self.active_perf_console() {
                        self.follow_window(i, true);
                    }
                }
                _ => {}
            }
        }
```

- [ ] **Step 8: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --lib 2>&1 | tail -3`
Expected: чисто, зелёные.

- [ ] **Step 9: коммит**

```bash
git add src/gui/led_console.rs src/gui/mod.rs src/lib.rs && git commit -m "fix: console arrows page the active mode, no window teleport (#50) — pattern follow dropped from manual paging, mode-entry window sync"
```

### Task 3: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер, с клавиатурой): MENU → PATTERN → 30/31 листают 2 страницы;
  выбрать последний патч страницы 2 → 29 (назад) → вернулось; стрелки в MUTE
  не переключают перф; повторный вход в PATTERN открывает страницу с текущим
  паттерном
- [ ] TECHDEBT #50: пометить ГОТОВО + зафиксировать новую разметку (30/31 =
  страницы в Pattern/Mute, StepPerf только в Home)

## Out of scope

- Chain-play follow (finding 9): при автосмене паттернов цепочкой окно
  консоли не следует — осознанно (юзер смотрит, что выбрал сам).
- LED-пагинация в GUI-сетке (finding 10, опционально) — сетка показывает все
  8 паттернов сразу.
- Пошаговое перебирание паттернов с пэдов 30/31 — удалено по юзер-модели;
  если понадобится — отдельный пэд-биндинг.
