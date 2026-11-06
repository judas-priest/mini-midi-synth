# UX Tails Batch: Tracks Keyboard + #34/#35 Closure Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-07 (база = master 05630ab; факты вычитаны из кода сегодня; research — tavily_search 2026-11-06 (egui_dnd/egui input канон) + tavily_research 2026-11-06 (UX tails))

**Goal:** Три хвоста одним батчем: (1) вход в TRACKS-режим открывает страницу
активной папки (сейчас tracks_window залипает где было); (2) Tracks-менеджер —
навигация стрелками + Enter (TECHDEBT #35 для менеджера); (3) закрыть #34
(браузер: позиция уже персистится — проверено, осталось пометить) и #35
браузер (стрелки/Enter уже реализованы :1563-1597 — осталось пометить).

**Architecture:** (1) mode-entry sync Tracks-плечо: tracks_window = страница
active_folder (active/7, clamp по max_w); toast-текст «MENU — назад» уже
есть. (2) App поле `tracks_sel: Option<usize>`; клавиатурный ввод ДО окна
(канон браузера :1563-1570): если show_tracks_manager И rename-TextEdit не в
фокусе — ↑/↓ двигают sel, Enter ставит active_folder = sel (save + refresh +
perf_window = 0). Рисование: selectable_label(tracks_sel == Some(fi)).
Guard фокуса — repaint-флаг `rename_focused: bool` локально в
draw_tracks_manager (TextEdit .has_focus()), прокинутый через App-поле
 НЕ нужно: ввод читается в update() ДО draw — фокус прошлого кадра;
канон: хранить `tracks_rename_focus: bool` App-поле, выставляемое в draw.

**Tech Stack:** Rust/egui; TDD — keyboard-семантика чистая (state-машина/поля
App не тестируемы юнитом без App — ввод/селекшн экран; serde/config не
трогаются). Пины через существующие тесты led_console (Mode::Tracks уже
есть).

**Проверенные факты (вычитано; не пересматривать):**
- Browser keyboard УЖЕ реализован (#35 браузер-часть): ввод до окна
  (gui/mod.rs:1563-1570: ArrowDown/Up двигают `inst_browser_index`,
  поиск-ресет :1572-1579, Enter :1581), Esc — modal shell, позиция окна
  персистится (#34: config.ui.inst_browser_pos, restore :1604, save :1897).
  ТЕХДЕБТ #34/#35 (браузер-часть) — ГОТОВО, осталось пометить.
- mode-entry sync (gui/mod.rs:806-826): match m { Pattern/Tracks(toast)/
  Mute/Home/_ } — refresh_console_list() безусловно в начале блока.
  tracks_window в Tracks-плече sync НЕ сбрасывается (хвост 1).
- draw_tracks_manager (gui/mod.rs:2924+): двухпанельный (ui.columns(2)),
  папки — `ui.selectable_label(active, &name)` в левой колонке; rename
  TextEdit resp (:3060) — `lost_focus && Enter` коммит.
- Toast текст Tracks-entry: «TRACKS: tap a folder — MENU — назад»
  (gui/mod.rs:815-816) — уже правильный (round-3 фикс), НЕ трогать.
- `PARTS_PER_ZONE` (gui/mod.rs:7, = 8); тест-модуль led_console: `use
  super::*`, канон prev_next_step_semantics.

---

## Test Cases (from the requirement)

Требование: (1) вход в TRACKS показывает страницу активной папки; (2) в
Tracks-менеджере ↑/↓ двигают выделение папок, Enter делает папку активной;
rename-TextEdit не конфликтует со стрелками; (3) #34/#35 браузер-часть —
пометить ГОТОВО (уже реализовано).

- TC1 (sync): active_folder = Some(9), folders 12 → вход TRACKS:
  tracks_window = 9/7 = 1 (страница 2, папка 9 видна: слот 1 = 7+1... idx =
  w+1 → папка (w·7)+i−1: w=1, i=2 → папка 8? трассировка: слот i (1..=7)
  показывает папку w + i − 1 → папка 9 видна при w = 3 (9 = 3+7−1)... w =
  min(active_folder, max_w = 12−7 = 5)... ФОРМУЛА ТЕСТА: w_after = страница,
  содержащая active_folder: `active_folder / 7` С clamp... активная 9:
  страницы 0 (0-6), 1 (7-13) → w = 1. Тест-пин: `page_of(9, max_w) == 1`.
- TC2 (manager nav): tracks_sel None → ↓ → Some(0); ↓ → Some(1); ↑ → Some(0).
- TC3 (manager Enter): tracks_sel = Some(2), folders ≥ 3 → Enter →
  active_folder = Some(2), config saved, console_list refreshed.
- TC4 (rename guard): rename-TextEdit в фокусе → ↑/↓/Enter НЕ двигают sel /
  НЕ меняют active_folder.
- TC5 (#34/#35 браузер — регрессия): существующие тесты браузера зелёные
  (канон #37/arrow-тесты уже в gui/mod.rs — не ломаются).

## Negative Matrix

- папок 0: TRACKS — tracks_window = 0 (clamp), пэды тёмные (канон #78 TC7);
  менеджер — пусто + «+ New» (канон empty state).
- tracks_sel = None при Enter — no-op (ничего не выбрано).
- sel за пределами после удаления папки — clamp при nav (sel.min(len−1)),
  Enter-guard `get(i)` уже есть.
- rename фокус: стрелки работают КАК текстовый курсор в TextEdit (egui
  консьюмит) — nav guard от этой коллизии и есть TC4.
- Browser (TC5): не трогаем — уже канон.
- двойной Enter подряд (после set active) — active_folder == sel → set
  идемпотентен, save перезапишет то же — безвредно.
- RT: всё GUI-поток.

---

### Task 1: Tracks-entry sync — страница активной папки

**Files:**
- Modify: `src/gui/mod.rs` — mode-entry sync Tracks-плечо (gui/mod.rs:806-826)

- [ ] **Step 1: Tracks-плечо sync** — заменить:

```rust
                led_console::Mode::Tracks => {
                    self.show_toast("TRACKS: tap a folder — MENU — назад",
                        ToastKind::Info);
                }
```

на

```rust
                led_console::Mode::Tracks => {
                    // Хвост #57: вход показывает страницу активной папки
                    let folders = self.config.ui.perf_folders.len();
                    if folders > 0 {
                        let page = self.config.ui.active_folder
                            .map(|a| a / 7)
                            .unwrap_or(0);
                        let max_w = led_console::window_max_start(folders);
                        self.tracks_window = page.min(max_w);
                    }
                    self.show_toast("TRACKS: tap a folder — MENU — назад",
                        ToastKind::Info);
                }
```

(константа 7 = слотов в окне; канон CONSOLE_VISIBLE/page_depth — сверить:
page_depth Pattern-плечо использует `/ 7`.)

- [ ] **Step 2: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: чисто, 32.

- [ ] **Step 3: коммит**

```bash
git add src/gui/mod.rs && git commit -m "fix: entering TRACKS opens the page of the active folder (#57 tail)"
```

### Task 2: Tracks-менеджер — стрелки + Enter (TECHDEBT #35)

**Files:**
- Modify: `src/gui/mod.rs` — App поля `tracks_sel: Option<usize>`,
  `tracks_rename_focus: bool` (рядом tracks_window :445), lib.rs литерал;
  draw_tracks_manager: selectable_label sel-state + nav-ввод в update()
  (канон браузера :1563-1570, ДО окна) с rename-guard; Enter → SetActive

- [ ] **Step 1: поля + init** — App (рядом tracks_window):

```rust
    pub tracks_sel: Option<usize>,
    pub tracks_rename_focus: bool,
```
lib.rs литерал: `tracks_sel: None, tracks_rename_focus: false,`.

- [ ] **Step 2: rename-focus флаг** — в draw_tracks_manager: локальная
  `let mut rename_focus = false;` — rename TextEdit resp ДО push в ops:
  `if resp.has_focus() { rename_focus = true; }`; после win.show:
  `self.tracks_rename_focus = rename_focus;` (прошлый кадр для nav-гейта в
  update() — draw идёт после update: флаг на кадр позже — канон
  консервативного guard'а, дребезг отсутствует: фокус живёт пока TextEdit
  активен).

- [ ] **Step 3: nav-ввод в update()** — РЯДОМ с browser-вводом (:1563),
  ДО окон:

```rust
        // #35: Tracks-менеджер — стрелки/Enter, если окно открыто и rename
        // НЕ в фокусе (канон браузера: ввод до окна)
        if self.show_tracks_manager && !self.tracks_rename_focus {
            let folders = self.config.ui.perf_folders.len();
            if folders > 0 {
                let moved = ctx.input(|i| {
                    let mut d: i32 = 0;
                    if i.key_pressed(egui::Key::ArrowDown) { d += 1; }
                    if i.key_pressed(egui::Key::ArrowUp) { d -= 1; }
                    d
                });
                if moved != 0 {
                    let cur = self.tracks_sel
                        .unwrap_or(self.config.ui.active_folder
                            .unwrap_or(0))
                        .min(folders - 1) as i32;
                    self.tracks_sel = Some((cur + moved).clamp(0, folders as i32 - 1) as usize);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if let Some(sel) = self.tracks_sel {
                        if sel < self.config.ui.perf_folders.len() {
                            self.config.ui.active_folder = Some(sel);
                            let _ = self.config.save();
                            self.refresh_console_list();
                            self.perf_window = 0;
                        }
                    }
                }
            }
        }
```

- [ ] **Step 4: paint sel-выделения** — в draw_tracks_manager левая панель,
  selectable_label: `tracks_sel == Some(fi)` вместо/вместе с `active`:

```rust
                        let sel_here = self.tracks_sel == Some(fi);
                        if ui.selectable_label(sel_here || active, &name)
                            .on_hover_text(if active { "Active — click to select for editing" } else { "Click/Enter to set active" }).clicked()
                        {
                            ops.push(Op::SetActive(fi));
                            self.tracks_sel = Some(fi);
                        }
```

+ после мутаций папок (delete) — clamp: `if let Some(s) = self.tracks_sel
{ if s >= folders.len() { self.tracks_sel = folders.checked_sub(1); } }`
(в apply-arms DelFolder/перестроений — по месту, компилятор/экран).

- [ ] **Step 5: гейт + коммит**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`
Expected: чисто, зелёные.

```bash
git add src/gui/mod.rs src/lib.rs && git commit -m "feat: Tracks manager keyboard nav — arrows move selection, Enter sets active, rename-focus guard (#35)"
```

### Task 3: Закрытие #34/#35-браузер + записи

**Files:**
- Modify: `docs/TECHDEBT.md` — #34: ГОТОВО (позиция персистится: restore
  gui/mod.rs:1604, save :1897 — verified); #35: браузер-часть ГОТОВО
  (стрелки/Enter/поиск-ресет :1563-1581 — verified), менеджер-часть — Task 2

- [ ] **Step 1: записи + gates + PR + rebuild + relaunch** (канон)

- [ ] **Step 2: Экран (юзер):** вход TRACKS показывает страницу активной
  папки; в менеджере ↑/↓ двигают выделение, Enter делает активной; rename
  в фокусе — стрелки правят текст; браузер — без регрессий

## Out of scope

- Настройки (Settings window) keyboard-nav — отдельная работа по фидбеку.
- Alias keyboard-nav в менеджере — папки сначала (алиасы — мышью/↑↓ кнопками).
- Arrow-nav в дереве параметров — вне скоупа #35.
