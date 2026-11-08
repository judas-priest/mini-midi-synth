# params.rs split: draw_params_editable_for (~1400 строк) → секции/файлы Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 603ee87; src/gui/params.rs 1666 строк;
draw_params_editable_for 251-~1660; хелперы 24-249)

**Goal:** монолит-метод режется на секции (verbatim), секции разъезжаются в
файлы params_osc.rs / params_filter.rs / params_env.rs / params_lfo.rs /
params_matrix.rs — behavior-preserving, goldens не задеты (GUI-рендер
параметров не в golden-наборе; bit-exactness обеспечивается verbatim-вырезкой).

**Architecture (review round-1 — 2 Critical перепроектированы):**
- СЕКЦИЯ = egui::Frame-блок. Тело draw_params_editable_for состоит из 9
  `egui::Frame::default().fill(BG_SECTION)...show(ui, |ui| {...})` блоков
  (концы помечены `}); // end X frame`: 816, 972, 1016, 1046, 1226, 1302,
  1385, 1469, 1642). Плоская вырезка на внешнем ui теряет BG_SECTION/
  margins/скругления — НЕЛЕГАЛЬНО (review C2). Декомпозиция: builder Frame
  уезжает ВНУТРЬ секцию-метод, parent вызывает `changed |= self.
  draw_sec_x(ui, part);` на месте прежнего блока.
- style_mut: 9 сайтов (280 Osc SECONDARY, 826 Filter PRIMARY, 982 Env
  PRIMARY, 1026 Dynamics SECONDARY, 1055 LFO SECONDARY, 1235 Matrix
  SECONDARY, 1312 Voice COMPACT, 1394 Arp COMPACT, 1479 Effects SECONDARY) —
  каждый остаётся ВНУТРИ своей секции на её Frame-ui (review C1: hoist
  SLIDER_SECONDARY на вход сделал бы все секции SECONDARY — тихий
  layout-регресс, компилятор/goldens не поймают).
- Фаза A (в params.rs): секция → `fn draw_sec_<name>(&mut self, ui, part)
  -> bool` (frame builder внутри, вернул changed). Фаза B: разнос в файлы.
- Файлы-приёмники (review I4 — 9 секций, не 5): osc (Oscillator 1/2,
  OscShaper), filter, env, lfo (LFO+Scene), matrix, dynamics, voice, arp,
  effects → 4 остающихся (dynamics/voice/arp/effects) ОСТАЮТСЯ в params.rs
  (явно указано; метрика финала: params.rs ~1666 → ~450-500, НЕ ~200-300).
- ПУЛЬС-ИСКЛЮЧЕНИЕ (review I3): блок pulse-width (598-610, osc_type==2)
  ставит `parts[part].params_dirty = true` НАПРЯМУЮ (607) БЕЗ `changed` —
  verbatim, НЕ нормализовать (иначе изменится engine-нотификация — нечём
  поймать). draw_macros_for не даёт changed — head вне секций.
- ГВАРД (review I5, канон пинов CLAUDE.md #8): source-parse pin-тест
  `params_sections_pin`: порядок `id_salt`/`ui.strong` строк, извлечённых из
  draw_params_editable_for + секций — фиксирует layout-эквивалентность
  механически (goldens здесь слепы).

**Tech Stack:** Rust; компилятор ведёт (замыкания/локалы секции); goldens
+ rt_safety формальные гейты.

**Проверенные факты (603ee87):**
- Свободные хелперы 24-249: filter_vis_category, compute_filter_response,
  freq_to_x, db_to_y, draw_filter_response (82), env_shape_combo (137),
  draw_lfo_trigger_mode (162) + метод draw_adsr_curve (190) — переезжают в
  файлы соответствующих секций.
- draw_params_editable_for: 251+; 9 Frame-секций (концы-маркеры выше);
  хвост 1645-1648: `if changed { parts[part].params_dirty = true;
  send_edited_params(part); }`; undo-пуши inline внутри секций
  (push_undo_snapshot_for — pub(super) mod.rs:2140) — едут verbatim.
- Хелперы 24-186 свободные, используются только внутри params.rs (grep
  внешних = 0) — переезд module-private безопасен; draw_adsr_curve (190-245) —
  impl App метод (не свободная fn), в params_env.rs внутри impl.
- Мёртвый комментарий 247-250 (упоминает несуществующий
  draw_params_editable wrapper) — поправить в фазе A.
- Единственный вызов метода: gui/browser.rs:413 (ScrollArea inner ui) —
  секции получают его транзитивно, ок.

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

- [ ] Step 1: карта секций: якоря = `egui::Frame::default()` / `// end X
  frame` (НЕ ui.strong — пропускает sub-заголовки ui.label: "Hard Sync" 685,
  "Supersaw" 707) → таблица (секция, строки, style_mut-сайт, локалы за
  границей). Верифицировать хвост changed (1645-1648) и pulse-width
  исключение (598-610/607). НЕ коммитить код.

### Task 1: фаза A — секции → методы (в том же файле)

**Files:** Modify `src/gui/params.rs`.

- [ ] Step 1: по карте Task 0 — каждый FRAME-блок → `fn draw_sec_<name>(
  &mut self, ui: &mut egui::Ui, part: usize) -> bool`: builder Frame + его
  style_mut переезжают ВНУТРЬ fn (review C1/C2 — verbatim, включая
  pulse-width исключение 598-610 КАК ЕСТЬ; draw_macros_for/head — не секция);
  parent: `changed |= self.draw_sec_x(ui, part);` на месте блока, порядок
  1:1. Хвост 1645-1648 не трогать.
- [ ] Step 2: gates: clippy -D warnings; cargo test; golden_fx.
- [ ] Step 3: pin-тест `params_sections_pin` (модуль tests в params.rs):
  извлекает упорядоченный список id_salt/ui.strong строк из тела метода+
  секций (include_str!-стиль исходника) — фиксирует layout-эквивалентность.
- [ ] Step 4: gates: clippy; cargo test (вкл. pin); golden_fx.
- [ ] Step 5: коммит `refactor: params — sections as methods (verbatim, phase A)`.

### Task 2: фаза B — разнос по файлам

**Files:** Create `src/gui/params_osc.rs`, `params_filter.rs`, `params_env.rs`,
`params_lfo.rs`, `params_matrix.rs`; Modify `src/gui/params.rs`,
`src/gui/mod.rs` (mod-декларации).

- [ ] Step 1: перенос по владельцу: osc-секции + osc-хелперы → params_osc.rs;
  filter (draw_filter_response + compute/freq/db/db_y + filter-секции) →
  params_filter.rs; env (draw_adsr_curve — impl App метод, env_shape_combo +
  env-секции) → params_env.rs; lfo (draw_lfo_trigger_mode + LFO/Scene) →
  params_lfo.rs; mod-matrix → params_matrix.rs. DYNAMICS/VOICE/ARP/EFFECTS
  остаются в params.rs (review I4 — иначе метрика врёт). Видимости: fn →
  pub(super) где cross-file; свободные хелперы — module-private (внешних
  потребителей нет, grep=0). Мёртвый комментарий 247-250 поправить.
- [ ] Step 2: gates: clippy; cargo test; golden_fx; rt_safety.
- [ ] Step 3: коммит `refactor: params — sections moved to params_*.rs (phase B)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens.
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.
- [ ] Step 3: метрика в PR: params.rs до/после (1666 → ~450-500: dynamics/
  voice/arp/effects остаются; review I4), список новых файлов + таблица
  Task 0.

## Out of scope
- Изменение структуры/логики секций (только move) — ux-правки отдельно.
- draw_params_editable_for wrappers (draw_macros и т.п.) — уже вынесены в
  macros.rs ранее.
- Config-сжатие патч-параметров — не здесь.
