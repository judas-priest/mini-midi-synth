# TRACKS Mode on Pad 20 + Orange Colors (TECHDEBT #57 polish) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-06 (база = master e33021a (3908063 — docs-коммит поверх, содержимое идентично); факты вычитаны из кода; юзер-спека: «пад 20 = режим треков, режим отличается цветом, логика — как у трёх существующих режимов»; audit 2026-11-06 — menu slot 3 тёмный (:256-268), вход в TRACKS уже реализован (menu_slot3_tap_opens_tracks :480), ALL PERFS отсутствует)

**Goal:** TRACKS — полноценный четвёртый режим консоли по канону трёх существующих:
MENU-плитка на паде 20 = оранжевая; в режиме ряд пэдов = оранжевые папки, активная
папка = пурпур; пад 18 (первый оранжевый) = **(ALL PERFS)** — выход к общим перфам
с пэдов (закрывает юзер-вопрос «как выйти из треков без GUI»).

**Architecture:** Минимальные диффы в двух функциях led_console.rs + один handler
в mod.rs. Цвета — только hardware-validated hues (FF8000 оранжевый — validated,
в верхнем ряду свободен; пурпур-актив — канон всех режимов). Виртуальный ALL
PERFS = слот с индексом 0 TRACKS-списка (w==0); папки = индексы 1..; тап слота 0
→ active_folder = None. Шифт +1/-1 живёт ТОЛЬКО в mod.rs TrackSelect и led_console
paint — согласованно.

**Tech Stack:** Rust; TDD — led_console tests (цветовые маппинги чистые, канон
prev_next_step_semantics/там же).

**Проверенные факты (вычитано; не пересматривать):**
- MENU-плитки: top_colors `Mode::Menu` arm (led_console.rs:252-264): `0 =>
  AVAIL (синий HOME), 1 => [0xFF,0xFF,0x00] (жёлтый PATTERN), 2 => [0x00,0xFF,0x00]
  (зелёный MUTE), _ => чёрный` — слот 3 тёмный.
- Вход в TRACKS уже есть: tap() в Menu слот 3 → Mode::Tracks (тест
  menu_slot3_tap_opens_tracks :480).
- `folder_colors(&self, active_folder: Option<usize>, filled: usize, w: usize)`
  (led_console.rs:327-342): текущая версия — синие папки + пурпур-актив, БЕЗ
  ALL PERFS-слота; вызов mod.rs:982 — `folder_colors(self.config.ui.active_folder,
  self.config.ui.perf_folders.len(), self.tracks_window)` (БЕЗ shift — уже нужный
  вид для финальной семантики).
- `Action::TrackSelect(i)` handler (mod.rs:2732-2745): `idx = tracks_window + i`
  БЕЗ виртуального слота (полу-правка не применялась — state чистый).
- Пэды: слот 0 = пад 17 (MENU), слоты 1..=7 = пэды 18..24; Paint idx =
  `w + i - 1`.
- Оранжевый FF8000 в верхнем ряду свободен (используется только в bottom_colors
  Prev/Next :302-303).
- UX-канон: пурпур FF00FF = выбрано; жёлтый/синий/зелёный/оранжевый/красный/белый
  — validated; непроверенные hue запрещены (PADS_NOTES.md).

---

## Test Cases (from the requirement)

Требование: пад 20 = оранжевый режим треков; TRACKS отличает цветом (оранжевый
ряд); пад 18 в TRACKS = ALL PERFS; логика — зеркало трёх существующих режимов.

- TC1: MENU-плитка слот 3 = оранжевая `[0xFF, 0x80, 0x00]`.
- TC2: `folder_colors(active=None, filled=2, w=0)`: слот 1 (пад 18) = ALL PERFS
  `[0xFF, 0x80, 0x00]`; слот 2 = папка 0 `[0xFF, 0x80, 0x00]`; обе оранжевые.
- TC3: `folder_colors(active=Some(0), filled=2, w=0)`: слот 2 = пурпур
  `[0xFF, 0x00, 0xFF]` (активная папка), слот 1 = оранжевый ALL PERFS.
- TC4: paging: `folder_colors(active=Some(0), filled=2, w=1)`: слот 1 = папка 0
  пурпур (`idx-1 == 0`).
- TC5 (tap-семантика, экран + код-ревью): TRACKS, w=0: тап пэда 18 → ALL PERFS
  (active_folder=None, Home); тап пэда 19 → папка 0; 28/29 страницы листают
  w=0/1.
- TC7 (filled=0): `folder_colors(None, 0, 0)`: слот 1 = оранжевый ALL PERFS,
  остальные тёмные — выход к общим перфам доступен даже без папок.
- TC8 (paging ≥7 папок): `window_max_start(n+1)` — последняя папка достижима
  с пэдов (Task 2 a2 clamp).

## Negative Matrix

- folders.len() = 0 (папок нет): TRACKS — все слоты тёмные кроме ALL PERFS
  (слот 1 = ALL PERFS всегда доступен: выход к общим перфам даже без папок —
  осознанный плюс, тап → None, idempotent).
- w=1 и папок 2: слот 1 = папка 0 (idx-1 = 0) — обычная оранжевая, ALL PERFS
  только на первой странице (w=0) — осознанно.
- active_folder = None в TRACKS — нет пурпур-слота, все папки оранжевые (TC2).
- MENU в TRACKS-режиме (слот 0) — белый MENU-тап → Home с треком (не сброс);
  сброс только через повторный вход TRACKS → ALL PERFS (юзер-спека).
- непроверенные hue — не используем (только FF8000/FF00FF).
- folders.len() = 0: TC7 — ALL PERFS тап → None (idempotent, безопасно).
- ≥7 папок: page_window clamp n+1 (TC8) — последняя папка достижима.
- существующий тест menu_slot3_led_is_off — обновляется spec-driven (Step 2/3a2).
- дубликат TrackSelect-тапа в один кадр — консоль шлёт одно событие на тап.

---

### Task 1: Цветовые тесты + MENU-плитка + folder_colors (TDD)

**Files:**
- Modify: `src/gui/led_console.rs` — tests (tc_menu_orange_tile, tc_tracks_colors),
  top_colors Mode::Menu arm, folder_colors rewrite

- [ ] **Step 1: падающие тесты** — в tests-модуль led_console.rs:

```rust
    #[test]
    fn tc_menu_orange_tile() {
        // #57-полировка: слот 3 MENU = оранжевая плитка TRACKS
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let colors = c.top_colors(None, 0, 0, &[false; 8], 0, 0, 0);
        assert_eq!(colors[3], [0xFF, 0x80, 0x00], "menu TRACKS tile must be orange");
    }

    #[test]
    fn tc_tracks_colors_all_perfs_and_folders() {
        // Ряд TRACKS: слот 1 (w=0) = ALL PERFS; папки оранжевые; активная пурпур
        let mut c = LedConsole::new();
        c.mode = Mode::Tracks;
        let none = c.folder_colors(None, 2, 0);
        assert_eq!(none[1], [0xFF, 0x80, 0x00], "ALL PERFS slot must be orange");
        assert_eq!(none[2], [0xFF, 0x80, 0x00], "folder 0 must be orange");
        let act = c.folder_colors(Some(0), 2, 0);
        assert_eq!(act[1], [0xFF, 0x80, 0x00], "ALL PERFS slot");
        assert_eq!(act[2], [0xFF, 0x00, 0xFF], "active folder 0 must be magenta");
        // paging w=1: слот 1 = папка 0 (ALL PERFS только на первой странице)
        let page2 = c.folder_colors(Some(0), 2, 1);
        assert_eq!(page2[1], [0xFF, 0x00, 0xFF], "folder 0 magenta on page 2");
    }
```

- [ ] **Step 2: красный**

Run: `cargo test --lib tc_menu_orange tc_tracks_colors menu_slot3_led_is_off 2>&1 | tail -6`
Expected: tc_menu_orange + tc_tracks_colors FAIL; **существующий тест
`menu_slot3_led_is_off` (led_console.rs:512-523) тоже FAIL** — он пинит слот 3
чёрным; в Step 3 обновить его ожидание на `[0xFF, 0x80, 0x00]` (изменение
существующего теста осознанное и spec-driven: слот 3 теперь TRACKS-плитка).

- [ ] **Step 3: реализация.**

(a) top_colors Mode::Menu arm — `_ => [0x00, 0x00, 0x00],` заменить на:

```rust
                        3 => [0xFF, 0x80, 0x00], // TRACKS: orange
                        _ => [0x00, 0x00, 0x00],
```

(a2) Обновить тест `menu_slot3_led_is_off` (led_console.rs:512-523):
`menu[3]` ожидание `[0x00, 0x00, 0x00]` → `[0xFF, 0x80, 0x00]` + переименовать
в `menu_slot3_led_is_orange_tracks` (слот 3 теперь TRACKS-плитка — spec-driven).

(b) folder_colors — заменить тело (заодно обновить протухшие комментарии:
top_colors doc «menu slot 3 is dark» (:198-199) → «slot 3 = orange TRACKS»;
Mode::Menu arm comment «slot 3 is dark (target toggle removed)» (:257-259) →
удалить/заменить; doc folder_colors «0066FF синий» → оранжевый):

```rust
    pub fn folder_colors(
        &self, active_folder: Option<usize>, filled: usize, w: usize,
    ) -> [[u8; 3]; 8] {
        // Ряд TRACKS: оранжевые папки (FF8000, validated), активная — пурпур
        // (канон режимов). Слот 1 при w==0 = виртуальный "(ALL PERFS)" —
        // выход к общим перфам с пэдов (юзер-спека 2026-11-06).
        let mut out = [[0x00, 0x00, 0x00]; 8];
        out[0] = [0xFF, 0xFF, 0xFF]; // MENU
        for (i, col) in out.iter_mut().enumerate().skip(1) {
            let idx = w + i - 1;
            if idx == 0 && w == 0 {
                *col = [0xFF, 0x80, 0x00]; // (ALL PERFS)
                continue;
            }
            let folder = idx - 1; // -1: слот ALL PERFS
            // ВАЖНО: bound-first — stale active_folder ≥ filled (папка
            // удалена, конфиг не пересохранён) не должен красить пустой слот
            if folder >= filled {
                *col = [0x00, 0x00, 0x00];
            } else if Some(folder) == active_folder {
                *col = [0xFF, 0x00, 0xFF]; // active: magenta
            } else {
                *col = [0xFF, 0x80, 0x00];
            }
        }
        out
    }
```

- [ ] **Step 4: зелёный**

Run: `cargo test --lib tc_menu_orange tc_tracks_colors 2>&1 | tail -3`
Expected: 2 passed.

- [ ] **Step 5: коммит**

```bash
git add src/gui/led_console.rs && git commit -m "feat: TRACKS menu tile orange (pad 20) + orange folder row with ALL PERFS slot (#57 polish)"
```

### Task 2: TrackSelect — виртуальный ALL PERFS-тап

**Files:**
- Modify: `src/gui/mod.rs` — TrackSelect handler (~:2732-2745)

- [ ] **Step 1: обработчик** — заменить тело `Action::TrackSelect(i)`:

```rust
            led_console::Action::TrackSelect(i) => {
                // Слот-элемент 0 TRACKS-списка = виртуальный "(ALL PERFS)" —
                // выход к общим перфам с пэдов (юзер-спека 2026-11-06)
                if self.tracks_window + i == 0 {
                    self.config.ui.active_folder = None;
                    let _ = self.config.save();
                    self.refresh_console_list();
                    self.perf_window = 0;
                    self.console.force_mode(led_console::Mode::Home);
                    return;
                }
                let idx = self.tracks_window + i - 1; // -1: слот ALL PERFS
                if idx < self.config.ui.perf_folders.len() {
                    self.config.ui.active_folder = Some(idx);
                    let _ = self.config.save();
                    self.refresh_console_list();
                    self.perf_window = 0;
                    self.console.force_mode(led_console::Mode::Home);
                }
            }
```

(Остальные действия в TRACKS — MENU-тап слота 0 → Home с треком — уже
реализованы; полировка: перед force_mode(Home) при ALL PERFS логика
mode-entry sync Home-плечо сама снимет follow.)

(a2) **page_window Tracks-плечо — clamp +1** (список TRACKS = folders+1
виртуальный ALL PERFS; старый clamp по folders.len() делал последний папок
недостижимым при ≥7 папках):

```rust
            led_console::Mode::Tracks => {
                let max_w = led_console::window_max_start(
                    self.config.ui.perf_folders.len() + 1);
                self.tracks_window = (self.tracks_window as i32 + delta)
                    .clamp(0, max_w as i32) as usize;
            }
```

- [ ] **Step 2: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --lib led_console 2>&1 | tail -3 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: чисто, все зелёные.

- [ ] **Step 3: коммит**

```bash
git add src/gui/mod.rs && git commit -m "feat: TRACKS slot 0 = ALL PERFS — exit to the flat perf list from pads (#57)"
```

### Task 3: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): MENU → пад 20 оранжевый → TRACKS: ряд оранжевых папок
  + пад 18 = ALL PERFS; активная папка пурпур; тап 18 → все перфы; MENU →
  жёлтая плитка паттернов — трек сохраняется; обратный вход через пад 20
- [ ] TECHDEBT #57: дополнить запись (оранжевый TRACKS-режим, ALL PERFS пэд)

## Out of scope

- Красный/белый/новые hue для TRACKS — только validated палитра.
- Имена папок на пэдах (нельзя — LED без текста).
