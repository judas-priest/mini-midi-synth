# UI Standardization: spacing/size tokens, layout hygiene (separate PR)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Вся геометрия GUI — через токены theme.rs (spacing кратен 4px, ширины виджетов одного типа одинаковы), 7 ScrollArea получают id_salt, однотипные контролы унифицированы. Ноль изменений поведения, только пиксели/константы. Работа в ветке `ui-standardization` → отдельный PR в master.

**Architecture:** Практика (Shopify spacing tokens, learnvisual 4px grid, law of proximity): константы вместо голых пикселей; один тип виджета = одна ширина; клипы/id_salt против «уезжания». Три задачи: (1) токены в theme.rs по итогам аудита, (2) механическая замена литералов, (3) hygiene-fixes. Инвентарь аудита — в плане ниже (файл:строка актуальны для master на момент плана; при сдвиге — grep по значению).

**Tech Stack:** Rust/egui 0.31, theme.rs; ветка + `gh pr create`.

**ИНВЕНТАРЬ (аудит 2026-10-31, агент):** см. ключевые позиции в задачах; полный аудит хранится в истории плана. Приоритет: только значения, чья замена не меняет поведение (ширины/отступы/радиусы). Canvas-геометрия (drum cell 28×22, keyboard, XY pad) переносится в токены БЕЗ изменения значений.

---

## Test Cases

1. **TC1 Токены существуют и используются:** `cargo build` зелёный; grep по прежним литералам в gui/ даёт только токен-определения в theme.rs и разрешённые исключения из Negative Matrix.
2. **TC2 Ноль поведенческих изменений:** все 29 тестов зелёные (ui_layout_contract и др.), `cargo clippy -D warnings`, rt_safety.
3. **TC3 Hygiene:** все ScrollArea в gui/ имеют id_salt; `grep allocate_ui` — единственное оставшееся (drum grid label) помечено ban-комментарием.
4. **TC4 Унификация однотипного:** path TextEdit = 300 везде; GM-комбо = 200 везде; velocity DragValue = 80 везде; слайдер-дефолт движка = SLIDER_PRIMARY.
5. **TC5 PR:** ветка ui-standardization, PR создан `gh pr create`, CI-гейты в описании.

## Negative Matrix

- **Пиксельная разница после унификации** (240→300 path, 120→200 GM combo, 60→80 velocity DragValue, 70→80 arp-mode combo) — намеренная стандартизация, задокументирована. Остальные замены = значение 1:1.
- **Дефолт item_spacing.y = 5.0 и Margin::same(6)** — офф-грид, но load-bearing (глобальный стиль/10 карточек): НЕ трогаем (менять = переверстка всего), выносим в токены APP_ITEM_SPACING/`CARD_MARGIN` с текущими значениями.
- **Драм-грид item_spacing 1.0 и pad_gap 3.0** — намеренная пиксельная плотность: токены GRID_TIGHT/PAD_GAP с теми же значениями.
- **Android-стиль item_spacing 10×8** — отдельная платформенная ветка: токен ANDROID_ITEM_SPACING, значение не меняется.
- **allocate_ui в drums.rs:156** — единственный выживший (внутри грид-строки, исторически работает); ban-комментарий, не переписываем (риск развалки на ровном месте).
- **fx_chain/midi_seq горизонтальный оверфлоу** — известный, чинится ScrollArea::both — ВЫНЕСЕНО (поведенческое изменение, отдельный пункт TECHDEBT), этот план только токены+соль.
- **Шириныcombo без width (~40 штук)** — auto-width по контенту; принудительно не назначаем (переверстка), только фиксируем существующие явные ширины в токены.
- **visual diff** — затронутые пиксели: path поля шире, GM-комбо шире, velocity DragValue шире. Приемлемо (TC4).

---

### Task 0: Ветка

- [x] `git checkout -b ui-standardization` (от свежего master).

### Task 1: Токены theme.rs

**Files:**
- Modify: `src/gui/theme.rs`

- [x] **Step 1: Добавить секции токенов (значения = текущие литералы, кроме помеченных unify):**

```rust
// ── Spacing (4px grid; legacy off-grid значения помечены) ──────────
pub const SP_XL: f32 = 16.0;          // settings help grids
pub const GRID_TIGHT: f32 = 1.0;      // drum cell pitch (intentional pixel-tight)
pub const PAD_GAP: f32 = 3.0;         // keyboard pads / perf window / zone gap
pub const APP_ITEM_SPACING: egui::Vec2 = egui::vec2(8.0, 5.0);   // global default (legacy y=5)
pub const ANDROID_ITEM_SPACING: egui::Vec2 = egui::vec2(10.0, 8.0);
pub const GRID_SP_FORM: [f32; 2] = [12.0, 6.0];   // settings forms
pub const GRID_SP_LIST: [f32; 2] = [8.0, 4.0];    // macros/midi_seq grids
pub const GRID_SP_HELP: [f32; 2] = [16.0, 2.0];   // CC help grids
pub const GRID_SP_FX: [f32; 2] = [4.0, 2.0];      // fx chain grid
pub const CARD_RADIUS: f32 = 4.0;
pub const CARD_MARGIN: f32 = 6.0;     // Margin::same (legacy off-grid, load-bearing)
pub const TOAST_MARGIN: (i8, i8) = (12, 6);

// ── Control widths ─────────────────────────────────────────────────
pub const COMBO_W_2CH: f32 = 50.0;
pub const COMBO_W_SM: f32 = 80.0;
pub const COMBO_W_MD: f32 = 120.0;
pub const COMBO_W_LG: f32 = 200.0;    // unify 120(GM remap)/200/220(SF2 files)
pub const COMBO_W_XL: f32 = 280.0;
pub const TEXT_W_NAME: f32 = 120.0;   // unify 80/110/120 name edits
pub const TEXT_W_PATH: f32 = 300.0;   // unify 240/300 path edits
pub const DRAG_W_SM: f32 = 60.0;
pub const DRAG_W_MD: f32 = 80.0;      // unify 60/80 velocity DragValues
pub const BUTTON_W_NAME: f32 = 170.0; // part-row instrument buttons
pub const PROGRESS_W_SM: f32 = 80.0;
pub const PROGRESS_W_MD: f32 = 160.0;
pub const MODAL_W_SM: f32 = 300.0;
pub const MODAL_W_MD: f32 = 360.0;
pub const MODAL_W_LG: f32 = 900.0;   // pad remap modal (1:1)
pub const LIST_MAX_H: f32 = 420.0;
pub const POPUP_LIST_MAX_H: f32 = 300.0;

// ── Canvas geometry (values unchanged — tokenized) ────────────────
pub const DRUM_CELL_W: f32 = 28.0;
pub const DRUM_CELL_H: f32 = 22.0;
pub const DRUM_LABEL_W: f32 = 80.0;
pub const BEAT_LED: f32 = 14.0;
pub const PITCH_STEP_MIN: f32 = 20.0;
pub const PITCH_STEP_MAX: f32 = 40.0;
pub const PITCH_BAR_H: f32 = 80.0;
pub const TIMELINE_H: f32 = 40.0;
pub const TIMELINE_W_MAX: f32 = 600.0;
pub const KEY_W_MIN: f32 = 6.0;
pub const KEY_W_MAX: f32 = 18.0;
pub const KEY_H_MIN: f32 = 70.0;
pub const PAD_SIZE_MIN: f32 = 32.0;
pub const PAD_SIZE_MAX: f32 = 48.0;
pub const PAD_PERF_W: f32 = 80.0;
pub const PAD_PERF_H: f32 = 52.0;
pub const ZONE_MAP_H: f32 = 8.0;
pub const ZONE_MAP_W_MAX: f32 = 500.0;
pub const PART_LIST_RESERVE: f32 = 36.0;
pub const FILTER_RESP_W: f32 = 300.0;
pub const FILTER_RESP_H: f32 = 80.0;
pub const XY_PAD_SIZE: f32 = 200.0;
pub const SCOPE_W: f32 = 150.0;
pub const SCOPE_H: f32 = 40.0;
pub const DOCK_DEFAULT_H: f32 = 500.0;
pub const DOCK_H_MIN: f32 = 200.0;
pub const DOCK_H_MAX: f32 = 760.0;
```

- [x] **Step 2: Гейт** `cargo build` (неиспользуемые pub const — ок, они pub).

### Task 2: Механическая замена литералов

**Files:** src/gui/{mod,drums,params,layers,settings,macros,midi_seq,fx_chain,keyboard}.rs

- [x] **Step 1: item_spacing / Grid::spacing / Margin / corner_radius → токены:**
  - mod.rs:479 → `ANDROID_ITEM_SPACING`; mod.rs:496 → `APP_ITEM_SPACING`; mod.rs:1240 → `.x = theme::SP_SM` (4.0); mod.rs:495 `slider_width(250.0)` → `slider_width(theme::SLIDER_PRIMARY)`.
  - drums.rs:164 → `GRID_TIGHT` (vec2(GRID_TIGHT, GRID_TIGHT)).
  - Grid::new spacing: macros.rs:21/midi_seq.rs:138 → `GRID_SP_LIST`; drums.rs:455 → ОСТАВИТЬ литералом `[8.0, 6.0]` (GRID_SP_FORM был бы 12 — неверная замена); fx_chain.rs:42 → `GRID_SP_FX`; settings.rs:138,158 → `GRID_SP_HELP`; settings.rs:599,672 → `GRID_SP_FORM`.
  - params.rs ×10 `Margin::same(6)`+`corner_radius(4.0)` → `Margin::same(theme::CARD_MARGIN as i8)`+`corner_radius(theme::CARD_RADIUS)`; mod.rs:1134-1135 toast → `corner_radius(theme::CARD_RADIUS)`, `Margin::symmetric(TOAST_MARGIN.0, TOAST_MARGIN.1)`.
  - settings.rs:38/111 `gap=3.0` → `theme::PAD_GAP`.
- [x] **Step 2: TextEdit/ComboBox/DragValue/ProgressBar/модали → токены:**
  - drums.rs:341 → TEXT_W_NAME; drums.rs:404 → TEXT_W_PATH; layers.rs:200 → TEXT_W_NAME (110→120); macros.rs:34 → TEXT_W_NAME (80→120); midi_seq.rs:87 → TEXT_W_PATH.
  - drums.rs:606 → COMBO_W_2CH; params.rs:1437 → COMBO_W_2CH; params.rs:1422 → COMBO_W_SM (70→80); drums.rs:621/params.rs:1259 → COMBO_W_SM; params.rs:150,1273/macros.rs:95,110 → 90.0 оставить литералом? НЕТ — добавить `COMBO_W_90: f32 = 90.0` НЕ нужно: 90→COMBO_W_MD (120) меняет пиксели; РЕШЕНИЕ: 90.0 → оставить как есть с комментарием `// legacy width` (не всё унифицировать ценой верстки мод-матрицы). Аналогично 110/115/140 оставить. drums.rs:463 (120, GM remap) → COMBO_W_LG (200, unify с midi_seq.rs:231); drums.rs:543, midi_seq.rs:231 → COMBO_W_LG; settings.rs:713,743 → COMBO_W_LG (220→200); settings.rs:679 → COMBO_W_XL; params.rs:570, drums.rs:923 → оставить 160.0 (legacy) — нет: добавить токен НЕ нужно, оставить.
  - drums.rs:471 → DRAG_W_SM… unify 60/80 → ОБЕ в DRAG_W_MD (80): drums.rs:471 `[80.0, 0.0]` (было 60), drums.rs:553 `[80.0, 0.0]` → `add_sized([theme::DRAG_W_MD, 0.0], …)`.
  - layers.rs:348,365 → `add_sized([theme::BUTTON_W_NAME, 0.0], …)`.
  - drums.rs:857 → PROGRESS_W_SM; midi_seq.rs:68 → PROGRESS_W_MD.
  - drums.rs:448 → MODAL_W_LG; drums.rs:538 → MODAL_W_SM; mod.rs:1232 → MODAL_W_MD; layers.rs:175 → оставить 280.0 (legacy `remove-part modal`) — токен `MODAL_W_280` не заводим, оставить литерал с комментарием; settings.rs:98 → PAD_PERF_W (140→80?? НЕТ — settings.rs:98 это popup 140 — оставить литералом).
  - midi_seq.rs:234 → POPUP_LIST_MAX_H; mod.rs:1258 → LIST_MAX_H.
- [x] **Step 3: Canvas-геометрия → токены (значения 1:1):**
  - drums.rs:144-146 → DRUM_CELL_W/H, DRUM_LABEL_W; drums.rs:60 → BEAT_LED (vec2 14×14; radius 5.0 на :62 — без токена).
  - drums.rs:661-666 → PITCH_STEP_MIN/MAX, PITCH_BAR_H.
  - drums.rs:1088-1089 → TIMELINE_W_MAX/H.
  - keyboard.rs:60-66 → PAD_GAP(60), PAD_SIZE clamp(61), KEY_W clamp(65), KEY_H(66); строка 62 (margin 12.0) — без токена.
  - settings.rs:36-38 → PAD_PERF_W/H, PAD_GAP.
  - layers.rs:113-114 → PART_LIST_RESERVE; layers.rs:513-514 → ZONE_MAP_W_MAX/H.
  - params.rs:89-90 → FILTER_RESP_W/H; macros.rs:133 → XY_PAD_SIZE; mod.rs:1024-1025 → SCOPE_W/H.
  - mod.rs:1079-1080 → DOCK_DEFAULT_H/H_MIN/H_MAX.
- [x] **Step 3b: НЕ трогать:** settings.rs font 11.0 ×3 и macros.rs 11.0 (токен FS_OVERLAY заводить не будем — мелочь); midi_seq.rs:142 size(13.0); панели header/global_controls (auto-sized); все 90/110/115/140/160 combo widths (legacy); модальные 280/140.

- [x] **Step 4: Гейты** `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety && cargo build --release`.

- [x] **Step 5: Commit** `git commit -m "refactor: gui geometry through theme tokens (values 1:1, unify path/name/GM-combo/velocity widths)"`.

### Task 3: Layout hygiene

**Files:** src/gui/{mod,midi_seq}.rs

- [x] **Step 1: id_salt всем ScrollArea (7):** mod.rs:1156 → `"fx_chain_scroll"`; mod.rs:1160 → `"midi_seq_scroll"`; mod.rs:1164 → `"drum_kit_scroll"`; mod.rs:1210 → `"params_scroll"`; mod.rs:1257 → `"inst_modal_list"`; midi_seq.rs:134 → `"midi_tracks_scroll"`; midi_seq.rs:233 → `"gm_popup_scroll"`.
- [x] **Step 2: Ban-комментарий** на drums.rs:156 `allocate_ui`: `// NOTE: allocate_ui внутри horizontal-строк — известный источник развалки (mod.rs docked panel); здесь легаси-грид, работает — не реплицировать`.
- [x] **Step 3: Гейты + Commit** `git commit -m "fix: id_salt on all ScrollAreas; ban-comment on grid allocate_ui"`.

### Task 4: PR

- [x] **Step 1:** `git push -u origin ui-standardization`.
- [x] **Step 2:** `gh pr create --title "UI standardization: theme tokens, widget width unification, ScrollArea hygiene" --body "$(cat <<'EOF'
## Summary
- Вся геометрия GUI через токены theme.rs (spacing 4px grid, ширины виджетов, canvas-геометрия 1:1)
- Унификация однотипных виджетов: path 240/300→300, GM combo 120/200→200, velocity DragValue 60/80→80, слайдер-дефолт → SLIDER_PRIMARY
- id_salt на всех 7 ScrollArea (защита от коллизий ID), ban-комментарий на легаси allocate_ui
## Test plan
- [x] cargo clippy -D warnings, cargo test (29 suites), rt_safety
- [ ] Ручной осмотр: Layers/Drums/docked-панель — ничего не уехало
- id_salt на 7 ScrollArea сбрасывает их запомненный скролл один раз — не поломка
EOF
)"`.
- [ ] **Step 3: Ручной чеклист (юзер):** визуальный осмотр — Layers, Drums, docked-панель, настройки: отступы ровные, ничего не уехало; path/GM-поля чуть шире; arp-mode combo 70→80; velocity DragValue 60→80 (стандартизация).
