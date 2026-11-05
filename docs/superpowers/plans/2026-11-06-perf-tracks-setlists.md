# Perf Tracks / Setlists (TECHDEBT #57) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-06 (база = master f3ec4d4; факты вычитаны из кода; research — tavily_research pro 2026-11-05/06: Helix Stadium setlists = «preset aliases», edit-once-update-everywhere; Kemper Performance = «все звуки песни в одном месте», слоты на песню; Fractal Setlists/Songs; UX-канон: dedicated Setlist Mode (паттерн №1), tombstone + warn-on-press для битых ссылок (индексы пэдов не плывут), rename-пропагация сильная, hold/подтверждение для деструктива)

**Goal:** Папки-треки (Setlists) для перфов: именованная папка = трек (песня), внутри
упорядоченный список АЛИАСОВ существующих перфов (по имени). LED-консоль в HOME
листает перфы ТЕКУЩЕГО трека; выбор трека — новый режим Tracks на консоли
(MENU-слот 3) + GUI-менеджер. Без папок — плоский список как сейчас (ноль регрессий).

**Architecture:** `PerfFolder { name, perfs: Vec<String> }` в config (алиасы по
имени перфа). Чистый резолвер `resolve_folder_entries` → `Vec<(name, Option<path>)>`
— БИТЫЕ ссылки НЕ выбрасываются: tombstone-слот (тёмный пэд + warn-on-press),
индексы пэдов стабильны (канон research). Кэш `console_list` в App пересчитывается
при изменениях (perf_list/папки/активная папка), все консольные сайты читают кэш.
Tracks-режим: `Mode::Tracks`, MENU-слот 3 (был тёмный), пэды 18-24 = папки, тап →
активная папка + возврат в HOME. GUI: ModalWindow «Tracks» (создать/переименовать/
удалить папку, добавить текущий/выбранный перф, убрать, ↑↓ порядок) + ComboBox
трека в Layers (рядом с perf ComboBox).

**Tech Stack:** Rust/egui; serde (канон #[serde(default)] config.rs:150+);
TDD — pure-функции (resolve/сериализация) тестируемы, консоль-пламбинг — экран.

**Проверенные факты (вычитано; не пересматривать):**
- `Mode { Home, Pattern, Mute, Menu }` (led_console.rs:14-20); `tap(slot)` (:147-168)
  :121-128: в Menu слот 0→Home, 1→Pattern, 2→Mute, **3 = тёмный no-op** (коммент
  «ex-toggle»); MENU-тап повторно в Menu-режиме → Home (:122-124: `if self.mode ==
  Mode::Menu { ... }` обрабатывает слоты). `Mode` — `#[derive(Default)]`-стиль,
  Home = default.
- `top_colors(&self, active_perf: Option<usize>, filled_perfs: usize,
  current_pattern: usize, mutes: &[bool], w_perf, w_pat, w_mute) -> [[u8;3];8]`
  (led_console.rs:189-200). Палитра — ТОЛЬКО hardware-validated hues
  (FF0000/00FF00/FFFF00/0066FF/FF8000/FF00FF/FFFFFF, PADS_NOTES.md).
- Perf-литерал консоли: paint `idx = w_perf + i - 1` (led_console.rs:202);
  LoadPerf handler: `idx = perf_window + i - 1` (mod.rs:2684); identical.
- `perf_list` строится в lib.rs:898-902 (list_performances + (Init) вставлен
  первым); `App.perf_list: Vec<(String, PathBuf)>` (:499).
- `active_perf_console()` (mod.rs:2789): `perf_list.iter().position(name ==
  &self.perf_name)`.
- LoadPerf: `load_performance_by_name` (mod.rs:2686) возвращает false при
  промахе — молча (аудит #50 finding 5; fix: warn+toast в этом плане).
- Консольные сайты, читающие len списка: page_window Home-плечо
  (window_max_start(perf_list.len()) :2749-2751), follow_window (~:2755),
  paint `filled` (:955), page_depth (:958-962), mode-entry sync (Home → follow).
- Кэш-поле в App — канон: `perf_list` (:499), `console_prev_mode` (:463).
- config UiSettings (config.rs:150+): `#[serde(default)]`-паттерн на каждом
  поле; `pad_perf_map: Vec<Option<String>>` (:163) — прецедент Vec<Option<String>>.
- ModalWindow (gui/modal.rs): `new(ctx, id_salt, title, win_open, default_width)`
  + `.fixed_width(w)` (.default_height/.default_pos) — канон браузера.
- Toast: `self.show_toast(msg, ToastKind::...)` (mod.rs:264, layers.rs:276).
- ПАМЯТЬ/канон: пэды 17-24 = слоты 0-7, слот 0 = MENU (белый); purple = выбрано.

---

## Test Cases (from the requirement)

Требование: папка-трек содержит алиасы перфов; консоль листает папку; битые
ссылки не ломают раскладку пэдов; без папок — как раньше.

- TC1 (resolve алиасов): perf_list = [A,B,C]; папка [«B»,«Ghost»,«A»] →
  `resolve_folder_entries` → [(B,Some),(Ghost,None),(A,Some)] — tombstone
  СОХРАНЯЕТСЯ (индексы стабильны), None = путь отсутствует.
- TC2 (плоский fallback): папки пусты / active_folder = None → консольный
  список = полный perf_list, включая (Init).
- TC3 (LoadPerf tombstone): пэд tombstone-слота → warn в логе + toast,
  активный перф НЕ меняется.
- TC4 (serde): ConfigUi с perf_folders [2 папки] + active_folder Some(1) →
  JSON-кругтрип; legacy JSON без ключей → пусто/None.
- TC5 (Tracks-режим): MENU → слот 3 → Mode::Tracks; пэды = папки (активная
  пурпур); тап папки → active_folder + возврат в HOME; HOME теперь листает
  папку.
- TC6 (не-регрессия): без папок все существующие тесты/поведение прежние.

## Negative Matrix

- Папок нет вообще (до первого создания) — resolve → полный список (TC2).
- Папка пуста — консоль: `filled = 0`, пэды 18-24 тёмные; нажатие → warn +
  toast (Task 3 catch-all), не молча.
- Дубликат имени перфа в папке — разрешён (два пэда на один перф; безвредно).
- Дубликат имени ПАПКИ — разрешён на уровне данных; GUI-менеджер не запрещает
  (различаются по индексу active_folder), rename по индексу.
- Удаление папки, когда active_folder = её индекс → active_folder = None
  (плоский режим), не паниковать.
- perf удалён вне приложения (файл снесён) → tombstone (TC1/TC3), не автопочистка
  (канон research: warn-on-press, resolve в менеджере).
- rename перфа в приложении не реализован (только Save-as) — пропагация не нужна
  пока; при добавлении rename — канон research (обновить все ссылки).
- active_folder ≥ folders.len() из битого конфига — guard при чтении кэша
  (treat as None).
- RT: всё в GUI-потоке; консольные события через существующее кольцо.
- LED-цвета: только hardware-validated палитра (PADS_NOTES.md) — tombstone =
  тёмный (00/чёрный), не новый hue.

---

### Task 1: PerfFolder в конфиге + serde-тесты (TDD)

**Files:**
- Modify: `src/config.rs` — struct PerfFolder (рядом с PartUiState :90-104),
  поля UiSettings: `perf_folders: Vec<PerfFolder>`, `active_folder: Option<usize>`
  (рядом с pad_perf_map :163, канон #[serde(default)])
- Test: `tests/perf_load_seq.rs` или новый `tests/config_folders.rs` — serde
  roundtrip (канон config_led.rs/config-тестов; serde_json в deps)

- [ ] **Step 1: падающие тесты** — tests/config_folders.rs:

```rust
//! #57: PerfFolder (setlists/tracks) — serde contract.
use mini_midi_synth::config::{self, PerfFolder};

#[test]
fn tc4_folders_roundtrip() {
    let mut ui = config::UiSettings::default();
    ui.perf_folders = vec![
        PerfFolder { name: "Track 1".into(), perfs: vec!["Bass".into(), "Piano".into()] },
        PerfFolder { name: "Track 2".into(), perfs: vec!["Piano".into(), "Synth".into()] },
    ];
    ui.active_folder = Some(1);
    let json = serde_json::to_string(&ui).unwrap();
    let back: config::UiSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.perf_folders.len(), 2);
    assert_eq!(back.perf_folders[0].name, "Track 1");
    assert_eq!(back.perf_folders[1].perfs, vec!["Piano".to_string(), "Synth".to_string()]);
    assert_eq!(back.active_folder, Some(1));
}

#[test]
fn tc4_legacy_config_without_folders_gets_defaults() {
    // минимальный legacy-JSON UiSettings: нет perf_folders/active_folder
    let legacy = r#"{}"#;
    let ui: config::UiSettings = serde_json::from_str(legacy).unwrap();
    assert!(ui.perf_folders.is_empty());
    assert_eq!(ui.active_folder, None);
}
```

UiSettings::default() НЕ существует (verified: config.rs:124, только
Config::default собирает литерал :235-263) — оба теста десериализуют из
`r#"{}"#` (serde-default покроет новые поля).

- [ ] **Step 2: красный**

Run: `cargo test --test config_folders 2>&1 | tail -6`
Expected: FAIL — типа PerfFolder нет.

- [ ] **Step 3: реализация** — в src/config.rs (рядом с PartUiState):

```rust
/// Папка-трек (#57): упорядоченный список АЛИАСОВ перфов (по имени).
/// Правка перфа-оригинала живёт во всех папках; отсутствующий перф =
/// tombstone-слот (тёмный пэд, warn-on-press).
#[derive(Clone, Serialize, Deserialize)]
pub struct PerfFolder {
    pub name: String,
    #[serde(default)]
    pub perfs: Vec<String>,
}
```

UiSettings поля (рядом с pad_perf_map :163):

```rust
    /// Папки-треки (setlists): алиасы перфов по имени (#57).
    #[serde(default)]
    pub perf_folders: Vec<PerfFolder>,
    /// Активная папка консоли (None = плоский список всех перфов).
    #[serde(default)]
    pub active_folder: Option<usize>,
```

Инициализация в литерале UiSettings внутри `Config::default` (config.rs:235-263,
рядом с pad_perf_map): `perf_folders: Vec::new(), active_folder: None,`.

- [ ] **Step 4: зелёный**

Run: `cargo test --test config_folders 2>&1 | tail -3`
Expected: 2 passed. (Если UiSettings имеет не-serde поля — тест через `{}`
десериализацию, см. Step 1 примечание.)

- [ ] **Step 5: коммит**

```bash
git add src/config.rs tests/config_folders.rs && git commit -m "feat: PerfFolder config — setlists of perf aliases, serde-default (#57, red-tested)"
```

### Task 2: resolve_folder_entries — чистый резолвер + App-кэш

**Files:**
- Modify: `src/config.rs` — `pub fn resolve_folder_entries` РЯДОМ с PerfFolder
  (НЕ в gui: mod gui приватный + feature-gated, tests/ его не видят; config
  компилируется во всех билдах)
- Modify: `src/gui/mod.rs` — App поле
  `console_list: Vec<(String, Option<std::path::PathBuf>)>` (:499 рядом с
  perf_list) + дефолт в lib.rs App-литерале + `fn refresh_console_list(&mut self)`
  + вызовы refresh: после построения perf_list (lib.rs:902 — нельзя, lib строит
  через CommonInit; refresh зовётся первым кадром и при изменениях — см. ниже),
  после perf Save (layers.rs:244-245), после изменений папок (Task 5), при
  смене active_folder, и в mode-entry sync (Home-плечо).

- [ ] **Step 1: падающие тесты** — src/synth/tests.rs НЕ место (GUI); добавить
  в tests/config_folders.rs:

```rust
#[test]
fn tc1_resolve_keeps_tombstones() {
    use mini_midi_synth::config::resolve_folder_entries;
    let pl = vec![
        ("A".to_string(), std::path::PathBuf::from("/a.json")),
        ("B".to_string(), std::path::PathBuf::from("/b.json")),
        ("C".to_string(), std::path::PathBuf::from("/c.json")),
    ];
    let folder = PerfFolder { name: "T".into(),
        perfs: vec!["B".into(), "Ghost".into(), "A".into()] };
    let r = resolve_folder_entries(&pl, Some(&folder));
    assert_eq!(r.len(), 3); // tombstone НЕ выбрасывается — индексы стабильны
    assert_eq!(r[0], ("B".to_string(), Some(std::path::PathBuf::from("/b.json"))));
    assert_eq!(r[1], ("Ghost".to_string(), None));
    assert_eq!(r[2], ("A".to_string(), Some(std::path::PathBuf::from("/a.json"))));
}

#[test]
fn tc2_no_folder_resolves_to_full_list() {
    use mini_midi_synth::config::resolve_folder_entries;
    let pl = vec![("(Init)".to_string(), std::path::PathBuf::from("/i.json"))];
    let r = resolve_folder_entries(&pl, None);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0, "(Init)");
}
```

(видимость: tests/ интеграционный — resolver живёт в config.rs, `pub mod
config` есть (lib.rs:6); `mod gui` приватный и feature-gated — в gui НЕ класть.)

- [ ] **Step 2: красный** — `cargo test --test config_folders 2>&1 | tail -4`

- [ ] **Step 3: реализация** — src/config.rs (рядом с PerfFolder):

```rust
/// Резолв папки (#57): алиасы → (name, Option<path>). Отсутствующий перф
/// ОСТАЁТСЯ tombstone-слотом (None) — раскладка пэдов стабильна, пэд тёмный,
/// нажатие = warn (research-канон setlists: warn-on-press). None = плоский
/// список (fallback до создания папок).
pub fn resolve_folder_entries(
    perf_list: &[(String, std::path::PathBuf)],
    folder: Option<&PerfFolder>,
) -> Vec<(String, Option<std::path::PathBuf>)> {
    match folder {
        Some(f) => f.perfs.iter()
            .map(|name| {
                let path = perf_list.iter().find(|(n, _)| n == name).map(|(_, p)| p.clone());
                (name.clone(), path)
            })
            .collect(),
        None => perf_list.iter()
            .map(|(n, p)| (n.clone(), Some(p.clone())))
            .collect(),
    }
}
```

App: поле `pub console_list: Vec<(String, Option<std::path::PathBuf>)>` (рядом
perf_list :499), дефолт `Vec::new()` в lib.rs литерале; метод:

```rust
    /// Пересчёт консольного списка перфов (#57): папка или плоский список.
    /// Также логирует tombstones (warn) — один раз на refresh.
    fn refresh_console_list(&mut self) {
        let folder = self.config.ui.active_folder
            .and_then(|i| self.config.ui.perf_folders.get(i))
            .map(|f| (f.name.clone(), f.clone()));
        let (fname, folder) = match folder {
            Some((n, f)) => (Some(n), Some(f)),
            None => (None, None),
        };
        self.console_list = config::resolve_folder_entries(&self.perf_list, folder.as_ref());
        for (name, path) in &self.console_list {
            if path.is_none() {
                if let Some(n) = &fname {
                    log::warn!("[console] папка '{n}': перф '{name}' не найден (tombstone)");
                }
            }
        }
    }
```

Вызовы refresh_console_list: (a) СРАЗУ В TASK 2 — иначе console_list пуст
вечно до Task 4 (пэды тёмные, LoadPerf no-op): добавить в конец
send_initial_presets ИЛИ в первый кадр sync-блока (который появится в Task 4 —
промежуточно: refresh в конце load_edited_params НЕ место; ЛУЧШЕ: в Task 2
добавить refresh в gui/mod.rs update() первом проходе — guard
`if self.console_list.is_empty() { self.refresh_console_list(); }` в начале
update(), удаляется в Task 4 на sync-блок); (b) layers.rs после
`self.perf_list = ...` (:245); (c) все изменения папок/active_folder (Task 5);
(d) после LoadPerf не нужно (perf_list не менялся).

- [ ] **Step 4: зелёный + консольные сайты на console_list** — заменить во
  ВСЕХ консольных местах `self.perf_list` → `self.console_list` (path Option —
  tombstone-чек):
  - LoadPerf handler (~:2683-2692): `console_list.get(idx)` — tombstone см. Task 3;
  - **StepPerf handler (~:2718-2730): len + `.get(next)` на console_list;
    tombstone при next → не молчать: лог warn + ПРОПУСТИТЬ шаг (итерировать dir
    дальше до живого перфа, максимум n попыток — канон TC3 warn-on-press),
    затем follow_window(next, true) одним вызовом после цикла;**
  - follow_window perf-плечо len (~:2755);
  - page_window Home-плечо len (~:2770);
  - paint `filled` (:955) + painting count;
  - active_perf_console (~:2804): матч по console_list именам (tombstone-активный
    → None — корректно);
  - **step() `_` arm (30/31 в Tracks) → StepPerf по console_list — ОСОЗНАННО**:
    стрелки шагают перфы ПАПКИ;
  - load_performance_by_name (~:2810) НЕ трогать (читает канонический perf_list
    для path — алиасы резолвятся именем);
  - page_depth (~:958): добавить Tracks-плечо `Mode::Tracks => self.tracks_window / 7`
    (сейчас `_` arm показывает perf_window — мусор в Tracks);
  - top_colors внутренний match станет non-exhaustive с Mode::Tracks — плечо
    unreachable/zeros (mod.rs выбирает folder_colors для Tracks).

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`

- [ ] **Step 5: коммит**

```bash
git add src/gui/mod.rs src/lib.rs src/gui/layers.rs tests/config_folders.rs && git commit -m "feat: console perf list resolves active folder aliases with stable tombstone slots (#57)"
```

### Task 3: LoadPerf tombstone warn + toast

**Files:**
- Modify: `src/gui/mod.rs` LoadPerf handler (~:2683-2692)

- [ ] **Step 1: warn + toast на tombstone-пэде** — LoadPerf: `idx` за
  пределами ИЛИ path = None → `log::warn!("[console] perf slot {}: перф не
  найден (tombstone)", i)` + `self.show_toast("Perf missing (tombstone)",
  ToastKind::Warn)` (проверить наличие Warn-варианта ToastKind в gui/mod.rs;
  если только Success/Error — использовать Error).

```rust
            led_console::Action::LoadPerf(i) => {
                let idx = self.perf_window + i - 1; // slot -> real index
                match self.console_list.get(idx) {
                    Some((name, Some(_))) => {
                        let name = name.clone();
                        if self.load_performance_by_name(&name) {
                            log::info!("[console] perf slot {} loaded ({})", i, name);
                            self.follow_window(idx, true);
                        }
                    }
                    _ => {
                        // #57 tombstone: битая ссылка папки — не молчать
                        log::warn!("[console] perf slot {}: missing perf (tombstone)", i);
                        self.show_toast("Perf missing — edit Tracks", ToastKind::Error);
                    }
                }
            }
```

- [ ] **Step 2: гейт + коммит**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test --lib 2>&1 | tail -3
git add src/gui/mod.rs && git commit -m "feat: tombstone perf slot warns + toasts instead of silent no-op (#57)"
```

### Task 4: Mode::Tracks на консоли (MENU-слот 3)

**Files:**
- Modify: `src/gui/led_console.rs` — Mode::Tracks (:14-20), tap (:121-128
  слот 3 в Menu → Tracks; в Tracks: тап слота 1..=7 → папка; слот 0 → Home),
  top_colors arm для Tracks (папки: активная FF00FF, остальные 0066FF; пустые
  слоты тёмные; MENU-слот 0 белый), page/follow для Tracks (len = folders.len();
  окно tracks_window — НОВОЕ поле App или переиспользовать pat_window? НЕ
  переиспользовать: новое `tracks_window: usize` в App + lib.rs init +0)
- Modify: `src/gui/mod.rs` — console_prev_mode sync arm (Tracks → refresh +
  без follow), handle TrackSelect(i) (set active_folder + refresh + mode Home),
  page_window Tracks-плечо
- Modify: `src/lib.rs` — tracks_window init

- [ ] **Step 1: led_console.rs** — Mode::Tracks; tap():

```rust
            // Menu-слот 3 (#57): Tracks — выбор активной папки-трека
            3 => { self.mode = Mode::Tracks; None }
```

в Tracks-режиме tap(slot) — вставка ДО generic-обработки
`if slot == 0 { self.mode = Mode::Menu }` (иначе MENU-тап в Tracks открыл бы
меню вместо возврата в HOME):

```rust
        if self.mode == Mode::Tracks {
            // слот 0 = MENU (белый) — тап возвращает в HOME
            if slot == 0 { self.mode = Mode::Home; return None; }
            return Some(Action::TrackSelect(slot - 1)); // window-local, канон SelectPattern
        }
```

Action::TrackSelect(usize) новый вариант; тесты в tests-модуле led_console
(канон prev_next_step_semantics): Menu slot3 → Mode::Tracks; Tracks slot1 →
TrackSelect(0).

- [ ] **Step 2: top_colors** — мод выберет fn в mod.rs (Paint site: match
console.mode() → Home: top_colors(...), Tracks: folder_colors(...), остальное
как сейчас; компилятор заставит добавить arm в ВНУТРЕННИЙ match top_colors —
плечо unreachable/zeros). Для Mode::Tracks цвета папок:
  (вход: active_folder: Option<usize>, folders_len, tracks_window — расширить
  сигнатуру top_colors параметром `folders: Option<(Option<usize>, usize, usize)>`
  ИЛИ отдельная fn folder_colors(active, len, w) -> [[u8;3];8] и в mod.rs
  выбрать по mode; ПРЕДПОЧТЕНИЕ — отдельная fn, не раздувать сигнатуру):

```rust
    /// Пэды режима Tracks: слот 0 = MENU белый, 1..=7 = папки (активная
    /// FF00FF пурпур, остальные 0066FF синий, пустые — чёрный).
    pub fn folder_colors(
        &self, active_folder: Option<usize>, filled: usize, w: usize,
    ) -> [[u8; 3]; 8] {
        let mut out = [[0x00, 0x00, 0x00]; 8];
        out[0] = [0xFF, 0xFF, 0xFF]; // MENU
        for i in 1..8 {
            let idx = w + i - 1;
            if idx >= filled { break; }
            out[i] = if active_folder == Some(idx) { [0xFF, 0x00, 0xFF] } else { [0x00, 0x66, 0xFF] };
        }
        out
    }
```

- [ ] **Step 3: mod.rs** — Action::TrackSelect handler:

```rust
            led_console::Action::TrackSelect(i) => {
                let idx = self.tracks_window + i;
                if idx < self.config.ui.perf_folders.len() {
                    self.config.ui.active_folder = Some(idx);
                    let _ = self.config.save();
                    self.refresh_console_list();
                    self.perf_window = 0; // новая папка — с первой страницы
                    self.console.force_mode(led_console::Mode::Home);
                    // mode-entry sync подхватит след. кадром (prev=Tracks≠Home)
                }
            }
```

(verified: поле `mode` ПРИВАТНОЕ (:85), getter `pub fn mode()` (:301),
Mode уже derives PartialEq (:13) — хедж снят.) Для смены режима добавить в
led_console.rs каноничный мутатор и использовать ЕГО (поле не делать pub —
инкапсуляция state-машины):

```rust
    /// Программная смена режима (TrackSelect → Home, #57). Не через tap().
    pub fn force_mode(&mut self, m: Mode) {
        self.pending.clear();
        self.mode = m;
    }
```

page_window — Tracks-плечо clamp по folders.len(); mode-entry sync: Tracks →
refresh + без follow; `console_prev_mode` сравнение работает (PartialEq есть).

- [ ] **Step 4: гейт + коммит**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --lib 2>&1 | tail -3
git add src/gui/led_console.rs src/gui/mod.rs src/lib.rs && git commit -m "feat: console Tracks mode — MENU slot 3 lists folders, tap selects active track, HOME pages its perfs (#57)"
```

### Task 5: GUI — Tracks менеджер + селектор в Layers

**Files:**
- Modify: `src/gui/layers.rs` — в perf-строке рядом с perf ComboBox:
  ComboBox «Track: …» (значения: «(all)» + папки; выбор → active_folder +
  refresh_console_list) + кнопка «Tracks…» (открывает менеджер)
- Modify: `src/gui/mod.rs` — draw_tracks_manager (ModalWindow shell,
  fixed_width 520): список папок (rename TextEdit, delete ×-кнопка с
  confirm-каноном confirm.rs если есть — иначе двойной тап), «+ Add track»,
  для активной: список алиасов с ↑↓/× и ComboBox «add perf from all» +
  кнопка «+ add current perf»

- [ ] **Step 1: селектор в Layers** — рядом с perf ComboBox (:246-266):

```rust
                // #57: активный трек (папка перфов)
                let mut names: Vec<String> = vec!["(all perfs)".into()];
                names.extend(self.config.ui.perf_folders.iter().map(|f| f.name.clone()));
                let cur = self.config.ui.active_folder.map(|i| i + 1).unwrap_or(0);
                egui::ComboBox::from_id_salt("track_sel")
                    .selected_text(names.get(cur).map(|s| s.as_str()).unwrap_or("(all perfs)"))
                    .width(90.0)
                    .show_ui(ui, |ui| {
                        if ui.selectable_label(cur == 0, "(all perfs)").clicked() {
                            self.config.ui.active_folder = None;
                            let _ = self.config.save();
                            self.refresh_console_list();
                        }
                        for (fi, fname) in self.config.ui.perf_folders.iter().enumerate() {
                            if ui.selectable_label(cur == fi + 1, &fname.name).clicked() {
                                self.config.ui.active_folder = Some(fi);
                                let _ = self.config.save();
                                self.perf_window = 0;
                                self.refresh_console_list();
                            }
                        }
                    });
                if ui.small_button("Tracks…").on_hover_text("Manage track folders (setlists)").clicked() {
                    self.show_tracks_manager = true;
                }
```

(поле `show_tracks_manager: bool` + false в литералах App/lib.rs — канон
show_pad_perf. BORROW: снапшот `let folders: Vec<(usize, String)> =
self.config.ui.perf_folders.iter().enumerate().map(|(i,f)|(i,f.name.clone())).collect();`
ДО ComboBox; замыкание итерирует folders (owned), клики сохраняют решение в
локаль, мутация + refresh_console_list() — после замыкания. Тот же
copy-then-act канон, что в ВНИМАНИЕ-блоке Step 2 — распространяется и сюда.)

- [ ] **Step 2: менеджер** — ModalWindow (fixed_width(520.0)). Скелет
  draw_tracks_manager (ui, ctx) — вызов вCentralPanel-хвосте рядом с другими
  модалками, канон inst_browser:

```rust
    fn draw_tracks_manager(&mut self, ctx: &egui::Context) {
        if !self.show_tracks_manager { return; }
        let mut open = true;
        let mut win = super::modal::ModalWindow::new(
            ctx, "tracks_manager", "Track Folders (Setlists)", &mut open, 520.0)
            .fixed_width(520.0);
        win.show(|ui| {
            ui.label(egui::RichText::new("Папки-треки: алиасы перфов. Правка перфа — в оригинале.").weak());
            for (fi, folder) in self.config.ui.perf_folders.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let active = self.config.ui.active_folder == Some(fi);
                    if ui.selectable_label(active, &folder.name).on_hover_text("Set active").clicked() {
                        self.config.ui.active_folder = Some(fi);
                        let _ = self.config.save();
                        self.perf_window = 0;
                        self.refresh_console_list();
                    }
                    if ui.small_button("↑").clicked() && fi > 0 {
                        self.config.ui.perf_folders.swap(fi - 1, fi);
                        if self.config.ui.active_folder == Some(fi) { self.config.ui.active_folder = Some(fi - 1); }
                    }
                    if ui.small_button("↓").clicked() && fi + 1 < self.config.ui.perf_folders.len() {
                        self.config.ui.perf_folders.swap(fi, fi + 1);
                        if self.config.ui.active_folder == Some(fi) { self.config.ui.active_folder = Some(fi + 1); }
                    }
                    if ui.small_button("\u{00D7}").on_hover_text("Delete folder").clicked() {
                        self.config.ui.perf_folders.remove(fi);
                        if self.config.ui.active_folder == Some(fi) { self.config.ui.active_folder = None; }
                        else if let Some(a) = self.config.ui.active_folder { if a > fi { self.config.ui.active_folder = Some(a - 1); } }
                        let _ = self.config.save();
                        self.refresh_console_list();
                    }
                });
                // алиасы папки: имя + [×]; add-perf ComboBox
                for pi in (0..folder.perfs.len()).rev() {
                    ui.horizontal(|ui| {
                        ui.label(format!("  {}", folder.perfs[pi]));
                        if ui.small_button("\u{00D7}").clicked() { folder.perfs.remove(pi); self.refresh_console_list(); }
                    });
                }
                ui.horizontal(|ui| {
                    let cur = self.parts[self.active_part].perf_name.clone();
                    if ui.small_button("+ add current perf").clicked() {
                        folder.perfs.push(cur.clone()); self.refresh_console_list();
                    }
                    egui::ComboBox::from_id_salt(format!("add_perf_{fi}"))
                        .selected_text("+ perf from list")
                        .show_ui(ui, |ui| {
                            for (n, _) in &self.perf_list {
                                if ui.selectable_label(false, n).clicked() {
                                    folder.perfs.push(n.clone()); self.refresh_console_list();
                                }
                            }
                        });
                }
                ui.separator();
            });
            if ui.button("+ New track").clicked() {
                let n = self.config.ui.perf_folders.len() + 1;
                self.config.ui.perf_folders.push(
                    crate::config::PerfFolder { name: format!("Track {n}"), perfs: Vec::new() });
                let _ = self.config.save();
                self.refresh_console_list();
            }
        });
        if !open { self.show_tracks_manager = false; }
    }
```

ВНИМАНИЕ реализатору: итерация `iter_mut()` по perf_folders + ОДНОВРЕМЕННЫЙ
`&self`-доступ (active_part, perf_list, refresh_console_list &mut self) внутри
замыканий — НЕ скомпилируется как написано. Правильный порядок: (1) собрать
данные мутаций в локальные переменные ДО замыкания (used_perf = имя текущего
перфа, active_folder копия, len копия), (2) внутри замыкания только читать
folder.perfs через индекс (`let f = &mut self.config.ui.perf_folders[fi]` —
borrowck: цикл по индексам, не iter_mut), (3) мутации config + refresh —
после замыкания по сохранённому решению (_pattern: egui-виджеты возвращают
response, мутация по .changed()/clicked() вне). Разрешать borrow-конфликты
по канону Sus-чекбокса (copy-then-act). Точная раскладка мутаций — на
реализаторе; тесты не покрывают GUI, гейт — компиляция + clippy + экран.
ПЕРСИСТ: КАЖДАЯ мутация папок (create/rename/delete/reorder/add/remove) —
`let _ = self.config.save();` (канон TrackSelect handler): setlists иначе
исчезают при рестарте (config.save() event-driven, на выходе не гарантирован).

- [ ] **Step 3: гейт + коммит**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30
git add src/gui/layers.rs src/gui/mod.rs src/lib.rs && git commit -m "feat: Tracks manager modal + track selector in perf row (#57)"
```

### Task 6: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): создать папки «Track 1» (Bass Test, Organ), «Track 2»
  (Carol, Halloween); консоль MENU → 3-й слот → Tracks → выбрать Track 1 →
  HOME пэды показывают 2 перфа папки; LoadPerf/стрелки в пределах папки;
  GUI ComboBox синхронен; tombstone: переименовать файл перфа вне приложения →
  пэд тёмный + warn + toast; (all perfs) — старое поведение
- [ ] TECHDEBT #57: ГОТОВО (tombstone/manager/Tracks-режим), Out of scope
  зафиксировать

## Out of scope

- Drag-and-drop reorder папок/алиасов (egui_dnd) — ↑↓ кнопок достаточно (#54
  отдельно для партов).
- UUID-идентификация перфов — имена (канон #49); UUID при появлении
  мультихоста.
- (Init) в папках — алиасом может быть любой перф включая (Init).
- CBOR/атомарный save — config.save() уже json; atomic-write при случае.
- Export/import сетлистов (портативность) — позже.
