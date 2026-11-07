# Stage 4a: S4-lib — разрез lib.rs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (база = master f1f4893, lib.rs 1090 строк; анкоры
собраны субагентом с HEAD; канон — Tavily 2026-11-08: behavior-preserving
split, mechanical extraction, small commits; аудитор entry-points 2026-11-07)

**Goal:** lib.rs 1090 → ~350 строк: init.rs (EngineHandles/CommonInit/
init_common/helpers), headless.rs (run_headless/parse_sf2_file), android_jni.rs
(statics/JNI/android_main), App::new в gui (литерал 207 полей уезжает из run_gui).
Compiler-enforced (ни App, ни CommonInit без Default; литерал исчерпывающий).

**Architecture:** Move-only сплит (никакой логики): секции → модули, pub use
ре-экспорты в lib.rs для `crate::EngineHandles` (synth/mod.rs:1765),
`crate::AMIDI_PORT`/`crate::AUDIO_PAUSED` (gui/settings.rs:304,308).
Проверка android-экспортов: `./android/build_rust.sh` в конце (no_mangle
символы выживают т.к. модуль декларирован в lib.rs без cfg... JNI-модуль
cfg(android) — экспорты живут в cdylib-сборке). Тесты ссылаются только на
7 корневых pub mod — сплит невидим тестам (verified аудитором).

**Tech Stack:** Rust; гейты: clippy -D warnings + cargo test + rt_safety +
`cargo check --lib --no-default-features` + `./android/build_rust.sh` (один раз
в Task 4, ~5-15 мин).

**Проверенные факты (HEAD f1f4893; не пересматривать):**
- Секции lib.rs: (a) decls+allocator 1-28; imports 30-54 (OnceLock android-gated
  :34-35); (b) android statics 37-46 (AMIDI_PORT БЕЗ гейта! AUDIO_PAUSED/
  JNI_MIDI_TX/JNI_NOTE_STATE/JNI_PAD_STATE — android-gated); (c) helpers 56-78
  (suppress_alsa_errors linux-gated, find_host_idx not(android), find_midi_port
  ungated); (d) EngineHandles 80-116; (e) CommonInit 118-155 (private, 25
  полей); (f) init_common 157-340 `pub(crate) fn init_common() ->
  Result<CommonInit>`; (g) desktop_main 342-363; (h) run_headless 365-607 (БЕЗ
  cfg — компилится на android тоже!); (i) parse_sf2_file 609-616; (j)
  make_piano_icon 618-650 (gui+not(android)); (k) run_gui 652-969; (l)
  android_main 974-1005 + 4 JNI :1009-1090 (все android-gated no_mangle).
- Cross-refs: run_headless НЕ использует gui; init_common НЕ ссылается на App;
  statics: писатели в init_common (:207-209,237,244), читатели JNI-фн + settings.
- App-литерал: gui/mod.rs — run_gui:721-927 (207 полей), источники: c.*,
  locals (on_midi_reconnect, layer_a, layer_b, parts_extra :709-719), gui
  конструкторы (PartState::new_empty, LedConsole::default, NoteFlashState::new,
  Keybinds::from_config, scan_sf2_files), preset (list_drum_kits,
  list_performances), synth константы, ReconnectPolicy::default. cfg-гварды в
  литерале: current_host 2 варианта (:728-731), available_hosts/supported_sr/
  selected_host_idx not(android).
- Тесты: только mini_midi_synth::{audio,cc_map,config,midi,preset,synth,trash}.
- Android: `cargo ndk -t arm64-v8a -P 30 --link-libcxx-shared -o
  android/app/src/main/jniLibs build --release --no-default-features --features
  gui,android-app`; android_main → run_gui(:1002).
- EngineHandles: конструируется synth/mod.rs:1765 (`crate::EngineHandles`).

---

## Test Cases

- TC1: после КАЖДОЙ задачи — все гейты зелёные (desktop); функции не менялись
  (git diff = чистые move-хунки: всё содержимое идентично, меняются только
  заголовки `pub fn` → `pub(crate) fn`/`pub fn` и mod-декларации).
- TC2: crate-пути живы: `crate::EngineHandles`, `crate::AMIDI_PORT`,
  `crate::AUDIO_PAUSED` резолвятся (компиляция = proof).
- TC3 (Task 4): `./android/build_rust.sh` завершается ok; JNI-символы в .so
  (nm/llvm-nm проверить 4 Java_* + android_main) — или хотя бы успешная сборка
  + наличие android/symbols/arm64-v8a/*.so свежее.
- TC4: gui-less сборка (`cargo check --lib --no-default-features`) зелёная
  после каждой задачи (CommonInit/init_common с их cfg-гейтами уезжают целиком).

## Negative Matrix

- Порядок задач: init.rs ПЕРВЫЙ (от него зависят headless/android через
  CommonInit/statics... на самом деле statics уезжают в android_jni, а
  init_common их ПИШЕТ → init.rs должен их видеть: `use
  crate::android_jni::{AUDIO_PAUSED, ...}` — цикличность модулей в Rust ок
  (один crate). Порядок: A(init) → B(headless) → C(android_jni) → D(App::new)
  — каждый компилируем.
- JNI no_mangle в дочернем модуле cfg(android) — экспорт жив, если модуль
  компилится в cdylib (декларация `#[cfg(target_os = "android")] mod
  android_jni;` в lib.rs). Проверка сборкой android (TC3), не рассуждением.
- AMIDI_PORT без гейта → уезжает в init.rs (там init_common пишет);
  android statics → android_jni.rs с их гейтами. AUDIO_PAUSED читается
  settings.rs → ре-экспорт из lib.rs обязателен.
- run_headless БЕЗ cfg → в headless.rs тоже без cfg (не менять видимость!).
- App::new: cfg-гварды внутри литерала переезжают 1:1; параметры
  (c: CommonInit, locals) — сигнатура покрывает все 5 источников; CommonInit
  становится pub(crate) (из init.rs), поле handles уже pub.
- Дубликат декларации static (lib.rs оставил + модуль создал) — компилятор
  поймает (ambiguous) — после каждого move из lib.rs УДАЛЯТЬ оригинал.
- ~~timeout/clock~~ — n/a (build-only стадия).

---

### Task A: init.rs

**Files:** Create `src/init.rs`; Modify `src/lib.rs` (удалить секции d/e/f/c +
их содержимое, добавить `mod init;` + ре-экспорты), `src/init.rs`
(pub(crate) CommonInit; pub items).

- [ ] Step 1: `src/init.rs` = секции (c)+(d)+(e)+(f) lib.rs verbatim; видимость:
  `pub(crate) struct CommonInit`, `pub(crate) fn init_common()`, helpers
  `pub(crate) fn find_host_idx/find_midi_port/suppress_alsa_errors` (или
  pub(super)... инициаторы — desktop_main в lib.rs и headless.rs → pub(crate)),
  EngineHandles остаётся `pub struct`.
- [ ] Step 2: lib.rs: `mod init;` + `pub use init::{EngineHandles, AMIDI_PORT};`
  + импорты секции (30-54 частично) — разнести по модулям (OnceLock android
  import → android_jni). Удалить moved-секции из lib.rs.
- [ ] Step 3: гейт: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo check --lib --no-default-features 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
- [ ] Step 4: коммит `refactor: extract init.rs — EngineHandles/CommonInit/init_common/helpers (S4-lib a)`.

### Task B: headless.rs

**Files:** Create `src/headless.rs`; Modify lib.rs (удалить (h)+(i), `mod headless;`), desktop_main вызов `run_headless(...)` → `headless::run_headless(...)` (или pub use).
- [ ] Step 1: headless.rs = run_headless + parse_sf2_file verbatim
  (pub(crate) fn — desktop_main вызывает). ВНУТРИ: замены `crate::init_common()`,
  `crate::find_midi_port()` → пути init.
- [ ] Step 2: гейт + коммит `refactor: extract headless.rs — run_headless/parse_sf2_file (S4-lib b)`.

### Task C: android_jni.rs

**Files:** Create `src/android_jni.rs`; Modify lib.rs (удалить (b android-часть:
4 gated statics; AMIDI_PORT уже в init) + (l), `#[cfg(target_os="android")]
mod android_jni;`), gui/settings.rs — crate::AUDIO_PAUSED резолвится через
`pub use android_jni::AUDIO_PAUSED;` в lib.rs.
- [ ] Step 1: android_jni.rs = 4 android statics + android_main + 4 JNI fns
  verbatim; внутри ссылки crate::init/common по новым путям.
- [ ] Step 2: lib.rs: gated mod-decl + `pub use android_jni::AUDIO_PAUSED;`
  (gated? settings.rs читает в android-ветке — ре-экспорт с тем же cfg).
- [ ] Step 3: гейт desktop + коммит `refactor: extract android_jni.rs — statics/JNI/android_main (S4-lib c)`. Android-сборка — Task 4.

### Task D: App::new

**Files:** Modify `src/gui/mod.rs` (impl App:: pub(crate) fn new(...)), `src/lib.rs` (run_gui: литерал 721-927 → `let mut app = gui::App::new(...);`).
- [ ] Step 1: сигнатура `pub(crate) fn new(c: crate::init::CommonInit,
  on_midi_reconnect: ..., layer_a: ..., layer_b: ..., parts_extra: ...) -> Self`
  (типы locals списать с run_gui :709-719); тело = литерал 1:1 (включая
  cfg-гварды); CommonInit делается pub(crate) в init.rs.
- [ ] Step 2: run_gui:locals остаются, литерал заменяется вызовом. Литерал
  uses gui-внутренние конструкторы — в gui/mod.rs доступны напрямую.
- [ ] Step 3: гейт + коммит `refactor: gui::App::new — 207-field literal out of run_gui (S4-lib d)`.

### Task 4: Верификация стадии

- [ ] Step 1: полный desktop гейт (все 5 команд).
- [ ] Step 2: `./android/build_rust.sh` — ok; .so свежий; `llvm-nm`/`nm -D`
  android/symbols/arm64-v8a/*.so | grep Java_ — 4 символа + android_main.
  СБОЙ → откат Task C только (android_jni.rs), остальные задачи остаются.
- [ ] Step 3: `wc -l src/lib.rs` — цель <400; PR → squash → rebuild release →
  launch-check → TECHDEBT + roadmap чекбокс.

## Out of scope

- gui/synth/drum сплиты — день (отдельные планы).
- App sub-structs (168 полей → группы) — S4-gui, день.
- Удаление `run_headless` на android (dead там) — отдельно, поведение.
