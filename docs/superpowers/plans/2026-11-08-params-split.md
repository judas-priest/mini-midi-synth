# params.rs split: draw_params_editable_for (~1400 строк) → секции/файлы Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 603ee87; src/gui/params.rs 1666 строк;
draw_params_editable_for 251-~1660; хелперы 24-249)

**Goal:** монолит-метод режется на секции (verbatim), секции разъезжаются в
файлы params_osc.rs / params_filter.rs / params_env.rs / params_lfo.rs /
params_matrix.rs — behavior-preserving, goldens не задеты (GUI-рендер
параметров не в golden-наборе; bit-exactness обеспечивается verbatim-вырезкой).

**Architecture:** двухфазно. Фаза A (в том же файле): каждый section-блок
→ `fn draw_sec_<name>(&mut self, ui, part) -> bool` (возвращает changed —
сверить семантику финального использования `changed` в хвосте
draw_params_editable_for и сохранить порядок вызовов 1:1). Фаза B: методы
разносятся в 5 файлов `impl App` (канон проекта — impl App разложен по
файлам: browser.rs, sf2.rs и т.д.).
ОБЩЕЕ СОСТОЯНИЕ: `ui.style_mut().spacing.slider_width = SLIDER_SECONDARY`
(на входе метода ~280) — работает на общий ui и ПЕРСИСТИТ между вызовами
секций (style_mut мутирует ui) — вызовы секций последовательны на том же
ui, поведение не меняется; после вырезки установить style ОДИН раз в
draw_params_editable_for до первого вызова секции.

**Tech Stack:** Rust; компилятор ведёт (замыкания/локалы секции); goldens
+ rt_safety формальные гейты.

**Проверенные факты (603ee87):**
- Свободные хелперы 24-249: filter_vis_category, compute_filter_response,
  freq_to_x, db_to_y, draw_filter_response (82), env_shape_combo (137),
  draw_lfo_trigger_mode (162) + метод draw_adsr_curve (190) — переезжают в
  файлы соответствующих секций.
- draw_params_editable_for: 251+; вход: ui.style_mut slider_width
  SLIDER_SECONDARY (~280); секции маркированы `ui.strong("...")` заголовками
  (Oscillator 1/2, OscShaper, Filter, Envelope, LFO, Scene LFO, Mod Matrix —
  инвентарь Task 0 уточняет точные границы/порядок).
- Хвост `changed` — сверить использование (push/undo) и прокинуть через
  возвращаемые значения секций.

---

## Test Cases (from the requirement)

- TC1: goldens байт-стабильны (рендер params не в golden — гейт формальный,
  но обязателен по канону).
- TC2: cargo test все gui pure-fn зелёные (затронутые helpers чистые fn —
  переезжают verbatim).
- TC3: код-ревью: КАЖДАЯ секция — verbatim-тело, порядок вызовов = старому
  линейному порядку, `changed`-семантика сохранена (Diff-ревью по секциям).
- Негатив: секция использует локал предыдущей секции — компилятор (E0425/
  E0382) укажет; такой локал = параметр новой fn (по значению/ссылке по
  факту использования — мутируется ли).

---

### Task 0: инвентарь секций

**Files:** Нет (таблица в отчёт).

- [ ] Step 1: карта секций: grep `ui.strong(` в теле draw_params_editable_for
  → таблица (заголовок, строки-границы, локалы-выходы за границы секции).
  Верифицировать хвост `changed` (строки ~1650-1660): что происходит при
  changed==true. НЕ коммитить код.

### Task 1: фаза A — секции → методы (в том же файле)

**Files:** Modify `src/gui/params.rs`.

- [ ] Step 1: по карте Task 0 — каждый section-блок → `fn draw_sec_<name>(
  &mut self, ui: &mut egui::Ui, part: usize) -> bool` verbatim-телом
  (возвращать накопленный changed секции; свободные локалы секции — параметры
  по факту). Порядок вызовов в draw_params_editable_for сохранён 1:1;
  style_mut — до первого вызова.
- [ ] Step 2: gates: clippy -D warnings; cargo test; golden_fx.
- [ ] Step 3: коммит `refactor: params — sections as methods (verbatim, phase A)`.

### Task 2: фаза B — разнос по файлам

**Files:** Create `src/gui/params_osc.rs`, `params_filter.rs`, `params_env.rs`,
`params_lfo.rs`, `params_matrix.rs`; Modify `src/gui/params.rs`,
`src/gui/mod.rs` (mod-декларации).

- [ ] Step 1: перенос по владельцу: osc-секции + osc-хелперы → params_osc.rs;
  filter (draw_filter_response + compute/freq/db/db_y + filter-секции) →
  params_filter.rs; env (draw_adsr_curve + env_shape_combo + env-секции) →
  params_env.rs; lfo (draw_lfo_trigger_mode + LFO/Scene-секции) →
  params_lfo.rs; mod-matrix секция → params_matrix.rs. Видимости: fn →
  pub(super) где cross-file.
- [ ] Step 2: gates: clippy; cargo test; golden_fx; rt_safety.
- [ ] Step 3: коммит `refactor: params — sections moved to params_*.rs (phase B)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens.
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.
- [ ] Step 3: метрика в PR: params.rs до/после (1666 → ~200-300), список
  новых файлов.

## Out of scope
- Изменение структуры/логики секций (только move) — ux-правки отдельно.
- draw_params_editable_for wrappers (draw_macros и т.п.) — уже вынесены в
  macros.rs ранее.
- Config-сжатие патч-параметров — не здесь.
