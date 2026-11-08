# App ~180 полей → сабструкты Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (re-checked на 603ee87: App struct gui/mod.rs:296-579,
литерал App::new 592-791; механика = compiler-driven move)

**Goal:** App struct (≈180 полей, god-struct) разбивается на 12 подсистем-сабструктов;
доступ `self.x` → `self.group.x`; behavior-preserving, чистый mechanical move.

**Architecture:** все substruct-ы в ОДНОМ файле `src/gui/app_structs.rs`
(review #6 — Architecture ранее противоречил Tasks), App держит их как поля. Механика на группу: создать struct со списком полей
(verbatim декларации), заменить в App-литерале инициализацию литералом
субструкта, далее КОМПИЛЯТОР ведёт: E0609 no field → точечные правки
`self.x` → `self.group.x` (grep-driven, механика без решений).
ПРИВАТНОСТЬ: поля App pub(crate)/pub — сохранять те же видимости у
substruct-полей (util-файлы gui обращаются напрямую).

**Tech Stack:** Rust; гейты: clippy/cargo test/rt_safety/goldens —
все behavior-preserving; gui pure-fn тесты (mod.rs:2635 и др.) остаются
зелёными (трогают только задетые поля — compiler укажет).

**Проверенные факты (603ee87):**
- App struct: src/gui/mod.rs:296-579 (≈170-180 полей; точный список —
  инвентарь Task 0).
- Литерал App::new: 592-791 (все поля литералом — E0063 гарант полноты).
- Группировка-рубрика (12 групп — расширено по review #3: иначе misc = 70
  полей, не-подсистема):
  transport (play/rec/bpm/metro/undo/redo), drum_seq (паттерны/chain/steps),
  looper (capture/слои), midi_seq (пути/треки/файл-pick), browser (поиск/
  индексы/категории/позиция/размер), perf (setlist/pad-perf/last_performance),
  led (socket/консоль-состояние),
  settings_audio (current_host/hosts/sample_rates/buffer/midi-port/is_jack/
  show_settings и др.),
  sf2 (sf2_file_list + 10 sampler-полей sf2_keys/drums_*),
  pitch_seq (7 полей seq), audio_reconnect (audio_disconnected/device_name/
  host_id/watch/_handle, trash_rx),
  core_state (patches, parts, active_part, config, ctrl_tx, sample_rate,
  note_state, pad_state, feedback_rx, cc_map, midi_learn_target, global_params,
  pickup_indicators, sustain_*, collapsed_categories, toasts, scope,
  split_point, show_fx_chain, dsp_load/peak, active_knob, config_dirty/
  last_config_save, keybinds, on_midi_reconnect, global_dirty),
  misc (остаток — целевой размер < 30 полей).
  Поле — в группу основного использования (grep максимум упоминаний).
  cfg-ПОЛЯ (review #5): 3 поля #[cfg(not(android))] + асимметричные
  cfg-ветки литерала (напр. android current_host: "AAudio", ~601) — атрибуты
  переносить VERBATIM, иначе host-билд зелёный, android-билд красный.
- Запрещено в этом плане: переименовывать поля, менять типы, вводить
  accessor-методы вместо прямых полей, трогать логику.

---

## Test Cases (from the requirement)

- TC1: полный билд + clippy после каждой группы (компилятор = корректность
  move).
- TC2: cargo test все gui pure-fn тесты зелёные.
- TC3: goldens байт-стабильны; rt_safety зелёный (GUI не в RT-зоне, гейт
  формальный).
- TC4: инвентарь-эквивалентность ИМЕННЫМИ СПИСКАМИ (review #7): comm
  отсортированных списков имён полей до/после — пусто (не только число:
  swap 1-в-1-вне компенсирует счётчик).
- TC5 (review #5b): init-порядок литерала меняется при разбиении (scan_sf2_files/
  list_drum_kits/from_config имеют side-effect логов) — ПРИНЯТЬ reordering
  (startup-логи), отметить в Task 0 инвентаре.

## Negative Matrix

- Поле используется в 2 подсистемах — идёт в основную (по максимуму
  упоминаний), вторая читает через self.group.x (не дублировать!).
- Поле-тип Arc<Mutex<..>> (file-pick) — переносится verbatim с типом.
- cfg-gated поле — атрибут verbatim, android-check в Task 13.
- init-выражение с side-effect — reordering принят (TC5).
- ~~~timeout/clock~~ — n/a; поле «ни в одну группу» — misc.

---

### Task 0: инвентарь

**Files:** Нет (отчёт в план-коммит или PR-описание).

- [ ] Step 1: инвентарь всех полей App (grep struct App блок) с назначением
  группы по рубрике; сохранить как таблицу в PR-описании (поле → группа).
  НЕ коммитить код.

### Tasks 1-12: по одной группе (порядок от малой к большой: led → audio_reconnect
→ pitch_seq → sf2 → browser → perf → midi_seq → transport → settings_audio →
drum_seq → looper → core_state+misc)

Каждый таск (шаблон, X = группа):

**Files:** Create `src/gui/app_structs.rs` (ОДИН файл для всех substruct —
решение фикс); Modify `src/gui/mod.rs` (struct App минус поля; литерал;
sites).

- [ ] Step 1: `src/gui/app_structs.rs`: struct XUi { поля verbatim с теми же
  vis }; `mod app_structs;` + `pub use` в gui/mod.rs.
- [ ] Step 2: App: удалить поля группы, добавить `pub x_ui: XUi`; литерал:
  `x_ui: XUi { ...init-выражения verbatim... }`.
- [ ] Step 3: `cargo build` → список E0609 → все `self.x` / `app.x` места
  заменить на `self.x_ui.x` (механически; util-файлы тоже).
- [ ] Step 4: gates: clippy -D warnings; cargo test; cargo test --test golden_fx.
- [ ] Step 5: коммит `refactor: App fields — extract <X>Ui substruct (techdebt S4-gui residue)`.

Особые случаи (реализатор):
- Поля, читаемые в util-файлах через `app.` (drums.rs, midi_seq.rs и т.д.) —
  правки по всему gui — компилятор перечислит.
- НЕ переносить в этом плане не-App поля (SynthEngine, config — отдельные
  структуры).

### Task 13: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens
  + ANDROID (review #5): `cargo check --target aarch64-linux-android`
  (android/build_rust.sh toolchain) — cfg-поля не ломают android-сборку.
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.
- [ ] Step 3: в PR — инвентарь Task 0 (поле → группа), число сайтов правок
  на группу.

## Out of scope
- Субструкты для SynthEngine/config — другие структуры, не App.
- accessor-рефакторинг (геттеры/сеттеры) — прямые поля сохраняются.
- Любое переименование/смена типа поля.
