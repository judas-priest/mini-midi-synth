# Audit Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Устранить все дефекты, найденные в аудите от 2026-08-23, и добавить недостающую инженерную инфраструктуру, не меняя пользовательские сценарии работы синтезатора.

**Architecture:** Работа идёт снизу вверх. Сначала чинится сборка и структура крейта — без этого `clippy -D warnings` бесполезен, а CI нечего запускать. Затем real-time safety: вводится диагностический аллокатор `assert_no_alloc`, который превращает «free в аудиопотоке» из теории в падающий тест, и очередь отложенного освобождения (`Trash`), через которую аудиопоток отдаёт heap-объекты GUI-потоку. Дальше — функциональные баги, Android-релиз, тесты, наблюдаемость, документация.

**Tech Stack:** Rust 1.96.0, cpal (vendored), eframe/egui 0.31, rtrb (SPSC-кольца), rustysynth, midly, wmidi, jni, Gradle/AGP для Android, GitHub Actions для CI.

**Исходный аудит:** дефекты пронумерованы 1-19 (блокеры и баги) и разделы «чего не хватает». Каждая задача ниже ссылается на номер пункта аудита.

---

## Принятые решения (карт-бланш от пользователя)

Эти вопросы обычно требуют решения владельца проекта. Пользователь дал карт-бланш, решения приняты так:

| Вопрос | Решение | Почему |
|---|---|---|
| Как чинить `vendor/cpal` | Закоммитить в репозиторий (668K, 39 `.rs`-файлов) | Правка ровно 2 строки в `src/host/oboe/mod.rs:217-218`; форк на GitHub добавляет внешнюю зависимость от живости чужого хостинга |
| Лицензия проекта | MIT | Совместима с вендоренными rustysynth (MIT), mi-plaits-dsp (MIT) и cpal (Apache-2.0) |
| Версия тулчейна | Пин на 1.96.0 в `rust-toolchain.toml` | Текущая локальная версия; сборка становится воспроизводимой |
| Android release keystore | Задача создаёт конфигурацию, читающую путь и пароли из `~/.gradle/gradle.properties`; сам keystore не создаётся и не коммитится | Приватный ключ не должен попасть в git ни при каких условиях |
| Крупные фичи (аудиовход, MIDI out, CLAP-плагин, микротюнинг, экспорт WAV) | В этот план **не входят** | Каждая — отдельная подсистема на недели работы; см. раздел «Отдельные планы» в конце |

---

## File Structure

**Создаются:**

| Файл | Ответственность |
|---|---|
| `src/lib.rs` | Объявление модулей, `init_common`, `run_gui`, `run_headless`, Android entry points, JNI-функции. Всё, что сейчас в `src/main.rs` |
| `src/trash.rs` | Очередь отложенного освобождения: `Trash`, `TrashTx`, `TrashRx` |
| `rust-toolchain.toml` | Пин версии тулчейна и компонентов |
| `rustfmt.toml` | Конфигурация форматирования |
| `deny.toml` | Политика лицензий и уязвимостей для cargo-deny |
| `.github/workflows/ci.yml` | fmt + clippy + test + сборка Android |
| `LICENSE` | MIT |
| `CHANGELOG.md` | Keep a Changelog |
| `CLAUDE.md` | Контекст проекта для агентов |
| `docs/ARCHITECTURE.md` | Потоки, инварианты, путь события от MIDI до семпла |
| `tests/preset_roundtrip.rs` | Интеграционные тесты сериализации пресетов |
| `tests/midi_parse.rs` | Интеграционные тесты разбора MIDI |
| `fuzz/fuzz_targets/parse_wt.rs` | Fuzz-таргет для парсера wavetable |
| `fuzz/fuzz_targets/import_midi_drums.rs` | Fuzz-таргет для импорта MIDI-барабанов |
| `benches/dsp.rs` | Бенчмарки `tick_block` |
| `src/audio/metrics.rs` | Счётчики xrun, времени callback, переполнений колец |

**Модифицируются:**

| Файл | Что меняется |
|---|---|
| `Cargo.toml` | `[lib] path`, `crate-type`, фича `rt-assert`, dev-зависимости, `license`, профиль release |
| `src/main.rs` | Сжимается до вызова `mini_midi_synth::desktop_main()` |
| `src/audio.rs` | Дренаж при паузе, `assert_no_alloc`, отправка мусора в `Trash`, метрики |
| `src/amidi.rs` | Защита от use-after-free при закрытии портов |
| `src/synth/mod.rs` | `handle_control` возвращает владение heap-данными |
| `src/synth/looper.rs` | Преаллокация вектора отображения |
| `src/midi.rs` | Пропуск CC 120-127, счётчик потерянных событий |
| `src/config.rs` | Атомарная запись, сохранение битого конфига в `.bak` |
| `src/preset.rs` | Санитизация имён файлов, атомарная запись |
| `src/gui/settings.rs` | Полное восстановление состояния после reconnect |
| `src/gui/mod.rs` | Индикаторы метрик, SAF-пикер SF2 на Android |
| `android/app/build.gradle.kts` | Release signing, versionCode из git |
| `android/app/src/main/java/com/minimidisynth/SynthActivity.java` | Остановка BLE-реконнекта, дедупликация устройств |
| `android/app/src/main/AndroidManifest.xml` | `allowBackup`, `maxSdkVersion` для legacy-разрешений |
| `.gitignore` | Убрать `vendor/cpal`, добавить `android/build/` |

---

## Phase 0 — Сборка из чистого клона

### Task 1: Вернуть vendor/cpal в репозиторий

Аудит, пункт 1. `.gitignore:6` исключает `vendor/cpal`, но `Cargo.toml:63` требует его через `[patch.crates-io]`. Клон не собирается.

**Files:**
- Modify: `.gitignore`
- Add: `vendor/cpal/**`

- [ ] **Step 1: Убедиться, что проблема реальна**

```bash
cd /home/dima/Projects/mini_midi_synth
git ls-files vendor/cpal | wc -l
```

Ожидается: `0` — ни одного файла cpal не отслеживается.

- [ ] **Step 2: Убрать строку из .gitignore**

Удалить строку `vendor/cpal` из `.gitignore`. Итоговый файл:

```
/target
android/local.properties
android/app/src/main/jniLibs/
android/app/build/
android/build/
android/.gradle/
android/.claude_session
```

Обрати внимание: одновременно добавлены `android/build/` и `android/.claude_session` — это пункт 18 аудита (артефакты сборки в git).

- [ ] **Step 3: Убрать из индекса ранее закоммиченный артефакт сборки**

```bash
git rm --cached android/build/reports/problems/problems-report.html
```

Ожидается: `rm 'android/build/reports/problems/problems-report.html'`

- [ ] **Step 4: Добавить vendor/cpal**

У `vendor/cpal` есть собственный `.gitignore` (унаследован от апстрима), поэтому нужен `-f`:

```bash
git add -f vendor/cpal
git status --short vendor/cpal | wc -l
```

Ожидается: число больше 40 (39 `.rs` + Cargo.toml, LICENSE, README и т.д.)

- [ ] **Step 5: Проверить, что правка на месте**

```bash
grep -n "LowLatency\|Exclusive" vendor/cpal/src/host/oboe/mod.rs
```

Ожидается:
```
217:        .set_performance_mode(oboe::PerformanceMode::LowLatency)
218:        .set_sharing_mode(oboe::SharingMode::Exclusive);
```

Если строк нет — vendor/cpal не тот, что нужен, останови выполнение и сообщи.

- [ ] **Step 6: Проверить сборку из чистого клона**

```bash
cd /tmp && rm -rf mms-clone-test && git clone --depth 1 file:///home/dima/Projects/mini_midi_synth mms-clone-test 2>&1 | tail -1
```

Клон делается из рабочей копии, поэтому сначала нужен коммит. Выполни этот шаг после Step 7.

- [ ] **Step 7: Commit**

```bash
git add .gitignore
git commit -m "fix: track vendored cpal so a fresh clone builds

vendor/cpal was gitignored while Cargo.toml patches cpal to that path,
so any clone failed at cargo build. The vendored copy differs from
upstream 0.15.3 by two lines in src/host/oboe/mod.rs that request
AAudio LowLatency + Exclusive mode.

Also stops tracking android/build artifacts."
```

- [ ] **Step 8: Теперь выполнить проверку клона**

```bash
cd /tmp && rm -rf mms-clone-test && git clone --depth 1 file:///home/dima/Projects/mini_midi_synth mms-clone-test && cd mms-clone-test && cargo build 2>&1 | tail -5
```

Ожидается: `Finished \`dev\` profile`. Если ошибка `failed to read .../vendor/cpal/Cargo.toml` — файлы не добавились, вернись к Step 4.

```bash
cd /tmp && rm -rf mms-clone-test
```

---

### Task 2: Зафиксировать версию тулчейна

Раздел «чего не хватает», инфраструктура. Без пина версии сборка невоспроизводима: clippy разных версий даёт разные наборы лintов.

**Files:**
- Create: `rust-toolchain.toml`

- [ ] **Step 1: Создать файл**

```toml
[toolchain]
channel = "1.96.0"
components = ["rustfmt", "clippy"]
targets = ["aarch64-linux-android"]
```

- [ ] **Step 2: Проверить, что тулчейн подхватывается**

```bash
cd /home/dima/Projects/mini_midi_synth && rustc --version
```

Ожидается: `rustc 1.96.0 (ac68faa20 2026-05-25)`

Если rustup начнёт скачивать тулчейн — это нормально, дождись окончания.

- [ ] **Step 3: Commit**

```bash
git add rust-toolchain.toml
git commit -m "build: pin toolchain to 1.96.0 with rustfmt, clippy, android target"
```

---

## Phase 1 — Структура крейта

### Task 3: Разделить lib и bin

Аудит, пункт 14. Сейчас `[lib] path = "src/main.rs"` и `[[bin]] path = "src/main.rs"` — одно и то же дерево компилируется дважды, тесты гоняются дважды (90 с вместо 45), и 520 из 525 предупреждений clippy — это `dead_code` от lib-цели, где ничего не используется. Полезные предупреждения тонут в шуме, поэтому `-D warnings` в CI невозможен.

**Files:**
- Create: `src/lib.rs`
- Modify: `src/main.rs`
- Modify: `Cargo.toml:6-13`

- [ ] **Step 1: Зафиксировать текущее состояние как эталон**

```bash
cd /home/dima/Projects/mini_midi_synth
cargo test 2>&1 | grep "test result" 
```

Ожидается две строки `test result: ok. 26 passed` (lib и bin — это и есть дублирование).

Запиши число тестов: после рефакторинга должна остаться **одна** строка с тем же числом 26.

- [ ] **Step 2: Переместить main.rs в lib.rs**

```bash
git mv src/main.rs src/lib.rs
```

- [ ] **Step 3: Сделать точки входа публичными в lib.rs**

В `src/lib.rs` изменить три сигнатуры. Было:

```rust
fn init_common() -> Result<CommonInit> {
```

Стало:

```rust
pub(crate) fn init_common() -> Result<CommonInit> {
```

Было:

```rust
fn run_headless() -> Result<()> {
```

Стало:

```rust
pub fn run_headless() -> Result<()> {
```

Было (строка с `#[cfg(feature = "gui")]` перед ней сохраняется):

```rust
fn run_gui(
```

Стало:

```rust
pub fn run_gui(
```

- [ ] **Step 4: Заменить `fn main` в lib.rs на `desktop_main`**

В `src/lib.rs` найти блок:

```rust
// On Android the real entry point is `android_main` (see bottom of file).
// This dummy keeps the bin target compilable for `cargo ndk build`.
#[cfg(target_os = "android")]
fn main() {}

#[cfg(not(target_os = "android"))]
fn main() -> Result<()> {
    #[cfg(target_os = "linux")]
    suppress_alsa_errors();

    #[cfg(feature = "gui")]
    {
        let headless = std::env::args().any(|a| a == "--headless" || a == "-H");
        if headless {
            run_headless()
        } else {
            run_gui()
        }
    }

    #[cfg(not(feature = "gui"))]
    run_headless()
}
```

Заменить на:

```rust
/// Desktop entry point. The `mini_midi_synth` binary calls this; on Android
/// the entry point is `android_main` instead (see bottom of file).
#[cfg(not(target_os = "android"))]
pub fn desktop_main() -> Result<()> {
    #[cfg(target_os = "linux")]
    suppress_alsa_errors();

    #[cfg(feature = "gui")]
    {
        let headless = std::env::args().any(|a| a == "--headless" || a == "-H");
        if headless {
            run_headless()
        } else {
            run_gui()
        }
    }

    #[cfg(not(feature = "gui"))]
    run_headless()
}
```

Пустой `fn main() {}` для Android удаляется целиком — bin-цель больше не собирается под Android (см. Step 6).

- [ ] **Step 5: Создать тонкий main.rs**

Записать в `src/main.rs` ровно это:

```rust
//! Desktop binary. All logic lives in the library crate so that the Android
//! cdylib and the desktop executable share exactly one code path.

fn main() -> anyhow::Result<()> {
    mini_midi_synth::desktop_main()
}
```

- [ ] **Step 6: Обновить Cargo.toml**

Заменить блоки `[lib]` и `[[bin]]`:

```toml
[lib]
name = "mini_midi_synth"
path = "src/lib.rs"
crate-type = ["rlib", "cdylib"]

[[bin]]
name = "mini_midi_synth"
path = "src/main.rs"
required-features = ["desktop"]
```

`rlib` нужен, чтобы бинарь мог линковаться с библиотекой; `cdylib` — чтобы Android получил `.so`.

- [ ] **Step 7: Собрать и посчитать предупреждения**

```bash
cargo clippy --all-targets --message-format=json 2>/dev/null | grep -oP '"code":\{"code":"[^"]+' | sed 's/.*"//' | sort | uniq -c | sort -rn
```

До рефакторинга было `520 dead_code`. Ожидается резкое падение — единицы предупреждений. Если `dead_code` всё ещё сотни, значит `pub` расставлен не там: библиотека экспортирует `run_gui`/`run_headless`, а всё, до чего от них дотянуться нельзя, честно считается мёртвым. Проверь, что модули в `lib.rs` объявлены как раньше (`mod audio;`, `mod synth;` и т.д.) — их приватность правильная.

- [ ] **Step 8: Проверить, что тесты больше не дублируются**

```bash
cargo test 2>&1 | grep "test result"
```

Ожидается **одна** строка `test result: ok. 26 passed; 0 failed`. Время должно упасть примерно вдвое.

- [ ] **Step 9: Проверить, что бинарь работает**

```bash
cargo build --release 2>&1 | tail -2 && ls -la target/release/mini_midi_synth
```

Ожидается собранный исполняемый файл.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs
git commit -m "refactor: split lib and bin targets

src/main.rs served as both [lib] path and [[bin]] path, so the whole
tree compiled twice, the test suite ran twice, and 520 of 525 clippy
warnings were phantom dead_code from the lib target. Logic now lives in
src/lib.rs; src/main.rs is a three-line shim calling desktop_main()."
```

---

### Task 4: Убрать оставшиеся предупреждения clippy

Аудит, пункт 14 (хвост). После Task 3 остаются настоящие предупреждения: `too_many_arguments` в `audio.rs:86`, `unused_mut`, мёртвый `AmidiPort::close` на не-Android платформах.

**Files:**
- Modify: `src/audio.rs:86-95`
- Modify: `src/amidi.rs:174-179`

- [ ] **Step 1: Посмотреть, что осталось**

```bash
cargo clippy --all-targets 2>&1 | grep -A3 "^warning" | head -60
```

Выпиши список. Дальнейшие шаги чинят те, что известны на момент написания плана; если появятся другие — чини их тем же принципом (не глушить `#[allow]`, а устранять причину).

- [ ] **Step 2: Сгруппировать аргументы AudioBackend::new в структуру**

В `src/audio.rs` перед `impl AudioBackend` добавить:

```rust
/// Everything the audio callback needs to share with the rest of the app.
/// Grouped into a struct because passing eight positional arguments is both
/// a clippy lint and an easy place to swap two `Arc<AtomicBool>` by mistake.
pub struct AudioShared {
    pub amidi_port: std::sync::Arc<crate::amidi::AmidiPort>,
    pub note_state: crate::midi::NoteState,
    pub pad_state: crate::midi::PadState,
    pub paused: Arc<AtomicBool>,
}
```

Заменить сигнатуру:

```rust
    pub fn new(
        config: AudioConfig,
        mut synth: SynthEngine,
        mut midi_rx: Consumer<MidiEvent>,
        mut ctrl_rx: Consumer<ControlEvent>,
        shared: AudioShared,
    ) -> Result<(Self, u32)> {
        let AudioShared { amidi_port, note_state, pad_state, paused } = shared;
```

Остальное тело функции не меняется — имена переменных те же.

- [ ] **Step 3: Обновить два места вызова**

В `src/lib.rs` (бывший main.rs, функция `init_common`) было:

```rust
    let (audio_backend, actual_sr) =
        audio::AudioBackend::new(audio_config, engine, midi_rx, ctrl_rx,
            amidi_port, note_state.clone(), pad_state.clone(), audio_paused)?;
```

Стало:

```rust
    let (audio_backend, actual_sr) = audio::AudioBackend::new(
        audio_config,
        engine,
        midi_rx,
        ctrl_rx,
        audio::AudioShared {
            amidi_port,
            note_state: note_state.clone(),
            pad_state: pad_state.clone(),
            paused: audio_paused,
        },
    )?;
```

В `src/gui/settings.rs:236-238` было:

```rust
        let (audio_backend, actual_sr) =
            crate::audio::AudioBackend::new(audio_config, engine, midi_rx, ctrl_rx,
                amidi_port, self.note_state.clone(), self.pad_state.clone(), audio_paused)?;
```

Стало:

```rust
        let (audio_backend, actual_sr) = crate::audio::AudioBackend::new(
            audio_config,
            engine,
            midi_rx,
            ctrl_rx,
            crate::audio::AudioShared {
                amidi_port,
                note_state: self.note_state.clone(),
                pad_state: self.pad_state.clone(),
                paused: audio_paused,
            },
        )?;
```

- [ ] **Step 4: Убрать мёртвый AmidiPort::close на не-Android**

В `src/amidi.rs` заглушка для не-Android содержит `pub fn close(&self) {}`, который нигде не вызывается вне Android. Вызов есть только в `src/lib.rs` под `#[cfg(target_os = "android")]`. Пометить заглушку так, чтобы сигнатура сохранилась для симметрии, но линт молчал:

```rust
#[cfg(not(target_os = "android"))]
impl AmidiPort {
    pub const fn new() -> Self { Self }
    #[inline]
    pub fn receive(&self, _buf: &mut [u8]) -> Option<usize> { None }
    /// Mirrors the Android implementation so call sites compile on every
    /// platform; there is nothing to close when there is no NDK MIDI device.
    #[allow(dead_code)]
    pub fn close(&self) {}
}
```

- [ ] **Step 5: Проверить, что предупреждений нет**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Ожидается: `Finished` без единого `warning`. Если что-то осталось — почини по существу.

- [ ] **Step 6: Тесты всё ещё зелёные**

```bash
cargo test 2>&1 | grep "test result"
```

Ожидается: `test result: ok. 26 passed; 0 failed`

- [ ] **Step 7: Commit**

```bash
git add src/audio.rs src/amidi.rs src/lib.rs src/gui/settings.rs
git commit -m "refactor: clear remaining clippy warnings

Groups AudioBackend::new's eight positional arguments into AudioShared —
the lint was right, two Arc<AtomicBool> parameters next to each other are
easy to swap by accident."
```

---

### Task 5: Настроить rustfmt и запретить регресс

**Files:**
- Create: `rustfmt.toml`

- [ ] **Step 1: Создать конфигурацию**

Кодовая база написана в компактном стиле с многими объявлениями в строку (`src/synth/patch_params.rs`). Полный `cargo fmt` переписал бы тысячи строк и похоронил историю в `git blame`. Поэтому конфигурация консервативная — она фиксирует то, что уже есть:

```toml
edition = "2021"
max_width = 120
use_small_heuristics = "Max"
```

- [ ] **Step 2: Посмотреть масштаб расхождения**

```bash
cargo fmt --check 2>&1 | grep -c "^Diff" || echo "0 diffs"
```

Запиши число. Если оно больше 200 — переформатирование всей базы отдельным коммитом того стоит, но **только отдельным коммитом**, чтобы не смешивать с содержательными правками.

- [ ] **Step 3: Переформатировать всё одним коммитом**

```bash
cargo fmt
cargo test 2>&1 | grep "test result"
```

Ожидается: `test result: ok. 26 passed` — форматирование не меняет поведение.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "style: apply rustfmt with project config

Pure formatting, no behavior change. Separate commit so git blame on
subsequent functional changes stays readable."
```

---

## Phase 2 — CI и проверки зависимостей

### Task 6: GitHub Actions

Аудит, пункт 13.

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Создать workflow**

```yaml
name: CI

on:
  push:
    branches: [master, android-polish]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install ALSA and JACK headers
        run: |
          sudo apt-get update
          sudo apt-get install -y libasound2-dev libjack-jackd2-dev

      - uses: Swatinem/rust-cache@v2

      - name: Formatting
        run: cargo fmt --check

      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings

      - name: Tests
        run: cargo test

      - name: Headless build without GUI
        run: cargo build --no-default-features --features desktop

  android:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-java@v4
        with:
          distribution: temurin
          java-version: '17'

      - uses: Swatinem/rust-cache@v2

      - name: Install cargo-ndk
        run: cargo install cargo-ndk --locked

      - name: Build native library
        run: |
          cargo ndk -t arm64-v8a -P 30 --link-libcxx-shared \
            -o android/app/src/main/jniLibs \
            build --release --no-default-features --features gui,android-app

      - name: Build APK
        working-directory: android
        run: ./gradlew assembleDebug --no-daemon

      - uses: actions/upload-artifact@v4
        with:
          name: debug-apk
          path: android/app/build/outputs/apk/debug/*.apk
```

- [ ] **Step 2: Проверить, что headless-сборка действительно проходит локально**

CI проверяет конфигурацию `--no-default-features --features desktop`, которая давно не собиралась. Проверь до пуша:

```bash
cargo build --no-default-features --features desktop 2>&1 | tail -20
```

Ожидается: `Finished`. Если появились ошибки компиляции — почини их сейчас, они настоящие: это тот самый режим, который включается флагом `--headless`.

- [ ] **Step 3: Проверить синтаксис YAML**

```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/ci.yml')); print('ok')"
```

Ожидается: `ok`

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add fmt, clippy, test and Android APK build"
```

---

### Task 7: Проверка зависимостей на уязвимости и лицензии

**Files:**
- Create: `deny.toml`
- Modify: `.github/workflows/ci.yml`
- Create: `LICENSE`
- Modify: `Cargo.toml`

- [ ] **Step 1: Добавить LICENSE (аудит, пункт 16)**

Записать в `LICENSE` текст MIT со строкой копирайта `Copyright (c) 2026 Dmitry`. Полный канонический текст лицензии MIT — от `Permission is hereby granted, free of charge...` до `...DEALINGS IN THE SOFTWARE.`

- [ ] **Step 2: Указать лицензию в манифесте**

В `Cargo.toml` в секцию `[package]` добавить строку после `edition`:

```toml
license = "MIT"
```

- [ ] **Step 3: Создать deny.toml**

```toml
[advisories]
version = 2
yanked = "warn"

[licenses]
version = 2
allow = [
    "MIT",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Zlib",
    "MPL-2.0",
]

[bans]
multiple-versions = "warn"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

- [ ] **Step 4: Запустить локально и посмотреть, что скажет**

```bash
cargo install cargo-deny --locked 2>&1 | tail -2
cargo deny check 2>&1 | tail -40
```

Если какая-то транзитивная зависимость под лицензией не из списка — добавь её в `allow`, если лицензия permissive, или зафиксируй исключение с комментарием почему. Если найдены advisories — выпиши их, это результат аудита зависимостей, которого раньше не было.

- [ ] **Step 5: Добавить job в CI**

В `.github/workflows/ci.yml` добавить job:

```yaml
  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check
```

- [ ] **Step 6: Commit**

```bash
git add LICENSE Cargo.toml deny.toml .github/workflows/ci.yml
git commit -m "chore: add MIT license and cargo-deny supply chain checks"
```

---

## Phase 3 — Real-time safety

### Task 8: Диагностический аллокатор

Раздел «чего не хватает», механизмы. Это первая задача фазы намеренно: она превращает следующие три задачи из «надо поверить на слово» в падающий тест.

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/lib.rs`
- Modify: `src/audio.rs`

- [ ] **Step 1: Добавить зависимость и фичу**

В `Cargo.toml` в `[features]`:

```toml
rt-assert = ["dep:assert_no_alloc"]
```

В `[dependencies]`:

```toml
assert_no_alloc = { version = "1.1", optional = true }
```

- [ ] **Step 2: Зарегистрировать аллокатор**

В `src/lib.rs` сразу после объявлений модулей добавить:

```rust
/// Diagnostic allocator: with `--features rt-assert`, any allocation or
/// deallocation inside the audio callback aborts the process instead of
/// silently costing an unbounded amount of time. Not enabled by default —
/// it adds a thread-local check to every allocation in the program.
#[cfg(feature = "rt-assert")]
#[global_allocator]
static ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;
```

- [ ] **Step 3: Обернуть тело аудио-callback**

В `src/audio.rs` добавить хелпер рядом с `soft_limit`:

```rust
/// Runs `f` under the no-allocation assertion when the `rt-assert` feature is
/// on, and unchanged otherwise. Keeps the callback body free of cfg noise.
#[inline(always)]
fn rt_guard<R>(f: impl FnOnce() -> R) -> R {
    #[cfg(feature = "rt-assert")]
    { assert_no_alloc::assert_no_alloc(f) }
    #[cfg(not(feature = "rt-assert"))]
    { f() }
}
```

В `device.build_output_stream` заменить внешнюю обёртку. Было:

```rust
                no_denormals::no_denormals(|| {
```

Стало:

```rust
                rt_guard(|| no_denormals::no_denormals(|| {
```

и соответствующее закрытие в конце тела callback. Было:

```rust
                });
            },
```

Стало:

```rust
                }));
            },
```

- [ ] **Step 4: Написать тест, который ловит нарушение**

Создать `tests/rt_safety.rs`:

```rust
//! Verifies that processing a control event does not allocate or free memory.
//! Run with: cargo test --features rt-assert --test rt_safety
#![cfg(feature = "rt-assert")]

use assert_no_alloc::assert_no_alloc;
use mini_midi_synth::synth::{ControlEvent, SynthEngine, BLOCK_SIZE};

#[global_allocator]
static A: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[test]
fn tick_block_does_not_allocate() {
    let mut engine = SynthEngine::new(48_000.0);
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];

    engine.handle_event(mini_midi_synth::synth::MidiEvent::NoteOn {
        channel: 0,
        note: 60,
        velocity: 100,
    });

    assert_no_alloc(|| {
        for _ in 0..64 {
            engine.tick_block(&mut l, &mut r);
        }
    });
}

#[test]
fn loading_a_patch_does_not_free_on_the_audio_thread() {
    let patches = mini_midi_synth::preset::load_all_patches();
    let patch = patches.first().expect("factory presets are compiled in");
    let mut engine = SynthEngine::new(48_000.0);
    engine.set_patches(patches.clone());

    let event = ControlEvent::load_patch_from(0, patch);

    // Before Task 9 this aborts: handle_control takes ControlEvent by value
    // and drops the Box<LoadPatchEvent> right here.
    assert_no_alloc(|| {
        let _returned = engine.handle_control(event);
    });
}
```

Этот тест требует, чтобы `synth`, `preset` и их содержимое были публичны. Сделать в `src/lib.rs`:

```rust
pub mod preset;
pub mod synth;
```

вместо текущих `mod preset;` / `mod synth;` (с сохранением атрибутов `#[cfg_attr(...)]` над ними).

- [ ] **Step 5: Запустить тест и убедиться, что второй падает**

```bash
cargo test --features rt-assert --test rt_safety 2>&1 | tail -20
```

Ожидается: `tick_block_does_not_allocate ... ok`, а `loading_a_patch_does_not_free_on_the_audio_thread` **падает** с сообщением про запрещённую деаллокацию. Это подтверждение пункта 3 аудита. Если оба проходят — значит `handle_control` уже не дропает; перечитай `src/synth/mod.rs:1651`, скорее всего тест собран без фичи.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src/lib.rs src/audio.rs tests/rt_safety.rs
git commit -m "test: add rt-assert feature and a failing test for audio-thread frees

assert_no_alloc aborts on any (de)allocation inside the audio callback when
the feature is on. The second test currently fails on purpose: it documents
the Box<LoadPatchEvent> being dropped on the audio thread. Task 9 fixes it."
```

Коммит с падающим тестом делается сознательно — это TDD-шаг «красный».

---

### Task 9: Очередь отложенного освобождения

Аудит, пункт 3. `handle_control` принимает `ControlEvent` по значению и дропает heap-содержимое в аудиопотоке: `Box<LoadPatchEvent>` при каждой смене пресета, `Box<CcMap>`, `Box<[DrumPattern;8]>`, `Box<MidiSeqData>`, и — хуже всего — `Arc<SoundFont>` на десятки мегабайт при выгрузке или замене саундфонта.

**Files:**
- Create: `src/trash.rs`
- Modify: `src/lib.rs`
- Modify: `src/synth/mod.rs:1649`
- Modify: `src/audio.rs`
- Modify: `src/gui/mod.rs`

- [ ] **Step 1: Написать модуль очереди**

Создать `src/trash.rs`:

```rust
//! Deferred deallocation for the audio thread.
//!
//! The audio callback must not call free(). Anything heap-allocated that
//! reaches the engine — a boxed patch, a soundfont Arc, a drum kit — is handed
//! back through this SPSC queue and dropped by the GUI thread.
//!
//! If the queue is full the object is dropped on the spot: a rare glitch beats
//! an unbounded leak, and a full queue means the GUI has stalled for seconds.

use rtrb::{Consumer, Producer, RingBuffer};

use crate::synth::ControlEvent;

/// Heap-owning values that must not be freed on the audio thread.
pub enum Trash {
    Control(ControlEvent),
    SoundFont(std::sync::Arc<rustysynth::SoundFont>),
}

pub struct TrashTx(Producer<Trash>);
pub struct TrashRx(Consumer<Trash>);

/// Capacity is generous: patch changes come from a human turning a knob, and
/// the GUI drains this every frame.
const TRASH_CAPACITY: usize = 64;

pub fn channel() -> (TrashTx, TrashRx) {
    let (tx, rx) = RingBuffer::new(TRASH_CAPACITY);
    (TrashTx(tx), TrashRx(rx))
}

impl TrashTx {
    /// Hand a value over for deferred dropping. Real-time safe: the push
    /// itself never allocates. Falls back to dropping in place if the queue
    /// is full.
    #[inline]
    pub fn dispose(&mut self, item: Trash) {
        let _ = self.0.push(item);
    }
}

impl TrashRx {
    /// Drop everything the audio thread handed over. Call from the GUI thread.
    /// Returns how many items were freed, for the metrics display.
    pub fn drain(&mut self) -> usize {
        let mut n = 0;
        while self.0.pop().is_ok() {
            n += 1;
        }
        n
    }
}
```

- [ ] **Step 2: Объявить модуль**

В `src/lib.rs` рядом с остальными объявлениями:

```rust
pub mod trash;
```

- [ ] **Step 3: Изменить сигнатуру handle_control**

В `src/synth/mod.rs:1649` заменить:

```rust
    pub fn handle_control(&mut self, event: ControlEvent) {
        match event {
```

на:

```rust
    /// Applies a control event. Returns the event back when it owns heap data,
    /// so the caller can hand it to the trash queue instead of freeing it on
    /// the audio thread. Returns `None` for the plain-data variants.
    #[must_use = "heap-owning events must be sent to the trash queue"]
    pub fn handle_control(&mut self, mut event: ControlEvent) -> Option<ControlEvent> {
        match &mut event {
```

- [ ] **Step 4: Переписать ветки, владеющие heap-данными**

Ветка `LoadPatch` была:

```rust
            ControlEvent::LoadPatch(ev) => {
                let LoadPatchEvent { part, params, mod_matrix, mseg1, mseg2, pitch_seq, lfo_step_seq, wavetable } = *ev;
                if let Some(l) = self.parts.get_mut(part) {
```

Стала (значения вынимаются из-под `&mut` через `std::mem::take`, а коробка остаётся живой и возвращается):

```rust
            ControlEvent::LoadPatch(ev) => {
                let part = ev.part;
                let params = ev.params;
                let mod_matrix = std::mem::take(&mut ev.mod_matrix);
                let mseg1 = ev.mseg1.take();
                let mseg2 = ev.mseg2.take();
                let pitch_seq = ev.pitch_seq.take();
                let lfo_step_seq = ev.lfo_step_seq.take();
                let wavetable = ev.wavetable.take();
                if let Some(l) = self.parts.get_mut(part) {
```

`params` — `PatchParams`, он `Copy`? Проверь: если он не `Copy`, используй `std::mem::take(&mut ev.params)` и добавь `#[derive(Default)]` на `PatchParams` (у него уже есть `Default`, судя по `PatchParams::default()` в кодовой базе — проверь командой `grep -n "impl Default for PatchParams\|#\[derive.*Default.*\]" src/synth/patch_params.rs`).

Тело ветки дальше не меняется. В конце ветки, после `self.reset_preset_pickups();`, ничего добавлять не нужно — возврат делается один раз в конце функции (Step 6).

Ветка `SetCcMap` была:

```rust
            ControlEvent::SetCcMap { map } => { self.cc_map = *map; }
```

Стала:

```rust
            ControlEvent::SetCcMap { map } => {
                self.cc_map = std::mem::take(map.as_mut());
            }
```

Это требует `impl Default for CcMap`. Проверь наличие:

```bash
grep -n "impl Default for CcMap\|derive.*Default" src/cc_map.rs
```

Если `Default` нет — добавь `#[derive(Default)]` к `CcMap`, если все поля дефолтируемы, иначе напиши `impl Default` вручную с теми же значениями, что в `CcMap::new()`.

Ветка `DrumLoadKit` была:

```rust
            ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume } => {
                self.drum_engine.sequencer.patterns = *patterns;
                self.drum_engine.params = *params;
                self.drum_engine.sequencer.bpm = bpm;
                self.drum_engine.sequencer.swing = swing;
                self.drum_engine.volume = volume;
                self.sampler.drum_volume = volume;
            }
```

Стала:

```rust
            ControlEvent::DrumLoadKit { patterns, params, bpm, swing, volume } => {
                self.drum_engine.sequencer.patterns = **patterns;
                self.drum_engine.params = **params;
                self.drum_engine.sequencer.bpm = *bpm;
                self.drum_engine.sequencer.swing = *swing;
                self.drum_engine.volume = *volume;
                self.sampler.drum_volume = *volume;
            }
```

`DrumPattern` и `DrumSlotParams` — `Copy` (в `src/lib.rs` они копируются в массивы), поэтому разыменование копирует содержимое, а коробки уезжают в мусор.

- [ ] **Step 5: Починить оставшиеся ветки под match по ссылке**

`match &mut event` меняет тип связываемых переменных: там, где раньше было `part: usize`, теперь `part: &mut usize`. Компилятор перечислит все места. Механическое правило: в арифметике и сравнениях добавить `*`, при присваивании в поля движка — тоже `*`.

```bash
cargo build 2>&1 | grep -E "^error" | head -40
```

Чини по одной, пока список не опустеет. Ожидается порядка 40-60 мест — это механическая работа, поведение не меняется.

- [ ] **Step 6: Вернуть событие в конце функции**

В самом конце `handle_control`, после закрывающей скобки `match`, добавить:

```rust
        }

        // Only these variants own heap memory; everything else is plain data
        // that costs nothing to drop right here.
        match &event {
            ControlEvent::LoadPatch(_)
            | ControlEvent::SetCcMap { .. }
            | ControlEvent::DrumLoadKit { .. }
            | ControlEvent::MidiSeqLoad { .. }
            | ControlEvent::LoadKeysSoundFont { .. }
            | ControlEvent::LoadDrumsSoundFont { .. } => Some(event),
            _ => None,
        }
    }
```

- [ ] **Step 7: Отдавать старый саундфонт в мусор**

Ветки `LoadKeysSoundFont`, `LoadDrumsSoundFont`, `UnloadKeysSoundFont`, `UnloadDrumsSoundFont` заменяют `Arc<SoundFont>` внутри сэмплера, и старый Arc дропается в аудиопотоке. Нужно, чтобы сэмплер возвращал предыдущее значение.

Найди в `src/synth/sampler.rs` методы загрузки:

```bash
grep -n "fn load_keys\|fn load_drums\|fn unload\|soundfont" src/synth/sampler.rs | head -20
```

Для каждого метода, который присваивает поле типа `Option<Arc<SoundFont>>`, изменить возвращаемый тип на `Option<Arc<SoundFont>>` и вернуть `std::mem::replace(&mut self.поле, новое)`. Например, если метод выглядит так:

```rust
    pub fn load_keys_soundfont(&mut self, sf: Arc<SoundFont>) {
        self.keys_sf = Some(sf);
        // ... reinitialisation
    }
```

то станет:

```rust
    /// Installs a new soundfont and returns the previous one so the caller can
    /// dispose of it off the audio thread — dropping the last Arc to a 40 MB
    /// soundfont inside the callback is a guaranteed xrun.
    #[must_use = "the previous soundfont must be freed off the audio thread"]
    pub fn load_keys_soundfont(&mut self, sf: Arc<SoundFont>) -> Option<Arc<SoundFont>> {
        let old = self.keys_sf.replace(sf);
        // ... reinitialisation unchanged
        old
    }
```

В `handle_control` соответствующая ветка кладёт старый саундфонт в `self.trash`. Для этого движку нужен доступ к очереди: добавь в `SynthEngine` поле

```rust
    trash: Option<crate::trash::TrashTx>,
```

инициализируй `None` в `SynthEngine::new`, добавь сеттер рядом с `set_feedback_tx`:

```rust
    pub fn set_trash_tx(&mut self, tx: crate::trash::TrashTx) {
        self.trash = Some(tx);
    }
```

и в ветке загрузки:

```rust
            ControlEvent::LoadKeysSoundFont { soundfont } => {
                if let Some(old) = self.sampler.load_keys_soundfont(soundfont.clone()) {
                    if let Some(t) = self.trash.as_mut() {
                        t.dispose(crate::trash::Trash::SoundFont(old));
                    }
                }
            }
```

`soundfont.clone()` здесь — клон `Arc`, то есть инкремент счётчика, не копия данных; исходный Arc уезжает в мусор вместе с событием.

- [ ] **Step 8: Подключить очередь в аудио-callback**

В `src/audio.rs` в структуре `AudioShared` добавить поле:

```rust
    pub trash: crate::trash::TrashTx,
```

В `AudioBackend::new` после деструктуризации сделать очередь доступной callback'у: `let mut trash = trash;` и в теле цикла заменить

```rust
                    while let Ok(event) = ctrl_rx.pop() {
                        synth.handle_control(event);
                    }
```

на

```rust
                    while let Ok(event) = ctrl_rx.pop() {
                        if let Some(spent) = synth.handle_control(event) {
                            trash.dispose(crate::trash::Trash::Control(spent));
                        }
                    }
```

- [ ] **Step 9: Дренировать мусор в GUI**

В `src/lib.rs` в `init_common` создать канал и раздать концы:

```rust
    let (trash_tx, trash_rx) = trash::channel();
    engine.set_trash_tx(trash_tx.clone_for_engine());
```

Поскольку `Producer` не клонируется, сделай проще — два независимых канала: один для движка (саундфонты), один для callback'а (события). Обнови `src/trash.rs`, добавив, что каналов может быть несколько, и создай в `init_common`:

```rust
    let (engine_trash_tx, engine_trash_rx) = trash::channel();
    let (cb_trash_tx, cb_trash_rx) = trash::channel();
    engine.set_trash_tx(engine_trash_tx);
```

`cb_trash_tx` уходит в `AudioShared`, оба приёмника — в `CommonInit`, оттуда в `gui::App` как поля `engine_trash_rx: trash::TrashRx` и `cb_trash_rx: trash::TrashRx`, и в headless-цикл в `run_headless`.

В `src/gui/mod.rs` в `update()` рядом с `self.drain_feedback();`:

```rust
        // Free whatever the audio thread handed over this frame.
        self.engine_trash_rx.drain();
        self.cb_trash_rx.drain();
```

В `run_headless` в основном цикле рядом с `while c.feedback_rx.pop().is_ok() {}`:

```rust
        c.engine_trash_rx.drain();
        c.cb_trash_rx.drain();
```

- [ ] **Step 10: Тест из Task 8 должен позеленеть**

Обнови второй тест в `tests/rt_safety.rs` — теперь `handle_control` возвращает значение:

```rust
    let mut spent = None;
    assert_no_alloc(|| {
        spent = engine.handle_control(event);
    });
    // The event came back instead of being freed on the audio thread.
    assert!(spent.is_some(), "LoadPatch owns a Box and must be returned");
    drop(spent);
```

```bash
cargo test --features rt-assert --test rt_safety 2>&1 | tail -10
```

Ожидается: оба теста `ok`.

- [ ] **Step 11: Обычные тесты не сломались**

```bash
cargo test 2>&1 | grep "test result"
```

Ожидается: `test result: ok. 26 passed; 0 failed`

- [ ] **Step 12: Commit**

```bash
git add src/trash.rs src/lib.rs src/synth/mod.rs src/synth/sampler.rs src/audio.rs src/gui/mod.rs tests/rt_safety.rs
git commit -m "fix: stop freeing heap memory on the audio thread

handle_control took ControlEvent by value and dropped Box<LoadPatchEvent>,
Box<CcMap>, Box<[DrumPattern;8]>, Box<MidiSeqData> and — worst of all —
the last Arc to a multi-megabyte SoundFont inside the callback. Events now
come back out and are freed by the GUI thread through an SPSC trash queue.

Verified by tests/rt_safety.rs under --features rt-assert."
```

---

### Task 10: Убрать use-after-free в AMidi

Аудит, пункт 2. `receive()` (аудиопоток) загружает указатель порта и затем вызывает `AMidiOutputPort_receive`. Параллельно `close_all()` из JNI-потока закрывает порт. Между загрузкой указателя и FFI-вызовом порт может быть освобождён. Триггер — выдёргивание USB-MIDI во время игры.

**Files:**
- Modify: `src/amidi.rs`

- [ ] **Step 1: Добавить счётчик читателей**

В `src/amidi.rs` расширить структуру:

```rust
#[cfg(target_os = "android")]
pub struct AmidiPort {
    ports: [AtomicPtr<AMidiOutputPort>; MAX_PORTS],
    device: AtomicPtr<AMidiDevice>,
    /// Non-zero while the audio thread is inside an AMidi call. `close_all`
    /// spins on this before releasing anything: an atomic pointer protects the
    /// pointer, not the lifetime of what it points at.
    readers: std::sync::atomic::AtomicUsize,
    /// Set before closing so `receive` stops entering.
    closing: std::sync::atomic::AtomicBool,
}
```

Обновить `new()`:

```rust
    pub const fn new() -> Self {
        Self {
            ports: [
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
            ],
            device: AtomicPtr::new(ptr::null_mut()),
            readers: std::sync::atomic::AtomicUsize::new(0),
            closing: std::sync::atomic::AtomicBool::new(false),
        }
    }
```

- [ ] **Step 2: Взять «читательский билет» в receive**

```rust
    /// Poll ALL open ports for MIDI data. Non-blocking, RT-safe.
    /// Call from audio callback. Returns Some(nbytes) from first port with data.
    #[inline]
    pub fn receive(&self, buf: &mut [u8]) -> Option<usize> {
        if self.closing.load(Ordering::Acquire) {
            return None;
        }
        self.readers.fetch_add(1, Ordering::AcqRel);
        // Re-check: close_all may have set `closing` between our check and the
        // increment. If so, back out without touching any port pointer.
        if self.closing.load(Ordering::Acquire) {
            self.readers.fetch_sub(1, Ordering::AcqRel);
            return None;
        }

        let result = self.receive_inner(buf);
        self.readers.fetch_sub(1, Ordering::AcqRel);
        result
    }

    #[inline]
    fn receive_inner(&self, buf: &mut [u8]) -> Option<usize> {
        for slot in &self.ports {
            let port = slot.load(Ordering::Acquire);
            if port.is_null() { continue; }

            let mut opcode: i32 = 0;
            let mut nbytes: usize = 0;
            let mut timestamp: i64 = 0;

            let rc = unsafe {
                AMidiOutputPort_receive(
                    port, &mut opcode, buf.as_mut_ptr(),
                    buf.len(), &mut nbytes, &mut timestamp,
                )
            };

            if rc > 0 && opcode == AMIDI_OPCODE_DATA && nbytes > 0 {
                return Some(nbytes);
            }
        }
        None
    }
```

- [ ] **Step 3: Дождаться читателей перед закрытием**

```rust
    /// Close all ports and release device. Call before opening new device.
    /// Blocks until the audio thread has left `receive` — this runs on the JNI
    /// thread where blocking for a few microseconds is fine, and it is the only
    /// way to guarantee the audio thread is not inside AMidiOutputPort_receive
    /// when we free the port.
    pub fn close_all(&self) {
        self.closing.store(true, Ordering::Release);

        // Spin until the audio thread is out. One callback's worth of AMidi
        // polling is microseconds; std::hint::spin_loop keeps the wait cheap.
        while self.readers.load(Ordering::Acquire) != 0 {
            std::hint::spin_loop();
        }

        for slot in &self.ports {
            let port = slot.swap(ptr::null_mut(), Ordering::AcqRel);
            if !port.is_null() {
                unsafe { AMidiOutputPort_close(port); }
            }
        }
        let device = self.device.swap(ptr::null_mut(), Ordering::AcqRel);
        if !device.is_null() {
            unsafe { AMidiDevice_release(device); }
        }

        self.closing.store(false, Ordering::Release);
    }
```

- [ ] **Step 4: Проверить, что сборка под Android проходит**

```bash
cargo ndk -t arm64-v8a -P 30 --link-libcxx-shared \
  -o /tmp/jnilibs-check build --release \
  --no-default-features --features gui,android-app 2>&1 | tail -5
```

Ожидается: `Finished \`release\` profile`. Если `cargo-ndk` не установлен: `cargo install cargo-ndk --locked`.

- [ ] **Step 5: Проверить, что не-Android сборка не задета**

```bash
cargo build 2>&1 | tail -2 && cargo test 2>&1 | grep "test result"
```

Ожидается: сборка проходит, `26 passed`.

- [ ] **Step 6: Commit**

```bash
git add src/amidi.rs
git commit -m "fix: prevent use-after-free when USB MIDI is unplugged

receive() loaded a port pointer and then called into AMidi with it, while
close_all() from the JNI thread could free that port in between. An atomic
pointer protects the pointer, not the lifetime of the object. Readers are
now counted and close_all waits them out before releasing anything."
```

---

### Task 11: Дренировать события во время паузы

Аудит, пункт 4. При `paused` callback возвращается сразу, не читая `ctrl_rx`/`midi_rx`. Кольца на 256 элементов забиваются, GUI продолжает писать с `let _ = push(...)`, события молча теряются. После `onResume` движок рассинхронизирован с GUI, возможны повисшие ноты.

**Files:**
- Modify: `src/audio.rs:142-147`

- [ ] **Step 1: Заменить ранний возврат на дренаж**

Было:

```rust
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // If app is paused (backgrounded), output silence to save battery
                if paused_cb.load(Ordering::Relaxed) {
                    for sample in data.iter_mut() { *sample = 0.0; }
                    return;
                }
```

Стало:

```rust
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // Backgrounded: output silence to save battery, but still apply
                // every queued event. Skipping the drain lets the 256-slot rings
                // fill up, and the GUI's pushes are then silently dropped — the
                // engine would come back from onResume disagreeing with the UI
                // about volumes, presets and which notes are held.
                if paused_cb.load(Ordering::Relaxed) {
                    while let Ok(event) = ctrl_rx.pop() {
                        if let Some(spent) = synth.handle_control(event) {
                            trash.dispose(crate::trash::Trash::Control(spent));
                        }
                    }
                    while let Ok(event) = midi_rx.pop() {
                        synth.handle_event(event);
                    }
                    for sample in data.iter_mut() { *sample = 0.0; }
                    return;
                }
```

- [ ] **Step 2: Проверить сборку и тесты**

```bash
cargo build 2>&1 | tail -2 && cargo test 2>&1 | grep "test result"
```

Ожидается: сборка проходит, `26 passed`.

- [ ] **Step 3: Commit**

```bash
git add src/audio.rs
git commit -m "fix: keep draining control events while audio is paused

Backgrounding the app stopped the drain entirely, so the 256-slot rings
filled and every subsequent GUI event was dropped on the floor."
```

---

### Task 12: Убрать аллокацию в обновлении дисплея лупера

Аудит, пункт 12. `looper.rs:489` делает `d.events.push(...)` до 4096 раз внутри вызова, идущего из `handle_control`, то есть из аудиопотока. `try_lock` спасает от блокировки, но не от реаллокации вектора. Отдельно: `unlock()` при выходе из guard делает `futex(FUTEX_WAKE)` — syscall, которого в RT-коде быть не должно.

**Files:**
- Modify: `src/synth/looper.rs:47-53, 482-500`

- [ ] **Step 1: Преаллоцировать вектор**

Было:

```rust
impl LooperDisplay {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            events: Vec::new(),
            current_layer: 0,
        }))
    }
}
```

Стало:

```rust
impl LooperDisplay {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            // Sized for the worst case up front: update_display runs on the
            // audio thread, where growing a Vec means calling malloc.
            events: Vec::with_capacity(MAX_LOOP_EVENTS),
            current_layer: 0,
        }))
    }
}
```

- [ ] **Step 2: Гарантировать, что рост невозможен**

В `update_display` было:

```rust
    fn update_display(&self) {
        if let Ok(mut d) = self.display.try_lock() {
            d.events.clear();
```

Стало:

```rust
    /// Publishes a snapshot of recorded events for the GUI timeline.
    ///
    /// Runs on the audio thread. `try_lock` never blocks, and the vector is
    /// pre-sized to MAX_LOOP_EVENTS so `push` cannot allocate. The `unlock` on
    /// guard drop is still a futex syscall — replacing this Mutex with a triple
    /// buffer is tracked as follow-up work in docs/ARCHITECTURE.md.
    fn update_display(&self) {
        if let Ok(mut d) = self.display.try_lock() {
            debug_assert!(
                d.events.capacity() >= MAX_LOOP_EVENTS,
                "display vector must never grow on the audio thread"
            );
            d.events.clear();
```

Остальное тело не меняется — `clear()` сохраняет ёмкость, `push` до `self.len` (максимум `MAX_LOOP_EVENTS`) не реаллоцирует.

- [ ] **Step 3: Добавить тест на отсутствие аллокации**

В `tests/rt_safety.rs` добавить:

```rust
#[test]
fn looper_display_update_does_not_allocate() {
    use mini_midi_synth::synth::{ControlEvent, SynthEngine};

    let mut engine = SynthEngine::new(48_000.0);

    // Record a loop with a handful of notes so update_display has work to do.
    let _ = engine.handle_control(ControlEvent::LooperRecord);
    for note in 60..72u8 {
        engine.handle_event(mini_midi_synth::synth::MidiEvent::NoteOn {
            channel: 0, note, velocity: 100,
        });
    }

    assert_no_alloc(|| {
        let _ = engine.handle_control(ControlEvent::LooperTogglePlay);
        let _ = engine.handle_control(ControlEvent::LooperUndo);
        let _ = engine.handle_control(ControlEvent::LooperClear);
    });
}
```

- [ ] **Step 4: Запустить**

```bash
cargo test --features rt-assert --test rt_safety 2>&1 | tail -10
```

Ожидается: все три теста `ok`. Если этот падает — где-то в `start_record`/`undo`/`clear` есть ещё одна аллокация; найди её по стеку в сообщении об аборте.

- [ ] **Step 5: Обычные тесты**

```bash
cargo test 2>&1 | grep "test result"
```

Ожидается: `26 passed`

- [ ] **Step 6: Commit**

```bash
git add src/synth/looper.rs tests/rt_safety.rs
git commit -m "fix: pre-size looper display vector so it cannot allocate in RT

update_display runs from handle_control on the audio thread and pushed up
to MAX_LOOP_EVENTS entries into an empty Vec."
```

---

## Phase 4 — Функциональные баги

### Task 13: Остановить бесконечный BLE-реконнект

Аудит, пункт 5. `mReconnectRunnable` перепланируется вечно, а `connectBridge` сбрасывает `mReconnectAttempts = 0` при успехе. При подключённом устройстве каждые 3 секунды снова вызывается `openPairedBluetoothMidiDevices()`, добавляя новый `MidiDevice` в `mOpenDevices` и новый `MidiBridge` в `mBridges`. Каждая нота приходит в Rust N раз, N растёт линейно со временем работы.

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`

- [ ] **Step 1: Перестать переспрашивать, когда мост уже есть**

Заменить `mReconnectRunnable`:

```java
    private final Runnable mReconnectRunnable = new Runnable() {
        @Override
        public void run() {
            // A live bridge means we are done: rescanning would open the same
            // device again, appending another MidiBridge and duplicating every
            // incoming note.
            if (!mBridges.isEmpty()) {
                Log.i(TAG, "BLE reconnect: bridge already connected, stopping");
                return;
            }
            if (mReconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
                Log.i(TAG, "BLE reconnect: max attempts reached, stopping");
                return;
            }
            mReconnectAttempts++;
            Log.i(TAG, "BLE reconnect attempt " + mReconnectAttempts);
            openPairedBluetoothMidiDevices();
            long delay = Math.min(RECONNECT_BASE_DELAY_MS * (1L << (mReconnectAttempts - 1)), 30000);
            mReconnectHandler.postDelayed(this, delay);
        }
    };
```

- [ ] **Step 2: Не сбрасывать счётчик попыток при успехе**

В `connectBridge` было:

```java
                    mBridges.add(bridge);
                    mReconnectAttempts = 0; // Reset backoff on success
```

Стало:

```java
                    mBridges.add(bridge);
                    // Do not reset mReconnectAttempts here: the runnable already
                    // stops once mBridges is non-empty, and resetting turned the
                    // exponential backoff into a permanent 3-second poll.
                    mReconnectHandler.removeCallbacks(mReconnectRunnable);
```

- [ ] **Step 3: Не открывать одно устройство дважды**

Добавить проверку в `openBondedDevices` перед `openBluetoothDevice`:

```java
        Handler handler = new Handler(Looper.getMainLooper());
        for (BluetoothDevice device : bonded) {
            String name = device.getName();
            if (isAlreadyOpen(device.getAddress())) {
                Log.i(TAG, "BLE MIDI already open, skipping: " + name);
                continue;
            }
            Log.i(TAG, "Trying BLE MIDI: " + name + " [" + device.getAddress() + "]");
```

И добавить вспомогательный метод и поле рядом с `mOpenDevices`:

```java
    /// Addresses of devices we already opened, so a rescan does not open the
    /// same device a second time.
    private final java.util.Set<String> mOpenAddresses = new java.util.HashSet<>();

    private boolean isAlreadyOpen(String address) {
        return address != null && mOpenAddresses.contains(address);
    }
```

В `onDeviceOpened` внутри `openBondedDevices` зарегистрировать адрес:

```java
                        public void onDeviceOpened(MidiDevice midiDevice) {
                            if (midiDevice == null) {
                                Log.w(TAG, "BLE MIDI failed: " + name);
                                return;
                            }
                            mOpenAddresses.add(device.getAddress());
                            mOpenDevices.add(midiDevice);
                            Log.i(TAG, "BLE MIDI opened: " + name);
                            connectBridge(midiDevice, name);
                        }
```

`device` внутри анонимного класса должен быть effectively final — он таким и является, так как в цикле объявлен заново на каждой итерации.

- [ ] **Step 4: Очищать множество в onDestroy**

В `onDestroy` после `mOpenDevices.clear();` добавить:

```java
        mOpenAddresses.clear();
```

- [ ] **Step 5: Собрать APK**

```bash
cd /home/dima/Projects/mini_midi_synth/android && ./gradlew assembleDebug --no-daemon 2>&1 | tail -5
```

Ожидается: `BUILD SUCCESSFUL`. Ошибок компиляции Java быть не должно.

- [ ] **Step 6: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth
git add android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "fix: stop BLE MIDI reconnect loop from duplicating bridges

connectBridge reset the backoff counter on success while the runnable kept
rescheduling itself, so a connected keyboard was re-opened every 3 seconds.
Each rescan appended another MidiDevice and MidiBridge, and every note
arrived in Rust once per accumulated bridge."
```

---

### Task 14: Восстанавливать полное состояние после переподключения аудио

Аудит, пункт 6. `reconnect_audio` пересоздаёт движок и вызывает только `send_edited_params` по партам. Теряются глобальные параметры, загруженные SF2, драм-кит, содержимое лупера, CC-мапа, arp/step-seq, per-part volume/range/pan/sf2_mode. Вынуть наушники = потерять сессию.

**Files:**
- Modify: `src/gui/settings.rs:247-253`

- [ ] **Step 1: Заменить частичную отправку на полную**

Было:

```rust
        // Re-send current patch to new engine
        for part in 0..self.parts.len() {
            self.send_edited_params(part);
        }

        log::info!("[reconnect] Audio restarted: {actual_sr}Hz");
        Ok(())
```

Стало:

```rust
        // Re-send the entire GUI state, not just patches. send_edited_params
        // alone leaves the fresh engine without global params, soundfonts,
        // drum kit, part volumes and ranges — pulling out headphones used to
        // silently reset half the session.
        self.send_initial_presets();
        self.send_full_part_state();
        self.send_cc_map();

        log::info!("[reconnect] Audio restarted: {actual_sr}Hz");
        Ok(())
```

- [ ] **Step 2: Написать send_full_part_state**

Проверь, какие поля `PartState` уже отправляются в `send_edited_params`:

```bash
grep -n "fn send_edited_params" -A 30 src/gui/mod.rs
grep -n "fn send_part_volume" -A 10 src/gui/mod.rs
```

Добавить в `src/gui/mod.rs` метод, отправляющий всё остальное:

```rust
    /// Pushes every per-part setting that is not part of the patch itself.
    /// Used on startup and after an audio reconnect, where the engine is new
    /// and knows nothing about the GUI's state.
    pub(super) fn send_full_part_state(&mut self) {
        for (i, p) in self.parts.iter().enumerate() {
            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: p.enabled });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: p.mute });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartVolume { part: i, volume: p.volume });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartRange {
                part: i, min_note: p.min_note, max_note: p.max_note,
            });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange {
                part: i, vel_min: p.vel_min, vel_max: p.vel_max,
            });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan: p.pan });
            let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose {
                part: i, semitones: p.transpose,
            });
            if p.sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: true });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                    part: i, program: p.sf2_program, bank: 0,
                });
            }
        }
    }
```

Имена полей должны совпадать с реальной структурой `PartState`. Сверься:

```bash
grep -n "pub struct PartState" -A 25 src/gui/mod.rs
```

Если какого-то поля нет (например `mute` или `vel_min`) — убери соответствующую строку, не выдумывай поля.

- [ ] **Step 3: Написать send_cc_map**

```rust
    /// Re-sends the MIDI CC mapping to a freshly created engine.
    pub(super) fn send_cc_map(&mut self) {
        let _ = self.ctrl_tx.push(ControlEvent::SetCcMap {
            map: Box::new(self.cc_map.clone()),
        });
    }
```

Требует `Clone` на `CcMap`. Проверь:

```bash
grep -n "pub struct CcMap" -B3 src/cc_map.rs
```

Если `Clone` не выведен — добавь его в `#[derive(...)]`.

- [ ] **Step 4: Убедиться, что кольцо не переполнится**

`send_initial_presets` + `send_full_part_state` + `send_cc_map` при 8 партах дают порядка 8×(патч + 9 событий) + глобальные ≈ 100 событий, а кольцо рассчитано на 256. Запас есть, но проверь фактическое число:

```bash
grep -c "ctrl_tx.push" src/gui/mod.rs
```

Если после правок отправка превысит 256 событий разом — увеличь ёмкость колец в `src/lib.rs` и `src/gui/settings.rs` с 256 до 512 в обоих местах одинаково.

- [ ] **Step 5: Сборка и тесты**

```bash
cargo build 2>&1 | tail -2 && cargo test 2>&1 | grep "test result"
```

Ожидается: сборка проходит, `26 passed`.

- [ ] **Step 6: Ручная проверка**

```bash
cargo run --release
```

Загрузи пресет, выкрути master volume, включи парт B. Затем выдерни наушники (или переключи выход в системном микшере), чтобы сработал disconnect. После сообщения «Audio reconnected» громкость и парты должны остаться теми же. Раньше сбрасывались.

- [ ] **Step 7: Commit**

```bash
git add src/gui/mod.rs src/gui/settings.rs
git commit -m "fix: restore full state after audio reconnect

Reconnect built a fresh engine and only re-sent patches, so global params,
soundfonts, drum kit, part volumes, ranges and the CC map were lost."
```

---

### Task 15: Пропускать CC 120-127 в движок

Аудит, пункт 7. `midi.rs:60` пропускает только `(1..=119)`, поэтому All Sound Off (120) и All Notes Off (123) с внешнего устройства до движка не доходят. Panic-кнопка на клавиатуре не работает.

**Files:**
- Modify: `src/midi.rs:54-65`
- Modify: `src/synth/mod.rs` (обработчик ControlChange)
- Create: `tests/midi_parse.rs`

- [ ] **Step 1: Написать падающий тест**

Создать `tests/midi_parse.rs`:

```rust
//! Parsing of MIDI bytes into engine events.

use mini_midi_synth::midi::parse_midi_message;
use mini_midi_synth::synth::MidiEvent;

#[test]
fn note_on_parses() {
    let (event, consumed) = parse_midi_message(&[0x90, 60, 100]).expect("valid note on");
    assert_eq!(consumed, 3);
    match event {
        MidiEvent::NoteOn { channel, note, velocity } => {
            assert_eq!((channel, note, velocity), (0, 60, 100));
        }
        _ => panic!("expected NoteOn, got something else"),
    }
}

#[test]
fn all_notes_off_reaches_the_engine() {
    // CC 123 on channel 1 — the panic button on most controllers.
    let (event, _) = parse_midi_message(&[0xB0, 123, 0])
        .expect("CC 123 must not be filtered out");
    match event {
        MidiEvent::ControlChange { cc, .. } => assert_eq!(cc, 123),
        _ => panic!("expected ControlChange"),
    }
}

#[test]
fn all_sound_off_reaches_the_engine() {
    let (event, _) = parse_midi_message(&[0xB0, 120, 0])
        .expect("CC 120 must not be filtered out");
    match event {
        MidiEvent::ControlChange { cc, .. } => assert_eq!(cc, 120),
        _ => panic!("expected ControlChange"),
    }
}

#[test]
fn bank_select_is_still_filtered() {
    // CC 0 and CC 32 are bank select; the engine has no use for them and they
    // would otherwise collide with mapped parameters.
    assert!(parse_midi_message(&[0xB0, 0, 0]).is_none());
    assert!(parse_midi_message(&[0xB0, 32, 0]).is_none());
}
```

Тест требует, чтобы `midi` был публичным модулем. В `src/lib.rs` заменить `mod midi;` на `pub mod midi;`.

- [ ] **Step 2: Убедиться, что тесты падают**

```bash
cargo test --test midi_parse 2>&1 | tail -20
```

Ожидается: `note_on_parses ... ok`, `bank_select_is_still_filtered ... ok`, а два теста про 120/123 **падают** с `CC 123 must not be filtered out`.

- [ ] **Step 3: Расширить фильтр**

В `src/midi.rs` было:

```rust
            if cc_num == 1 {
                Some(MidiEvent::ModWheel { channel, value: v as f32 / MIDI_MAX_VAL })
            } else if (1..=119).contains(&cc_num) && cc_num != 32 {
                Some(MidiEvent::ControlChange { channel, cc: cc_num, value: v })
            } else {
                None
            }
```

Стало:

```rust
            if cc_num == 1 {
                Some(MidiEvent::ModWheel { channel, value: v as f32 / MIDI_MAX_VAL })
            } else if cc_num == 0 || cc_num == 32 {
                // Bank select — nothing downstream uses it, and letting it
                // through would collide with mapped parameters.
                None
            } else {
                // Everything else, including channel mode messages 120-127.
                // CC 120 (All Sound Off) and CC 123 (All Notes Off) are the
                // panic button on hardware controllers and must reach the
                // engine.
                Some(MidiEvent::ControlChange { channel, cc: cc_num, value: v })
            }
```

- [ ] **Step 4: Обработать 120 и 123 в движке**

Найти обработчик ControlChange:

```bash
grep -n "MidiEvent::ControlChange" -A 20 src/synth/mod.rs | head -40
```

В начале обработки CC, до сопоставления с CC-мапой, добавить:

```rust
                    // Channel mode messages. 120 = All Sound Off (immediate),
                    // 123 = All Notes Off (respects release stage). Both are
                    // treated as a panic: hardware sends them when the player
                    // hits the panic button or the transport stops.
                    120 | 123 => {
                        for part in &mut self.parts {
                            for v in &mut part.voices {
                                if v.active { v.note_off(); }
                            }
                            part.note_stack.clear();
                            part.latch_held.clear();
                            part.sustain_pedal = 0.0;
                            part.sustained_notes = [false; 128];
                        }
                        self.sampler.all_notes_off();
                        return;
                    }
```

Вставить в тот же `match cc { ... }`, где уже есть ветка `64 =>` (сустейн), рядом с ней. Проверь структуру обработчика — если это не `match`, а цепочка `if`, адаптируй.

- [ ] **Step 5: Тесты зелёные**

```bash
cargo test --test midi_parse 2>&1 | grep "test result"
cargo test 2>&1 | grep "test result"
```

Ожидается: оба `ok`, ни одного падения.

- [ ] **Step 6: Commit**

```bash
git add src/midi.rs src/synth/mod.rs src/lib.rs tests/midi_parse.rs
git commit -m "fix: let channel mode messages CC 120/123 reach the engine

The CC filter passed only 1..=119, so All Sound Off and All Notes Off from
the controller were dropped and the hardware panic button did nothing."
```

---

### Task 16: Считать потерянные MIDI-события

Аудит, пункт 9. `midi.rs:125` — `try_lock`, при занятом мьютексе событие исчезает; результат `push` отбрасывается. Потерянный NoteOff = повисшая нота, и об этом никто не узнаёт.

**Files:**
- Modify: `src/midi.rs`
- Create: `src/audio/metrics.rs` (используется и в Task 22)

- [ ] **Step 1: Создать счётчики**

Создать `src/metrics.rs`:

```rust
//! Health counters for the audio path. Everything here is a plain atomic that
//! real-time code can bump without allocating or blocking; the GUI reads them
//! once a frame.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct Metrics {
    /// MIDI events dropped because the ring was full or the producer was busy.
    pub midi_dropped: AtomicU64,
    /// Control events the GUI could not enqueue.
    pub ctrl_dropped: AtomicU64,
    /// Objects handed to the trash queue that had to be freed in place.
    pub trash_dropped: AtomicU64,
    /// Audio callbacks that reported an error (xrun, device error).
    pub stream_errors: AtomicU32,
    /// Longest callback duration seen, in microseconds.
    pub worst_callback_us: AtomicU32,
}

pub type SharedMetrics = Arc<Metrics>;

pub fn new() -> SharedMetrics {
    Arc::new(Metrics::default())
}

impl Metrics {
    #[inline]
    pub fn note_midi_drop(&self) {
        self.midi_dropped.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn note_ctrl_drop(&self) {
        self.ctrl_dropped.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    pub fn note_callback_time(&self, micros: u32) {
        self.worst_callback_us.fetch_max(micros, Ordering::Relaxed);
    }

    /// Resets the peak so the GUI shows a rolling worst case rather than the
    /// worst case since launch, which is always the first callback.
    pub fn take_worst_callback_us(&self) -> u32 {
        self.worst_callback_us.swap(0, Ordering::Relaxed)
    }
}
```

Объявить в `src/lib.rs`: `pub mod metrics;`

- [ ] **Step 2: Считать потери в parse_and_push**

В `src/midi.rs` расширить сигнатуру:

```rust
pub fn parse_and_push(
    data: &[u8],
    tx: &SharedMidiTx,
    note_state: &NoteState,
    pad_state: &PadState,
    metrics: &crate::metrics::Metrics,
) -> usize {
```

и заменить хвост:

```rust
    if let Ok(mut tx) = tx.try_lock() {
        if tx.push(event).is_err() {
            metrics.note_midi_drop();
        }
    } else {
        metrics.note_midi_drop();
    }

    consumed
}
```

- [ ] **Step 3: Протянуть metrics до всех вызовов**

`parse_and_push` вызывается в `connect_by_name` (замыкание midir) и в JNI-функции `Java_com_minimidisynth_MidiBridge_onMidiData` в `src/lib.rs`.

В `connect_by_name` добавить параметр `metrics: crate::metrics::SharedMetrics` и захватить его в замыкание:

```rust
            move |_timestamp, data, _| {
                parse_and_push(data, &tx, &note_state, &pad_state, &metrics);
            },
```

`connect` — тонкая обёртка, ей тоже нужен параметр. Все места вызова (`src/lib.rs` в `init_common` и в цикле реконнекта, `src/gui/settings.rs` в `on_midi_reconnect`) передают `c.metrics.clone()`.

Для JNI-пути добавить статик рядом с остальными:

```rust
#[cfg(target_os = "android")]
static JNI_METRICS: OnceLock<crate::metrics::SharedMetrics> = OnceLock::new();
```

устанавливать его в `init_common` рядом с `JNI_MIDI_TX`, и использовать в `onMidiData`:

```rust
    let (Some(tx), Some(ns), Some(ps), Some(m)) = (
        JNI_MIDI_TX.get(), JNI_NOTE_STATE.get(), JNI_PAD_STATE.get(), JNI_METRICS.get()
    ) else {
        log::warn!("[jni_midi] OnceLock not set yet");
        return;
    };

    let mut offset = 0;
    while offset < bytes.len() {
        let consumed = midi::parse_and_push(&bytes[offset..], tx, ns, ps, m);
        if consumed == 0 { break }
        offset += consumed;
    }
```

- [ ] **Step 4: Создать metrics в init_common**

В `src/lib.rs` рядом с созданием колец:

```rust
    let metrics = metrics::new();
    #[cfg(target_os = "android")]
    { let _ = JNI_METRICS.set(metrics.clone()); }
```

Добавить поле `metrics: crate::metrics::SharedMetrics` в `CommonInit` и заполнить его.

- [ ] **Step 5: Считать потери на стороне GUI**

Найти все `let _ = self.ctrl_tx.push(...)` в `src/gui/` и заменить на вызов хелпера. Добавить в `src/gui/mod.rs`:

```rust
    /// Enqueues a control event, counting the drop if the ring is full.
    /// A full ring means the engine stopped draining — the user needs to know.
    pub(super) fn send_ctrl(&mut self, event: ControlEvent) {
        if self.ctrl_tx.push(event).is_err() {
            self.metrics.note_ctrl_drop();
        }
    }
```

Замену делай механически:

```bash
grep -rn "let _ = self.ctrl_tx.push(" src/gui/ | wc -l
```

Замени каждое `let _ = self.ctrl_tx.push(X);` на `self.send_ctrl(X);`. Компилятор поймает места, где это невозможно из-за заимствований — там оставь прежнюю форму, но добавь подсчёт вручную.

- [ ] **Step 6: Сборка и тесты**

```bash
cargo build 2>&1 | tail -2 && cargo test 2>&1 | grep "test result"
```

Ожидается: сборка проходит, `26 passed`.

- [ ] **Step 7: Commit**

```bash
git add src/metrics.rs src/midi.rs src/lib.rs src/gui/mod.rs src/gui/settings.rs
git commit -m "feat: count dropped MIDI and control events

A busy producer lock or a full ring silently swallowed events; a lost
NoteOff is a stuck note with no trace anywhere. Task 22 surfaces these
counters in the UI."
```

---

### Task 17: Санитизировать имена файлов пресетов

Аудит, пункт 10. `preset.rs:530` и `preset.rs:17` строят имя файла из пользовательского ввода без проверки: имя `../../foo` пишет за пределы каталога пресетов.

**Files:**
- Modify: `src/preset.rs`
- Create: `tests/preset_roundtrip.rs`

- [ ] **Step 1: Написать падающий тест**

Создать `tests/preset_roundtrip.rs`:

```rust
//! Preset naming and serialization.

use mini_midi_synth::preset::sanitize_preset_filename;

#[test]
fn ordinary_names_become_snake_case() {
    assert_eq!(sanitize_preset_filename("Warm Pad"), "warm_pad");
    assert_eq!(sanitize_preset_filename("TB303 Acid v2"), "tb303_acid_v2");
}

#[test]
fn path_separators_cannot_escape_the_preset_directory() {
    assert_eq!(sanitize_preset_filename("../../etc/passwd"), "etc_passwd");
    assert_eq!(sanitize_preset_filename("/absolute/path"), "absolute_path");
    assert_eq!(sanitize_preset_filename(".."), "preset");
}

#[test]
fn empty_and_exotic_names_still_produce_a_usable_filename() {
    assert_eq!(sanitize_preset_filename(""), "preset");
    assert_eq!(sanitize_preset_filename("???"), "preset");
    // Non-ASCII is kept only if alphanumeric, so a Cyrillic name survives.
    assert_eq!(sanitize_preset_filename("Бас"), "бас");
}
```

- [ ] **Step 2: Убедиться, что не компилируется**

```bash
cargo test --test preset_roundtrip 2>&1 | tail -5
```

Ожидается: ошибка `cannot find function sanitize_preset_filename`.

- [ ] **Step 3: Реализовать**

В `src/preset.rs` добавить рядом с `save_json`:

```rust
/// Turns a user-supplied preset name into a safe filename stem.
///
/// Preset names come from a text field, so `../../etc/foo` would otherwise
/// write outside the preset directory. Everything that is not alphanumeric
/// collapses into a single underscore, and an empty result falls back to a
/// fixed name rather than producing `.json` with no stem.
pub fn sanitize_preset_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut last_was_sep = true; // leading separators are trimmed

    for ch in name.to_lowercase().chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }

    if out.is_empty() {
        "preset".to_string()
    } else {
        out
    }
}
```

- [ ] **Step 4: Использовать в обоих местах записи**

В `save_json` было:

```rust
    let filename = name.to_lowercase().replace(' ', "_") + ".json";
```

Стало:

```rust
    let filename = sanitize_preset_filename(name) + ".json";
```

В `save_patch` было:

```rust
    let filename = preset.name.to_lowercase().replace(' ', "_") + ".json";
```

Стало:

```rust
    let filename = sanitize_preset_filename(&preset.name) + ".json";
```

- [ ] **Step 5: Тест зелёный**

```bash
cargo test --test preset_roundtrip 2>&1 | grep "test result"
```

Ожидается: `test result: ok. 3 passed`

- [ ] **Step 6: Commit**

```bash
git add src/preset.rs tests/preset_roundtrip.rs
git commit -m "fix: sanitize preset names before using them as filenames

A preset named ../../foo wrote outside the preset directory."
```

---

### Task 18: Атомарная запись конфигурации и пресетов

Аудит, пункт 11. `fs::write` напрямую: обрыв на записи оставляет обрезанный JSON. Плюс `config.rs:192` заменяет битый конфиг дефолтным молча, и через 2 секунды автосейв затирает оригинал.

**Files:**
- Modify: `src/config.rs`
- Modify: `src/preset.rs`

- [ ] **Step 1: Написать хелпер атомарной записи**

В `src/config.rs` добавить:

```rust
/// Writes `contents` to `path` atomically: a crash mid-write leaves either the
/// old file or the new one, never a truncated one. Android kills backgrounded
/// processes without warning, which makes this more than theoretical.
pub fn write_atomic(path: &std::path::Path, contents: &str) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
```

- [ ] **Step 2: Использовать в Config::save**

Было:

```rust
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        Ok(())
```

Стало:

```rust
        let json = serde_json::to_string_pretty(self)?;
        write_atomic(&path, &json)?;
        Ok(())
```

- [ ] **Step 3: Сохранять битый конфиг вместо молчаливой замены**

Было:

```rust
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }
```

Стало:

```rust
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };

        match serde_json::from_str(&contents) {
            Ok(config) => config,
            Err(e) => {
                // Keep the unparseable file: autosave would overwrite it two
                // seconds later and the user's settings would be gone for good.
                let backup = path.with_extension("json.bad");
                let _ = fs::write(&backup, &contents);
                log::warn!(
                    "[config] {} is not valid JSON ({e}); starting from defaults, \
                     original kept at {}",
                    path.display(),
                    backup.display()
                );
                eprintln!(
                    "[config] Could not parse {}: {e}\n[config] Original saved as {}",
                    path.display(),
                    backup.display()
                );
                Self::default()
            }
        }
    }
```

- [ ] **Step 4: Использовать в preset.rs**

В `save_json` было:

```rust
    let json = serde_json::to_string_pretty(obj)?;
    fs::write(&path, json)?;
    Ok(path)
```

Стало:

```rust
    let json = serde_json::to_string_pretty(obj)?;
    crate::config::write_atomic(&path, &json)?;
    Ok(path)
```

То же в `save_patch`.

- [ ] **Step 5: Добавить тест**

В `tests/preset_roundtrip.rs` добавить:

```rust
#[test]
fn atomic_write_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join("mms-atomic-write-test");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("thing.json");

    mini_midi_synth::config::write_atomic(&target, "{\"a\":1}").unwrap();

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"a\":1}");
    assert!(!dir.join("thing.json.tmp").exists(), "temp file must be renamed away");

    std::fs::remove_dir_all(&dir).ok();
}
```

Требует `pub mod config;` в `src/lib.rs`.

- [ ] **Step 6: Тесты**

```bash
cargo test 2>&1 | grep "test result"
```

Ожидается: все зелёные.

- [ ] **Step 7: Commit**

```bash
git add src/config.rs src/preset.rs src/lib.rs tests/preset_roundtrip.rs
git commit -m "fix: write config and presets atomically, keep unparseable configs

fs::write leaves a truncated file if the process dies mid-write, and a
broken config was silently replaced by defaults and then overwritten by
autosave two seconds later."
```

---

### Task 19: Честно сообщать про SF2 на Android

Аудит, пункт 8. `gui/mod.rs:184` сканирует `/sdcard/Download`, но манифест не запрашивает storage-разрешений, а `minSdk = 29`. На Android 11+ `read_dir` возвращает `Err`, ошибка проглатывается, файлы просто не появляются в списке без объяснения.

Полноценное решение — Storage Access Framework с пикером, что требует Java-активити и обратного вызова в Rust. Это отдельная задача сопоставимого объёма с Task 13. В этом плане делается меньший, но честный шаг: приложение перестаёт молча притворяться, что сканирование работает, и подсказывает пользователю рабочий путь.

**Files:**
- Modify: `src/gui/mod.rs:179-204`

- [ ] **Step 1: Различать «пусто» и «нет доступа»**

Было:

```rust
pub fn scan_sf2_files() -> Vec<(String, std::path::PathBuf)> {
    let mut result = Vec::new();
    let mut dirs_to_scan = vec![sf2_dir()];
    // On Android, also scan /sdcard/Download for SF2 files
    #[cfg(target_os = "android")]
    dirs_to_scan.push(std::path::PathBuf::from("/sdcard/Download"));

    for dir in &dirs_to_scan {
        if let Ok(entries) = std::fs::read_dir(dir) {
```

Стало:

```rust
/// Directories that were requested but could not be read, so the UI can say so
/// instead of showing an empty list.
pub struct Sf2Scan {
    pub files: Vec<(String, std::path::PathBuf)>,
    pub unreadable: Vec<(std::path::PathBuf, String)>,
}

pub fn scan_sf2_files() -> Sf2Scan {
    let mut files = Vec::new();
    let mut unreadable = Vec::new();
    let mut dirs_to_scan = vec![sf2_dir()];
    // On Android /sdcard/Download is only readable with storage permissions
    // this app does not hold; scanning it usually fails, and the user needs to
    // be told that rather than shown an empty list.
    #[cfg(target_os = "android")]
    dirs_to_scan.push(std::path::PathBuf::from("/sdcard/Download"));

    for dir in &dirs_to_scan {
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("sf2") {
                        let name = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                            .to_string();
                        if !files.iter().any(|(n, _): &(String, std::path::PathBuf)| n == &name) {
                            files.push((name, path));
                        }
                    }
                }
            }
            Err(e) => {
                // A missing private sf2 dir is normal on first run; anything
                // else is worth surfacing.
                if e.kind() != std::io::ErrorKind::NotFound {
                    unreadable.push((dir.clone(), e.to_string()));
                }
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Sf2Scan { files, unreadable }
}
```

- [ ] **Step 2: Обновить места использования**

```bash
grep -rn "scan_sf2_files()" src/
```

В `src/lib.rs` было `sf2_file_list: gui::scan_sf2_files(),` — стало:

```rust
        sf2_file_list: {
            let scan = gui::scan_sf2_files();
            for (dir, err) in &scan.unreadable {
                log::warn!("[sf2] cannot read {}: {err}", dir.display());
            }
            scan.files
        },
```

В `src/gui/settings.rs`, где список обновляется по кнопке, сделать так же и показать тост:

```rust
                let scan = crate::gui::scan_sf2_files();
                for (dir, err) in &scan.unreadable {
                    self.show_toast(
                        format!("Cannot read {}: {err}", dir.display()),
                        ToastKind::Error,
                    );
                }
                self.sf2_file_list = scan.files;
```

Найди точное место:

```bash
grep -n "sf2_file_list = " src/gui/settings.rs
```

- [ ] **Step 3: Подсказать рабочий путь в UI**

В панели выбора SF2 в `src/gui/settings.rs` под списком добавить строку, видимую только на Android:

```rust
        #[cfg(target_os = "android")]
        ui.label(
            egui::RichText::new(
                "SF2 files must be placed in the app's own storage:\n\
                 Android/data/com.minimidisynth/files/sf2/",
            )
            .small()
            .weak(),
        );
```

Проверь, что путь соответствует тому, что возвращает `app_data_dir()` на Android — это `internal_data_path()` из `AndroidApp`, то есть `/data/data/com.minimidisynth/files`. Уточни фактическое значение через logcat при запуске (`data dir:` пишется в `android_main`) и поставь в подсказку именно его.

- [ ] **Step 4: Сборка**

```bash
cargo build 2>&1 | tail -2 && cargo test 2>&1 | grep "test result"
cd android && ./gradlew assembleDebug --no-daemon 2>&1 | tail -3
```

Ожидается: обе сборки проходят.

- [ ] **Step 5: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth
git add src/gui/mod.rs src/gui/settings.rs src/lib.rs
git commit -m "fix: report unreadable SF2 directories instead of showing nothing

Scanning /sdcard/Download fails on Android 11+ without storage permissions;
the error was swallowed and the user saw an empty list with no explanation."
```

---

## Phase 5 — Android релиз

### Task 20: Настроить подпись релизной сборки

Аудит, пункт 15. `android/app/build.gradle.kts:22` подписывает release-сборку debug-ключом. Публиковать такой APK нельзя, и подпись непереносима между машинами.

**Files:**
- Modify: `android/app/build.gradle.kts`

- [ ] **Step 1: Заменить конфигурацию сборки**

```kotlin
plugins {
    id("com.android.application")
}

/// Release signing is configured through ~/.gradle/gradle.properties so that
/// no keystore path, password or alias ever lands in the repository:
///
///   mmsStoreFile=/home/you/keys/mini-midi-synth.jks
///   mmsStorePassword=...
///   mmsKeyAlias=mini-midi-synth
///   mmsKeyPassword=...
///
/// Without those properties the release build falls back to the debug key and
/// says so, so a local `assembleRelease` still works for testing.
val hasReleaseKey = project.hasProperty("mmsStoreFile")

android {
    namespace = "com.minimidisynth"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.minimidisynth"
        minSdk = 29
        targetSdk = 35
        versionCode = (project.findProperty("mmsVersionCode") as String?)?.toInt() ?: 1
        versionName = "0.1.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    signingConfigs {
        if (hasReleaseKey) {
            create("release") {
                storeFile = file(project.property("mmsStoreFile") as String)
                storePassword = project.property("mmsStorePassword") as String
                keyAlias = project.property("mmsKeyAlias") as String
                keyPassword = project.property("mmsKeyPassword") as String
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = if (hasReleaseKey) {
                signingConfigs.getByName("release")
            } else {
                logger.warn(
                    "No mmsStoreFile property found — signing release with the " +
                    "debug key. This APK cannot be distributed."
                )
                signingConfigs.getByName("debug")
            }
        }
    }
}
```

- [ ] **Step 2: Проверить, что сборка без ключа всё ещё работает**

```bash
cd /home/dima/Projects/mini_midi_synth/android && ./gradlew assembleRelease --no-daemon 2>&1 | grep -E "WARNING|BUILD"
```

Ожидается: предупреждение про debug-ключ и `BUILD SUCCESSFUL`.

- [ ] **Step 3: Задокументировать процедуру создания ключа**

Добавить в `README.md` в раздел «Сборка» подраздел:

```markdown
### Релизная подпись Android

Ключ создаётся один раз и никогда не попадает в репозиторий:

```bash
keytool -genkey -v -keystore ~/keys/mini-midi-synth.jks \
  -keyalg RSA -keysize 4096 -validity 10000 -alias mini-midi-synth
```

Затем в `~/.gradle/gradle.properties`:

```properties
mmsStoreFile=/home/you/keys/mini-midi-synth.jks
mmsStorePassword=...
mmsKeyAlias=mini-midi-synth
mmsKeyPassword=...
mmsVersionCode=2
```

Потеря этого файла означает невозможность обновить уже установленное приложение — сделай резервную копию.
```

- [ ] **Step 4: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth
git add android/app/build.gradle.kts README.md
git commit -m "build: configure release signing from gradle.properties

Release APKs were signed with the debug key. Keystore path and passwords
now come from ~/.gradle/gradle.properties and never touch the repo."
```

---

### Task 21: Сохранить символы для отладки нативных крашей

Аудит, пункт 19. `strip = "symbols"` означает, что нативные краши на Android приходят без имён функций.

**Files:**
- Modify: `Cargo.toml:50-57`
- Modify: `android/build_rust.sh`
- Modify: `android/app/src/main/AndroidManifest.xml`

- [ ] **Step 1: Ослабить strip**

Было:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
```

Стало:

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
# `debuginfo` rather than `symbols`: the function name table survives, so a
# native crash in logcat is readable, while the DWARF sections that make the
# .so huge are gone.
strip = "debuginfo"
```

- [ ] **Step 2: Сравнить размер библиотеки**

```bash
cd /home/dima/Projects/mini_midi_synth
./android/build_rust.sh 2>&1 | tail -2
ls -la android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so
```

Запиши размер. Прирост относительно `strip = "symbols"` обычно единицы процентов; если библиотека выросла больше чем в полтора раза — вернись к `symbols` и вместо этого сохраняй неstripped-копию отдельно (шаг 3).

- [ ] **Step 3: Сохранять неstripped-копию для символизации**

Дополнить `android/build_rust.sh`:

```bash
#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-/home/dima/.local/opt/android-sdk/ndk/28.2.13676358}"

cargo ndk \
    -t arm64-v8a \
    -P 30 \
    --link-libcxx-shared \
    -o android/app/src/main/jniLibs \
    build --release \
    --no-default-features \
    --features gui,android-app

# Keep the unstripped library so ndk-stack can symbolize crash dumps later.
# Not committed — see .gitignore.
mkdir -p android/symbols/arm64-v8a
cp target/aarch64-linux-android/release/libmini_midi_synth.so \
   android/symbols/arm64-v8a/libmini_midi_synth.so

echo "Done: android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so"
echo "Symbols: android/symbols/arm64-v8a/libmini_midi_synth.so"
echo "Symbolize a crash with:"
echo "  adb logcat | \$ANDROID_NDK_HOME/prebuilt/linux-x86_64/bin/ndk-stack -sym android/symbols/arm64-v8a"
```

Добавить в `.gitignore`:

```
android/symbols/
```

- [ ] **Step 4: Разрешить резервное копирование настроек**

Заодно закрыть недостающий атрибут в манифесте — по умолчанию Android делает резервные копии данных приложения, включая config.json, что для пользователя скорее хорошо, но должно быть заявлено явно:

```xml
    <application
        android:label="Mini MIDI Synth"
        android:icon="@mipmap/ic_launcher"
        android:hasCode="true"
        android:allowBackup="true"
        android:fullBackupContent="true"
        android:theme="@android:style/Theme.NoTitleBar.Fullscreen">
```

И ограничить legacy-разрешение Bluetooth, которое на Android 12+ заменено на `BLUETOOTH_CONNECT`:

```xml
    <uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30" />
    <uses-permission android:name="android.permission.BLUETOOTH_CONNECT" />
```

- [ ] **Step 5: Собрать APK и проверить**

```bash
cd android && ./gradlew assembleDebug --no-daemon 2>&1 | tail -3
```

Ожидается: `BUILD SUCCESSFUL`

- [ ] **Step 6: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth
git add Cargo.toml android/build_rust.sh android/app/src/main/AndroidManifest.xml .gitignore
git commit -m "build: keep symbol names for readable native crash logs

strip=symbols made every Android crash in logcat an anonymous address.
Also declares allowBackup explicitly and caps legacy BLUETOOTH at API 30."
```

---

## Phase 6 — Тесты

### Task 22: Показать метрики здоровья аудиотракта

Раздел «чего не хватает», наблюдаемость. Сейчас нет ни счётчика xrun, ни CPU-метра, ни индикации потерянных событий — единственный признак проблемы в том, что пользователь слышит треск.

**Files:**
- Modify: `src/audio.rs`
- Modify: `src/gui/mod.rs`

- [ ] **Step 1: Измерять время callback**

В `src/audio.rs` в начале тела callback:

```rust
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let started = std::time::Instant::now();
```

и в самом конце тела, после закрытия `rt_guard`:

```rust
                // Instant::now is a vDSO read on Linux and Android — no syscall,
                // safe to call from the callback.
                metrics_cb.note_callback_time(started.elapsed().as_micros() as u32);
            },
```

`metrics_cb` — клон `SharedMetrics`, захваченный так же, как `paused_cb`.

- [ ] **Step 2: Считать ошибки стрима**

В обработчике ошибок:

```rust
            move |err| {
                metrics_err.stream_errors.fetch_add(1, Ordering::Relaxed);
                let msg = err.to_string();
```

- [ ] **Step 3: Вывести показатели в заголовок окна**

В `src/gui/mod.rs` в правой части верхней панели было:

```rust
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!(
                        "{} | {}Hz | buf {}",
                        self.current_host, self.sample_rate, self.config.audio.buffer_size
                    ));
```

Стало:

```rust
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // Callback budget: one buffer at the current rate. Anything
                    // approaching 100% will glitch.
                    let budget_us =
                        self.config.audio.buffer_size as f32 / self.sample_rate as f32 * 1e6;
                    let worst_us = self.metrics.take_worst_callback_us() as f32;
                    let load = if budget_us > 0.0 { worst_us / budget_us * 100.0 } else { 0.0 };
                    self.dsp_load = self.dsp_load * 0.8 + load * 0.2;

                    let dropped = self.metrics.midi_dropped.load(Ordering::Relaxed)
                        + self.metrics.ctrl_dropped.load(Ordering::Relaxed);
                    let errors = self.metrics.stream_errors.load(Ordering::Relaxed);

                    if dropped > 0 || errors > 0 {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 80, 80),
                            format!("drops {dropped} | errs {errors}"),
                        )
                        .on_hover_text(
                            "Events the engine never received, and audio stream errors. \
                             Non-zero means something is wrong — increase the buffer size \
                             or report it.",
                        );
                        ui.separator();
                    }

                    ui.label(format!(
                        "{} | {}Hz | buf {} | dsp {:.0}%",
                        self.current_host,
                        self.sample_rate,
                        self.config.audio.buffer_size,
                        self.dsp_load,
                    ));
```

Добавить поля в `gui::App`:

```rust
    pub metrics: crate::metrics::SharedMetrics,
    /// Smoothed DSP load percentage, so the number in the header does not flicker.
    pub dsp_load: f32,
```

и инициализировать в `src/lib.rs`: `metrics: c.metrics.clone(), dsp_load: 0.0,`.

- [ ] **Step 4: Проверить визуально**

```bash
cargo run --release
```

В правом верхнем углу должно появиться `dsp N%`. Значение при простое — единицы процентов, при игре аккордами с реверберацией — десятки. Если показывает 0% всегда — `note_callback_time` не вызывается, проверь, что `metrics_cb` действительно захвачен в замыкание.

- [ ] **Step 5: Commit**

```bash
git add src/audio.rs src/gui/mod.rs src/lib.rs
git commit -m "feat: show DSP load, dropped events and stream errors in the header

The only previous indication that the audio path was in trouble was the
user hearing crackle."
```

---

### Task 23: Fuzz-таргеты для парсеров бинарных форматов

Раздел «чего не хватает», тесты. `parse_wt`, `import_midi_drums` и загрузка SF2 читают файлы из недоверенных источников — пользователь скачивает `.sf2` из интернета и кладёт в Download.

**Files:**
- Create: `fuzz/Cargo.toml`
- Create: `fuzz/fuzz_targets/parse_wt.rs`
- Create: `fuzz/fuzz_targets/import_midi_drums.rs`
- Modify: `.gitignore`

- [ ] **Step 1: Инициализировать**

```bash
cd /home/dima/Projects/mini_midi_synth
cargo install cargo-fuzz --locked 2>&1 | tail -2
cargo fuzz init 2>&1 | tail -3
```

Это создаст `fuzz/` с шаблонным таргетом. Удали шаблон:

```bash
rm fuzz/fuzz_targets/fuzz_target_1.rs
```

- [ ] **Step 2: Написать таргет для wavetable**

Создать `fuzz/fuzz_targets/parse_wt.rs`:

```rust
#![no_main]
//! Surge .wt wavetable files are downloaded by users and dropped into the
//! wavetable directory; the parser must not panic or read out of bounds on
//! anything it is handed.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Returning None on garbage is correct behavior; panicking is not.
    let _ = mini_midi_synth::preset::parse_wt(data);
});
```

- [ ] **Step 3: Написать таргет для MIDI-импорта**

Создать `fuzz/fuzz_targets/import_midi_drums.rs`:

```rust
#![no_main]
//! Drum pattern import parses arbitrary .mid files chosen by the user.

use libfuzzer_sys::fuzz_target;
use std::io::Write;

fuzz_target!(|data: &[u8]| {
    // import_midi_drums takes a path, so the input has to hit the filesystem.
    // A single reused temp file keeps this fast enough to be worth running.
    let path = std::env::temp_dir().join("mms-fuzz-drums.mid");
    if std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(data))
        .is_err()
    {
        return;
    }
    let _ = mini_midi_synth::preset::import_midi_drums(&path);
});
```

- [ ] **Step 4: Прописать таргеты**

В `fuzz/Cargo.toml` секции `[[bin]]`:

```toml
[[bin]]
name = "parse_wt"
path = "fuzz_targets/parse_wt.rs"
test = false
doc = false

[[bin]]
name = "import_midi_drums"
path = "fuzz_targets/import_midi_drums.rs"
test = false
doc = false
```

- [ ] **Step 5: Дать затравку фаззеру**

Корпус из настоящих файлов ускоряет поиск в разы:

```bash
mkdir -p fuzz/corpus/parse_wt
if [ -d ~/.local/share/mini_midi_synth/wavetables ]; then
  find ~/.local/share/mini_midi_synth/wavetables -name "*.wt" -exec cp {} fuzz/corpus/parse_wt/ \; 2>/dev/null
fi
ls fuzz/corpus/parse_wt | wc -l
```

Если ноль — не страшно, фаззер найдёт магию `vawt` сам, просто дольше.

- [ ] **Step 6: Прогнать по минуте на таргет**

```bash
cargo +nightly fuzz run parse_wt -- -max_total_time=60 2>&1 | tail -20
cargo +nightly fuzz run import_midi_drums -- -max_total_time=60 2>&1 | tail -20
```

Ожидается: `Done ... runs` без падений. Если падение найдено — это настоящий баг, минимизируй (`cargo fuzz tmin <target> <artifact>`), почини и добавь найденный вход как обычный юнит-тест.

Если `cargo +nightly` недоступен: `rustup toolchain install nightly`.

- [ ] **Step 7: Не коммитить артефакты**

В `.gitignore`:

```
fuzz/target/
fuzz/artifacts/
fuzz/coverage/
```

Корпус (`fuzz/corpus/`) **коммитится** — он ценность, накопленная фаззером.

- [ ] **Step 8: Commit**

```bash
git add fuzz/Cargo.toml fuzz/fuzz_targets fuzz/corpus .gitignore
git commit -m "test: fuzz the wavetable and MIDI drum parsers

Both read files the user downloads from the internet. Neither had any
test coverage against malformed input."
```

---

### Task 24: Интеграционные тесты полного тракта

Раздел «чего не хватает», тесты. Не проверяется ни один сценарий уровня «загрузить пресет → нота → есть звук → NoteOff → тишина».

**Files:**
- Create: `tests/end_to_end.rs`

- [ ] **Step 1: Написать тесты**

```rust
//! Whole-engine behaviour: control events in, audio out.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

/// Renders `blocks` blocks and returns the peak absolute sample.
fn render_peak(engine: &mut SynthEngine, blocks: usize) -> f32 {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
    }
    peak
}

fn engine_with_first_patch() -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches.first().expect("factory presets are compiled in").clone();
    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);
    let _ = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = engine.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    engine
}

#[test]
fn note_on_produces_sound_and_note_off_stops_it() {
    let mut engine = engine_with_first_patch();

    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let sounding = render_peak(&mut engine, 100);
    assert!(sounding > 0.001, "NoteOn produced silence (peak {sounding})");

    engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    // Long enough for any reasonable release stage plus reverb tail.
    let _tail = render_peak(&mut engine, 4000);
    let after = render_peak(&mut engine, 200);
    assert!(after < 0.001, "sound continued after NoteOff (peak {after})");
}

#[test]
fn all_notes_off_silences_a_held_chord() {
    let mut engine = engine_with_first_patch();

    for note in [60u8, 64, 67, 72] {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    }
    assert!(render_peak(&mut engine, 100) > 0.001, "chord produced silence");

    let _ = engine.handle_control(ControlEvent::AllNotesOff);
    let _tail = render_peak(&mut engine, 4000);
    let after = render_peak(&mut engine, 200);
    assert!(after < 0.001, "AllNotesOff left sound behind (peak {after})");
}

#[test]
fn cc_123_from_the_wire_silences_a_held_chord() {
    let mut engine = engine_with_first_patch();

    for note in [60u8, 64, 67] {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    }
    assert!(render_peak(&mut engine, 100) > 0.001);

    // Exactly what a controller's panic button sends.
    let (event, _) = mini_midi_synth::midi::parse_midi_message(&[0xB0, 123, 0])
        .expect("CC 123 must parse");
    engine.handle_event(event);

    let _tail = render_peak(&mut engine, 4000);
    let after = render_peak(&mut engine, 200);
    assert!(after < 0.001, "CC 123 did not stop the notes (peak {after})");
}

#[test]
fn disabled_part_is_silent() {
    let mut engine = engine_with_first_patch();
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: false });

    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let peak = render_peak(&mut engine, 200);
    assert!(peak < 0.001, "disabled part produced sound (peak {peak})");
}

#[test]
fn engine_output_never_goes_out_of_range() {
    let mut engine = engine_with_first_patch();

    // Every note at once, worst case for summing.
    for note in 24..96u8 {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    }

    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..500 {
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            assert!(l[i].is_finite() && r[i].is_finite(), "engine emitted NaN or inf");
        }
    }
}
```

- [ ] **Step 2: Запустить**

```bash
cargo test --test end_to_end 2>&1 | tail -20
```

Ожидается: 5 тестов `ok`. Если `note_on_produces_sound_and_note_off_stops_it` падает на пороге тишины — посмотри, какой пресет первый в списке (`init`); у него может быть длинный релиз или включённый реверб. Увеличь число блоков в `_tail` или выбери пресет по имени `init` явно вместо `first()`.

- [ ] **Step 3: Commit**

```bash
git add tests/end_to_end.rs
git commit -m "test: cover the full control-event to audio path

Nothing verified that a NoteOn actually makes sound or that NoteOff,
AllNotesOff and CC 123 actually stop it."
```

---

### Task 25: Бенчмарк DSP

Раздел «чего не хватает», тесты. Целевые платформы — Orange Pi 5 и телефон; регресс по CPU на 30% сейчас обнаружится только на сцене.

**Files:**
- Create: `benches/dsp.rs`
- Modify: `Cargo.toml`

- [ ] **Step 1: Добавить зависимость и цель**

В `Cargo.toml`:

```toml
[dev-dependencies]
criterion = { version = "0.5", default-features = false, features = ["cargo_bench_support"] }

[[bench]]
name = "dsp"
harness = false
```

- [ ] **Step 2: Написать бенчмарк**

Создать `benches/dsp.rs`:

```rust
//! Measures the cost of one audio block under realistic load. The number that
//! matters is time per block against the buffer budget: at 48 kHz with a
//! 256-frame buffer the callback has 5333 µs, and tick_block is called
//! 256 / BLOCK_SIZE times per callback.

use criterion::{criterion_group, criterion_main, Criterion};
use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

fn engine_with(patch_name: &str, voices: usize) -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name == patch_name)
        .unwrap_or_else(|| patches.first().expect("factory presets exist"))
        .clone();

    let mut engine = SynthEngine::new(48_000.0);
    engine.set_patches(patches);
    let _ = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });

    for i in 0..voices {
        engine.handle_event(MidiEvent::NoteOn {
            channel: 0,
            note: 48 + i as u8,
            velocity: 100,
        });
    }
    engine
}

fn bench_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_block");

    for &voices in &[1usize, 8, 16] {
        let mut engine = engine_with("Init", voices);
        let mut l = [0.0f32; BLOCK_SIZE];
        let mut r = [0.0f32; BLOCK_SIZE];

        group.bench_function(format!("init_{voices}_voices"), |b| {
            b.iter(|| engine.tick_block(&mut l, &mut r));
        });
    }

    // A patch that exercises the effect chain, not just the oscillator.
    let mut engine = engine_with("Ambient Pad", 8);
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    group.bench_function("ambient_pad_8_voices", |b| {
        b.iter(|| engine.tick_block(&mut l, &mut r));
    });

    group.finish();
}

criterion_group!(benches, bench_block);
criterion_main!(benches);
```

- [ ] **Step 3: Запустить и записать базовую линию**

```bash
cargo bench --bench dsp 2>&1 | grep -E "time:|Benchmarking .* analysis"
```

Ожидается: несколько строк с временем. Запиши их — это точка отсчёта. Одно значение `tick_block` при `BLOCK_SIZE` семплов должно быть заметно меньше, чем `BLOCK_SIZE / 48000 * 1e6` микросекунд, иначе движок не успевает в реальном времени даже без запаса.

- [ ] **Step 4: Задокументировать базовую линию**

Добавить в `docs/ARCHITECTURE.md` (создаётся в Task 27) раздел «Performance baseline» с датой, моделью процессора (`lscpu | grep "Model name"`) и полученными числами.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml benches/dsp.rs
git commit -m "test: add criterion benchmarks for tick_block

A 30% DSP regression was previously only discoverable on stage."
```

---

## Phase 7 — Документация

### Task 26: CHANGELOG и CLAUDE.md

**Files:**
- Create: `CHANGELOG.md`
- Create: `CLAUDE.md`

- [ ] **Step 1: Создать CHANGELOG.md**

```markdown
# Changelog

Формат — [Keep a Changelog](https://keepachangelog.com/ru/1.1.0/),
версионирование — [SemVer](https://semver.org/lang/ru/).

## [Unreleased]

### Исправлено
- Клон репозитория снова собирается: `vendor/cpal` больше не игнорируется
- Аудиопоток больше не освобождает память: отработанные события и саундфонты
  уходят в очередь отложенного освобождения
- Устранён use-after-free при отключении USB-MIDI во время игры
- Управляющие события применяются и когда приложение свёрнуто
- BLE-реконнект на Android больше не создаёт дубликаты мостов и не размножает ноты
- Переподключение аудиоустройства восстанавливает всё состояние, а не только патчи
- CC 120 (All Sound Off) и CC 123 (All Notes Off) доходят до движка
- Имена пресетов санитизируются перед использованием в качестве имён файлов
- Конфигурация и пресеты пишутся атомарно; нечитаемый конфиг сохраняется как `.bad`
- Недоступные каталоги SF2 на Android сообщаются пользователю

### Добавлено
- Счётчики потерянных событий, ошибок стрима и загрузки DSP в заголовке окна
- CI: fmt, clippy, тесты, cargo-deny, сборка Android APK
- Фича `rt-assert` — диагностический аллокатор для аудиопотока
- Fuzz-таргеты для парсеров `.wt` и `.mid`
- Интеграционные тесты полного тракта и бенчмарки DSP
- Лицензия MIT

### Изменено
- Логика вынесена в библиотечный крейт; `src/main.rs` — трёхстрочная обёртка
- Релизная подпись Android читается из `~/.gradle/gradle.properties`
- `strip = "debuginfo"` вместо `symbols` — нативные краши читаемы
```

- [ ] **Step 2: Создать CLAUDE.md**

```markdown
# mini_midi_synth

Полифонический синтезатор на Rust: Linux (x86_64, aarch64) и Android.
32k строк, из них ~22k — DSP в `src/synth/`.

## Раскладка

- `src/lib.rs` — точки входа: `desktop_main`, `run_gui`, `run_headless`,
  `android_main`, JNI-функции. Вся инициализация в `init_common`.
- `src/main.rs` — трёхстрочная обёртка над `desktop_main`. Не добавляй сюда логику.
- `src/synth/` — движок. `mod.rs` — `SynthEngine`, диспетчеризация событий,
  `tick_block`. Остальное — осцилляторы, фильтры, эффекты.
- `src/gui/` — egui. `mod.rs` — `App` и главный цикл, `settings.rs` — переподключение
  аудио и MIDI, `params.rs` — редактор параметров.
- `vendor/` — cpal (правка: AAudio LowLatency + Exclusive), rustysynth, mi-plaits-dsp.
  Правки вендоров не откатывать при обновлении.

## Инварианты аудиопотока

Всё, что вызывается из `AudioBackend::new` → `build_output_stream`, обязано быть
real-time safe:

- Никаких аллокаций и освобождений. Heap-объекты уходят через `src/trash.rs`
  в GUI-поток; `handle_control` возвращает отработанное событие, его нельзя игнорировать.
- Никаких мьютексов. `try_lock` не спасает: `unlock` — это futex-syscall.
  Оставшееся место — `LooperDisplay`, помечено как долг.
- Никакого ввода-вывода, `format!`, `println!`.
- Проверяется: `cargo test --features rt-assert --test rt_safety`.
  Запускай это после любой правки в `src/synth/` или `src/audio.rs`.

## Общение с движком

GUI → движок: `rtrb` кольцо `ControlEvent` (256 слотов). Отправлять через
`App::send_ctrl`, а не `let _ = ctrl_tx.push(...)` — иначе потери не считаются.

Движок → GUI: кольцо `ParamFeedback` плюс атомарные переменные для дешёвых
величин (позиция лупера, шаг секвенсора, состояние воспроизведения).

MIDI → движок: кольцо `MidiEvent`, наполняется из callback midir, из JNI
(Android BLE) и прямо в аудиопотоке из AMidi (Android USB).

## Правила про пресеты

- Никогда не перезаписывай существующий пресет — создавай новый с суффиксом `_v2`.
- Клавиатура пользователя (SMK-37 Pro) не передаёт velocity: она всегда 127.
  Не делай тембр зависящим от velocity — используй brightness/макросы.
- Иерархия: Preset → Part → Split → Performance.

## Проверки перед коммитом

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

## Android

Сборка: `./android/build_rust.sh && (cd android && ./gradlew assembleDebug)`.
Логи: `adb logcat -s MiniMidiSynth`.
Символизация краша: `adb logcat | ndk-stack -sym android/symbols/arm64-v8a`.
```

- [ ] **Step 3: Commit**

```bash
git add CHANGELOG.md CLAUDE.md
git commit -m "docs: add changelog and project context for agents"
```

---

### Task 27: Архитектурная документация

Раздел «чего не хватает», документация. Нет описания того, кто в каком потоке живёт и как проходит событие от MIDI-порта до семпла.

**Files:**
- Create: `docs/ARCHITECTURE.md`

- [ ] **Step 1: Написать документ**

```markdown
# Архитектура

## Потоки

| Поток | Кто создаёт | Что делает | Ограничения |
|---|---|---|---|
| Главный (GUI) | ОС | egui, чтение конфигурации, загрузка SF2 и пресетов, освобождение мусора из аудиопотока | Может всё |
| Аудио | cpal | `AudioBackend` callback: применяет события, вызывает `tick_block`, лимитирует, пишет в буфер устройства | Real-time safe, см. ниже |
| MIDI (midir) | midir | Парсит байты, обновляет `NoteState`, кладёт `MidiEvent` в кольцо | Real-time safe |
| Клавиатура (evdev) | `input::spawn_keyboard_thread` | epoll на `/dev/input/event*`, шлёт `KeyAction` через mpsc | Обычный |
| Android main looper | ART | `SynthActivity`: BLE и USB MIDI, жизненный цикл | Обычный |
| JNI-вызовы | ART | `onMidiData` парсит и кладёт в кольцо | Как MIDI-поток |

## Путь события: от нажатия клавиши до семпла

1. **USB на Linux.** midir получает байты → `midi::parse_and_push` →
   `parse_midi_message` даёт `MidiEvent` → обновляется `NoteState` для подсветки →
   `try_lock` на `SharedMidiTx` → `push` в кольцо. Потери считаются в `Metrics`.
2. **USB на Android.** Java `SynthActivity` открывает порты, отдаёт их в
   `AmidiPort` через JNI. Аудиопоток сам опрашивает `AMidiOutputPort_receive`
   внутри callback — минимальная задержка, событие применяется в том же блоке.
3. **BLE на Android.** `MidiBridge` (Java `MidiReceiver`) → JNI `onMidiData` →
   тот же `parse_and_push`.
4. **Аудиопоток** в начале блока вычерпывает `ctrl_rx` и `midi_rx`, применяет
   события к `SynthEngine`, затем `tick_block` рендерит `BLOCK_SIZE` семплов.
5. **Выход** проходит `soft_limit` (tanh-подобное насыщение выше 0.8) и
   раскладывается по каналам устройства.

## Инварианты аудиопотока

Правила из [Real-time audio programming 101](http://www.rossbencina.com/code/real-time-audio-programming-101-time-waits-for-nothing):
никаких аллокаций, блокировок, ввода-вывода и вызовов с неограниченным худшим
временем.

Как это обеспечено здесь:

- **Память.** `tick_block` не аллоцирует: буферы блоков лежат на стеке,
  `layer_mods` живёт на движке. Heap-объекты, приходящие с событиями
  (`Box<LoadPatchEvent>`, `Arc<SoundFont>`), возвращаются из `handle_control`
  и уходят в `trash::TrashTx`, откуда их забирает GUI-поток.
- **Блокировки.** Обмен только через `rtrb` (SPSC, wait-free) и атомики.
  Единственное исключение — `Mutex<LooperDisplay>` в `looper::update_display`:
  вектор преаллоцирован, `try_lock` не блокирует, но `unlock` остаётся futex-вызовом.
  Долг: заменить на тройной буфер.
- **Проверка.** `cargo test --features rt-assert --test rt_safety` подключает
  `assert_no_alloc` как глобальный аллокатор и роняет тест при любой
  аллокации или освобождении в размеченном участке.

## Формат пресета

JSON: `name`, `category`, `params` (плоская карта `String → f32`),
опционально `wavetable_file`. Единственный источник истины о наборе параметров —
`PatchParams::from_map` в `src/synth/patch_params.rs`. Поля с суффиксом `_mix`
равные нулю означают выключенный эффект — движок пропускает его целиком
(`if fx.X_mix > 0.001`), так что нулевой mix не стоит ничего.

Фабричные пресеты вкомпилированы через `include_str!` в списке
`FACTORY_PRESETS` (`src/preset.rs`). Добавляя пресет, добавь и строку в этот
список — иначе файл окажется в каталоге, но не в сборке.

## Как добавить эффект

1. Новый модуль в `src/synth/`, структура с `new(sample_rate)`, `set_sample_rate`
   и `tick(l, r, ...параметры...) -> (f32, f32)`.
2. Поле в `SynthEngine`, инициализация в `new`, проброс в `set_sample_rate`.
3. Параметры в `PatchParams` (`patch_params.rs`) с чтением в `from_map` и
   записью в `to_map`.
4. Вызов в цепочке эффектов `tick_block` по образцу соседей: `if fx.X_mix > 0.001`.
5. Слайдеры в `src/gui/params.rs`.
6. Тест в `src/synth/tests.rs`: выход конечен и в диапазоне при экстремальных
   параметрах.

## Performance baseline

Заполняется по результатам Task 25.

| Дата | Процессор | Сценарий | Время на блок |
|---|---|---|---|
| | | | |
```

- [ ] **Step 2: Заполнить baseline реальными числами**

```bash
lscpu | grep "Model name"
cargo bench --bench dsp 2>&1 | grep "time:"
```

Впиши полученные значения в таблицу.

- [ ] **Step 3: Сослаться на документ из README**

Добавить в `README.md` после раздела с возможностями:

```markdown
## Разработка

- [Архитектура](docs/ARCHITECTURE.md) — потоки, инварианты аудиопотока, формат пресета
- [CHANGELOG](CHANGELOG.md)
```

- [ ] **Step 4: Commit**

```bash
git add docs/ARCHITECTURE.md README.md
git commit -m "docs: describe threads, audio-thread invariants and preset format"
```

---

## Self-Review

Проверка плана против аудита.

**Покрытие пунктов аудита:**

| Пункт | Задача |
|---|---|
| 1. vendor/cpal не в git | Task 1 |
| 2. Use-after-free в AMidi | Task 10 |
| 3. Освобождение памяти в аудиопотоке | Task 8, Task 9 |
| 4. Пауза теряет события | Task 11 |
| 5. Утечка BLE-мостов | Task 13 |
| 6. Reconnect сбрасывает состояние | Task 14 |
| 7. CC 120/123 игнорируются | Task 15 |
| 8. /sdcard/Download не читается | Task 19 |
| 9. Потеря MIDI без диагностики | Task 16 |
| 10. Path traversal в именах пресетов | Task 17 |
| 11. Неатомарная запись конфига | Task 18 |
| 12. push в аудиопотоке (looper) | Task 12 |
| 13. Нет CI | Task 6 |
| 14. Структура крейта | Task 3, Task 4 |
| 15. Release подписан debug-ключом | Task 20 |
| 16. Нет лицензии | Task 7 |
| 17. Нет rustfmt/clippy/deny/audit | Task 5, Task 7 |
| 18. Артефакты сборки в git | Task 1 |
| 19. strip=symbols | Task 21 |
| Механизмы (assert_no_alloc, basedrop, RTSan) | Task 8 |
| Наблюдаемость (xrun, CPU, дропы) | Task 16, Task 22 |
| Тесты (интеграционные, fuzz, бенчмарки) | Task 23, Task 24, Task 25 |
| Инфраструктура (toolchain, CHANGELOG, CLAUDE.md) | Task 2, Task 26 |
| Документация (архитектура, формат пресета) | Task 27 |

**Не покрыто намеренно** — крупные подсистемы из раздела «чего не хватает»:
аудиовход, MIDI Out, экспорт WAV, CLAP/VST3-плагин, микротюнинг и MPE,
поддержка Windows/macOS. Каждая требует собственного плана; см. ниже.

**Проверка согласованности типов:** `handle_control` меняет сигнатуру в Task 9
на `-> Option<ControlEvent>`, и все последующие задачи (11, 12, 15, 24, 25)
используют уже новую форму с `let _ =` или обработкой возврата. `AudioShared`
вводится в Task 4 и расширяется полем `trash` в Task 9 и `metrics` в Task 22 —
порядок задач это учитывает. `scan_sf2_files` меняет тип возврата в Task 19,
её единственные вызовы перечислены там же.

---

## Отдельные планы

Эти пункты аудита — не дефекты, а отсутствующие подсистемы. Каждая по объёму
сопоставима со всем планом выше, поэтому в него не входит.

| Подсистема | Почему отдельно | Грубая оценка |
|---|---|---|
| **Аудиовход** | Второй cpal-стрим, синхронизация с выходным, разрешение на микрофон на Android, изменение сигнатуры всей цепочки эффектов. Разблокирует настоящий вокодер — сейчас он подаёт себе на модулятор собственный выход (`synth/mod.rs:2469`) | 3-5 дней |
| **MIDI Out** | Новый исходящий поток, MIDI-клок, маршрутизация от лупера и драм-машины | 2-3 дня |
| **Экспорт WAV** | Офлайн-рендер лупера, кодирование, выбор пути; на Android — SAF | 2 дня |
| **CLAP/VST3 через nih-plug** | Движок уже отделён от GUI через `ControlEvent`, но параметры надо переописать в декларативной системе nih-plug, а egui-интерфейс перенести на их адаптер | 1-2 недели |
| **Микротюнинг (Scala) и MPE** | Таблица частот вместо `440 * 2^((n-69)/12)` во всех осцилляторах, per-note pitch bend в голосах | 1 неделя |
| **Windows/macOS** | Сборка проходит, но никто не проверял; evdev-путь только для Linux, ASIO/CoreAudio не тестировались | Неизвестно до первой попытки |
```
