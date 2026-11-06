# Tracks Hierarchy: Menu = Back, Folder Drill-Down (TECHDEBT #57 UX redesign) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-06 (база = master ae1cbe8; факты вычитаны из кода сегодня; юзер-спека ДОСЛОВНО: «MENU: синий=перфы, жёлтый=паттерны, зелёный=мьюты, оранжевый=треки. Оранжевый: меню треков, там треки. Пад 17 в верхнеуровневое меню ведёт [из HOME]. В меню треков — выбор треков. Нажимаешь на трек — ПОКАЗЫВАЮТСЯ ТОЛЬКО ПЕРФЫ ЭТОГО ТРЕКА. Меню нажимаешь — выходишь назад, в меню треков. Ещё раз — в верхнеуровневое.»)

**Goal:** Иерархическая навигация консоли: MENU = «назад на уровень». Уровни:
верхнее меню (синий/жёлтый/зелёный/оранжевый) → TRACKS (список папок) → папка
(пэды = ТОЛЬКО перфы этой папки, тап = загрузить) → MENU назад.

**Architecture:** Новый `Mode::FolderPerfs` — перфы активной папки (console_list
уже = папка при установленном active_folder; вход в папку ставит active_folder +
refresh). MENU-тап (слот 0): FolderPerfs → Tracks; Tracks → Menu (верхнее);
Home/Pattern/Mute → Menu (как сейчас). Пад 17 = «up» повсеместно. Убирается
ALL PERFS-виртуальный слот (выход к общим перфам = верхнее меню → синий HOME —
пэды показывают полный список). `Mode::Tracks` = только список папок (оранжевые,
активная папка пурпур-маркер последнего загруженного перфа не нужен).

**Tech Stack:** Rust/egui; TDD — tap()-навигация и цвета чисто тестируемы в
led_console tests; LoadPerf-глию — экран.

**Проверенные факты (вычитано сегодня; не пересматривать):**
- `tap()` (led_console.rs:149-175): Menu-блок первый (слоты 0→Home/1→Pattern/
  2→Mute/**3→Tracks**), затем Tracks-блок (слот 0 → `self.mode = Mode::Home`,
  слоты 1..=7 → `Action::TrackSelect(slot-1)`), затем generic slot 0 → Menu,
  слоты 1..=7 → `Mode::Home => LoadPerf(slot)` / Pattern / Mute.
- `Action::TrackSelect(i)` handler (gui/mod.rs:2732+): ТЕКУЩИЙ — sets
  active_folder = tracks_window + i, save, refresh, perf_window = 0 (stay).
  БЕЗ force_mode (удалён в ae1cbe8); смена режима из handler — поле приватное,
  есть только tap()-путь. ⇒ переход Tracks→FolderPerfs обязан происходить в
  led_console::tap (state-машина), НЕ в handler.
- `folder_colors(&self, active_folder: Option<usize>, filled: usize, w: usize)`
  (led_console.rs:327+): ТЕКУЩАЯ версия содержит ALL PERFS-жёлтый слот при
  w==0/idx==0 + сдвиг -1 (остаток полуприменённого #78) — ПРОТИВОРЕЧИТ
  press-what-you-see (paint слот 1 = ALL PERFS, тап слот 1 = папка 0).
- Paint сайта Tracks: mod.rs:982 — `folder_colors(active_folder,
  perf_folders.len(), tracks_window)` (без shift — согласовано с новой
  folder_colors ниже без изменений вызова).
- `Action::LoadPerf(slot)` handler: `idx = perf_window + slot - 1`,
  console_list.get(idx), tombstone → warn+toast (Task 3 плана #57, влит PR #77).
- `refresh_console_list()`: при Some(active_folder) console_list = алиасы папки
  (tombstones), None = все перфы. `active_perf_console()` матч по console_list.
- `Mode` derives PartialEq; `step()`: Home → StepPerf, Pattern → StepPattern,
  `_` → StepPerf (FolderPerfs унаследует StepPerf по console_list — folder perfs).
- Тест-модуль led_console.rs: `menu_slot3_tap_opens_tracks` (:480),
  `tracks_slot_tap_selects_folder` (:504, пинит СТАРУЮ семантику — обновляется),
  `menu_slot3_led_is_orange_tracks`, `tc_tracks_colors_*` (folder_colors,
  булевы/цветовые маппинги, канон `use super::*`).

---

## Test Cases (from the requirement)

Требование (юзер, дословно): «оранжевый: меню треков, там треки. Нажимаешь на
трек — показываются ТОЛЬКО перфы этого трека. Меню нажимаешь — выходишь назад,
в меню треков. Ещё раз — в верхнеуровневое.»

- TC1 (menu → tracks): Menu-режим, тап слота 3 → Mode::Tracks. (уже пинится
  menu_slot3_tap_opens_tracks — сохраняется).
- TC2 (folders paint): Tracks, folders = [T1, T2]: слоты 1,2 = оранжевые
  FF8000; активная папка (active_folder = Some(0)) — слот 1 пурпур FF00FF;
  пустые слоты тёмные. НЕТ жёлтого ALL PERFS-слота.
- TC3 (drill-down): Tracks, тап слота 1 → Mode::FolderPerfs.
- TC4 (perfs paint): FolderPerfs, active_folder = Some(0) (алиасы [B, A]):
  perf_window = 0 → слот 1 = B синим (доступен), слот 2 = A синим; если
  perf_name == "B" → слот 1 пурпур (загружен).
- TC5 (perf tap loads): FolderPerfs тап слота 1 → Action::LoadPerf(1)
  (window-local; handler грузит console_list[perf_window + slot - 1]).
- TC6 (MENU = up): FolderPerfs, слот 0 → Mode::Tracks; Tracks, слот 0 →
  Mode::Menu (верхнее).
- TC7 (StepPerf в папке): step() в FolderPerfs → StepPerf (шаг по алиасам
  папки через console_list).
- TC8 (folder switch resets perf window): вход в папку сбрасывает
  perf_window = 0 (перф-страница с начала).

## Negative Matrix

- Папка пуста (алиасов 0): FolderPerfs — слоты тёмные, тапы no-op (LoadPerf
  get → None → warn+toast, канон #77); MENU назад работает.
- Tombstone-алиас в папке — слот тёмный, тап → warn+toast (канон #77 Task 3).
- tracks_window/folder window при удалении папки — tracks_window clamp при
  page_window; папок меньше окна — слоты тёмные.
- (Init) — алиасом быть МОЖЕТ (load_performance_by_name спец-кейс есть).
- active_folder = None + Mode::FolderPerfs — невозможно: вход в FolderPerfs
  только тапом папки в Tracks, который ставит active_folder. Несогласованное
  состояние из битого конфига — paint по console_list (= полный список при
  None), не паникуем.
- дубликат имён папок — режим индексный, безразлично.
- RT: всё GUI-поток; новые поля — usize/bool.

---

### Task 1: Mode::FolderPerfs + иерархический tap() + folder_colors без ALL PERFS (TDD)

**Files:**
- Modify: `src/gui/led_console.rs` — Mode enum (+FolderPerfs), tap() (Tracks/
  FolderPerfs блоки), folder_colors (убрать ALL PERFS-слот; папки оранжевые,
  активная пурпур), tests

- [ ] **Step 1: падающие тесты** — в tests-модуль led_console.rs:

```rust
    #[test]
    fn tc_tracks_tap_drills_into_folder() {
        // #57 UX: тап папки в TRACKS = вход в перфы папки (не stay)
        let mut c = LedConsole::new();
        c.mode = Mode::Tracks;
        assert_eq!(c.on_event(PadEvent::Down(1), Instant::now()), None);
        assert_eq!(c.on_event(PadEvent::Up(1), Instant::now() + Duration::from_millis(50)),
                   Some(Action::TrackSelect(0)));
        // TrackSelect handler — GUI-сторона; state-машина: тап папки в Tracks
        // эмитит TrackSelect, а Mode::FolderPerfs ставится в mod.rs через
        // force_mode. Здесь пиним только Action.
    }

    #[test]
    fn tc_folderperfs_menu_goes_back_to_tracks() {
        // MENU = назад на уровень: FolderPerfs → Tracks
        let mut c = LedConsole::new();
        c.mode = Mode::FolderPerfs;
        assert_eq!(c.on_event(PadEvent::Down(0), Instant::now()), None);
        assert_eq!(c.on_event(PadEvent::Up(0), Instant::now() + Duration::from_millis(50)),
                   None); // слот 0 = смена режима внутрь state-машины (без Action)
        assert_eq!(c.mode(), Mode::Tracks);
    }

    #[test]
    fn tc_tracks_menu_goes_to_top_menu() {
        // MENU = назад на уровень: Tracks → верхнее меню
        let mut c = LedConsole::new();
        c.mode = Mode::Tracks;
        assert_eq!(c.on_event(PadEvent::Down(0), Instant::now()), None);
        assert_eq!(c.on_event(PadEvent::Up(0), Instant::now() + Duration::from_millis(50)),
                   None);
        assert_eq!(c.mode(), Mode::Menu);
    }
```

(слот 0 — смена режима внутри state-машины, Action нет — канон Menu-блока
:151-156; ассерт на c.mode() — getter :301.)

- [ ] **Step 2: красный** — `cargo test --lib tc_tracks_tap tc_folderperfs tc_tracks_menu 2>&1 | tail -6`
  Expected: tc_tracks_tap PASS (старая семантика эмитит TrackSelect), tc_folderperfs/
  tc_tracks_menu FAIL (Mode::FolderPerfs нет; Tracks слот 0 → Home).

- [ ] **Step 3: реализация.**

(a) Mode enum: добавить `FolderPerfs` после `Tracks`.

(b) tap() — Tracks-блок заменить:

```rust
        if self.mode == Mode::Tracks {
            // слот 0 = MENU (белый) — НАЗАД на уровень: верхнее меню
            if slot == 0 { self.mode = Mode::Menu; return None; }
            // слоты 1..=7 = папки: тап = вход в перфы папки (drill-down);
            // TrackSelect handler (gui/mod.rs) ставит active_folder
            return Some(Action::TrackSelect(slot - 1));
        }
        if self.mode == Mode::FolderPerfs {
            // слот 0 = MENU (белый) — НАЗАД: меню треков
            if slot == 0 { self.mode = Mode::Tracks; return None; }
            // слоты 1..=7 = перфы папки (window-local): тап = загрузить
            return Some(Action::LoadPerf(slot));
        }
```

(c) top_colors Mode::Menu arm — плитка 3 = оранжевая TRACKS:

```rust
                        3 => [0xFF, 0x80, 0x00], // TRACKS: orange
```

(d) folder_colors — убрать ALL PERFS-слот (папки 1..=7 = `w + i - 1`,
оранжевые, активная папка пурпур):

```rust
    /// Режим Tracks: список папок. Оранжевые FF8000 (validated), активная
    /// папка — пурпур-маркер последнего входа. ALL PERFS-слота нет: выход
    /// к общим перфам = MENU → синяя плитка HOME (иерархия #57 UX).
    pub fn folder_colors(
        &self, active_folder: Option<usize>, filled: usize, w: usize,
    ) -> [[u8; 3]; 8] {
        let mut out = [[0x00, 0x00, 0x00]; 8];
        out[0] = [0xFF, 0xFF, 0xFF]; // MENU
        for (i, col) in out.iter_mut().enumerate().skip(1) {
            let idx = w + i - 1;
            if idx >= filled { break; }
            *col = if Some(idx) == active_folder {
                [0xFF, 0x00, 0xFF] // последняя посещённая папка
            } else {
                [0xFF, 0x80, 0x00] // папка: orange
            };
        }
        out
    }
```

( `folder >= filled` break сохранён — старый bound-first эквивалентен break.)

- [ ] **Step 4: зелёный по led_console** — обновить СТАРЫЕ тесты
  spec-driven: `tracks_slot_tap_selects_folder` — теперь пинит drill-down
  (Action::TrackSelect(0) остаётся; комментарий обновить: «tap = вход в
  папку»); `tc_tracks_colors_*` из #78-плана (жёлтый ALL PERFS + shift-маппинг
  active+1/len+1) — УДАЛИТЬ/перезаписать под новую folder_colors (оранжевые +
  пурпур по active_folder без shift).

Run: `cargo test --lib led_console 2>&1 | tail -4`
Expected: все led_console зелёные.

- [ ] **Step 5: коммит**

```bash
git add src/gui/led_console.rs && git commit -m "feat: hierarchical console — Mode::FolderPerfs drill-down, MENU = back one level, folder_colors folders-only (#57 UX)"
```

### Task 2: mod.rs — TrackSelect = drill-down + FolderPerfs paint/load/paging

**Files:**
- Modify: `src/gui/mod.rs` — TrackSelect handler, paint match (+FolderPerfs),
  page_window (+FolderPerfs), page_depth (+FolderPerfs), mode-entry sync
  (FolderPerfs → perf_window = 0), step() комментарий

- [ ] **Step 1: TrackSelect = drill-down** — handler заменить:

```rust
            led_console::Action::TrackSelect(i) => {
                // Тап папки в TRACKS: папка становится активной, консоль
                // входит в её перфы (Mode::FolderPerfs ставит led_console::tap)
                let idx = self.tracks_window + i;
                if idx < self.config.ui.perf_folders.len() {
                    self.config.ui.active_folder = Some(idx);
                    let _ = self.config.save();
                    self.refresh_console_list();
                    self.perf_window = 0; // TC8: перф-страница с начала
                }
            }
```

(без смены режима — mode уже поставил led_console::tap; console_list после
refresh = алиасы этой папки.)

- [ ] **Step 2: paint + LoadPerf + paging** — в paint match добавить плечо
  (перед `_`):

```rust
                led_console::Mode::FolderPerfs => self.console.top_colors(
                    self.active_perf_console(), self.console_list.len(),
                    self.drum_current_pattern as usize, &mutes,
                    self.perf_window, self.pat_window, self.mute_window),
```

(Home-плечо не трогать; FolderPerfs = Home-стиль поверх console_list.)
LoadPerf handler уже читает console_list + perf_window — работает как есть.
page_window — добавить FolderPerfs-плечо:

```rust
            led_console::Mode::FolderPerfs => {
                let max_w = led_console::window_max_start(self.console_list.len());
                self.perf_window = (self.perf_window as i32 + delta).clamp(0, max_w as i32) as usize;
            }
```

page_depth — FolderPerfs arm `self.perf_window / 7`. step() — FolderPerfs
попадает в `_` arm (StepPerf по console_list = шаг по папке) — ОСОЗНАННО.

- [ ] **Step 3: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: чисто, все зелёные.

- [ ] **Step 4: коммит**

```bash
git add src/gui/mod.rs && git commit -m "feat: FolderPerfs — pads load folder perfs, MENU back to tracks list (#57 hierarchy)"
```

### Task 3: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): MENU → оранжевый → список папок → тап папки → ЕЁ перфы
  синими/пурпур (загружен) → тап перфа = загрузить → MENU = назад к папкам →
  MENU = верхнее меню → синяя плитка = все перфы
- [ ] TECHDEBT #57: дописать иерархию (MENU = up, drill-down), отметить
  отказ от ALL PERFS-слота

## Out of scope

- ALL PERFS-пэд в TRACKS — убран иерархией: общие перфы = верхнее меню →
  синяя плитка.
- Drag-reorder папок/алиасов, имена папок на LED — ранее зафиксировано.
- GUI Layers perf-строка — отображает активную папку (Track-индикатор топ-бара
  уже синхронен); отдельный пересмотр — по фидбеку.
