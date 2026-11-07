# mini_midi_synth

Полифонический синтезатор на Rust: Linux (x86_64, aarch64) и Android.
~32k строк, из них ~22k — DSP в `src/synth/`.

## Раскладка

- `src/lib.rs` — точки входа: `desktop_main`, `run_gui`, `run_headless`,
  `android_main`, JNI-функции. Вся инициализация в `init_common`.
- `src/main.rs` — трёхстрочная обёртка над `desktop_main`. Не добавляй сюда логику.
- `src/synth/` — движок. `mod.rs` — `SynthEngine`, диспетчеризация событий,
  `tick_block`. Остальное — осцилляторы, фильтры, эффекты.
- `src/gui/` — egui. `mod.rs` — `App` и главный цикл, `settings.rs` — переподключение
  аудио и MIDI, `params.rs` — редактор параметров.
- `src/trash.rs` — очередь отложенного освобождения (см. инварианты ниже).
- `src/metrics.rs` — счётчики здоровья аудиотракта.
- `vendor/` — cpal (правка: AAudio LowLatency + Exclusive в `src/host/oboe/mod.rs`),
  rustysynth, mi-plaits-dsp. Правки вендоров не откатывать при обновлении.

## Инварианты аудиопотока

Всё, что вызывается из `AudioBackend::new` → `build_output_stream`, обязано быть
real-time safe:

- **Никаких аллокаций и освобождений.** Heap-объекты уходят через `src/trash.rs`
  в GUI-поток. `handle_control` возвращает `Option<ControlEvent>` — возврат помечен
  `#[must_use]`, игнорировать его нельзя.
- **Никаких мьютексов.** `try_lock` не спасает: `unlock` — это futex-syscall.
  Оставшееся место — `LooperDisplay`, помечено как долг.
- **Никакого ввода-вывода**, `format!`, `println!`.
- **Проверяется:** `cargo test --features rt-assert --test rt_safety`.
  Запускай после любой правки в `src/synth/` или `src/audio.rs` — тест роняет
  процесс при первой же аллокации в защищённой зоне.

Известный долг: `Synthesizer::new` при загрузке SF2 и построение списка треков в
`MidiPlayer::load` всё ещё аллоцируют в callback'е.

## Обмен между потоками

GUI → движок: кольцо `rtrb` с `ControlEvent` (512 слотов), обёрнутое в
`metrics::CountingProducer` — потери считаются автоматически, поэтому привычное
`let _ = ctrl_tx.push(..)` безопасно оставлять.

Движок → GUI: кольцо `ParamFeedback` плюс атомики для дешёвых величин (позиция
лупера, шаг секвенсора, состояние воспроизведения).

MIDI → движок: кольцо `MidiEvent`, наполняется из callback midir, из JNI
(Android BLE) и прямо в аудиопотоке из AMidi (Android USB).

## Правила про пресеты

- Никогда не перезаписывай существующий пресет — создавай новый с суффиксом `_v2`.
- Клавиатура пользователя (SMK-37 Pro) не передаёт velocity: она всегда 127.
  Не делай тембр зависящим от velocity — используй brightness/макросы.
- Иерархия: Preset → Part → Split → Performance.
- Новый фабричный пресет надо добавить и в `FACTORY_PRESETS` в `src/preset.rs`,
  иначе файл будет в каталоге, но не в сборке.

## Проверки перед коммитом

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```

`cargo fmt` по всей базе **не** запускать: код написан в компактном стиле,
переформатирование даёт диff на 72 файла и разворачивает смысловые группировки
параметров в `patch_params.rs`. `rustfmt.toml` есть для новых файлов.

## Android

```bash
./android/build_rust.sh
(cd android && ./gradlew assembleDebug)
```

Логи: `adb logcat -s MiniMidiSynth`.
Символизация краша: `adb logcat | ndk-stack -sym android/symbols/arm64-v8a`.

## Гайдлайны рефакторинга (выстрадано 2026-11-07/08, 12 PR)

1. **Golden-канон.** Behavior-preserving изменение обязано держать goldens
   (`tests/golden/`) байт-в-байт. Красный golden = найденный баг — СТОП и
   разбор, НЕ регенерация. `MINI_MIDI_GOLDEN_UPDATE=1` — только для
   осознанного изменения звука + ревью диффа.
2. **Bit-identity в DSP.** Копии примитивов сливать только при идентичном
   порядке операций. ULP-сдвиг в feedback-петлях накапливается. Формула
   отличается (clamp/wrap/порядок множителей, tuple-state) — НЕ сливать
   (кейсы: bonsai, combulator/floaty, cubic-flanger).
3. **SSOT параметров.** CC-параметр добавляется одной строкой в
   `cc_patch_params!` (patch_params.rs) — getter/setter/ranges генерируются.
   Один ключ может иметь РАЗНЫЕ диапазоны в разных движках (ks_brightness:
   KarplusStrong vs PianoModel) — это не дрейф, пин `tc_ks_ranges`.
4. **RT-контракт.** Box-события не распаковывать в аудио-колбэке (free на
   RT-потоке): old payload возвращается в событии (canon LoadPatch), new — в
   trash. Мульти-событийный жест GUI = одна транзакция (canon SetPartFull).
5. **Персист.** Запись конфига — только `flush_config` (dirty + 2 с дебаунс,
   force на exit). Прямые `config.save()` в gui запрещены. Краш теряет ≤2 с
   настроек — осознанный трейд-офф.
6. **Глифы.** Иконки — только из emoji-icon-font, проверить cmap (fontTools)
   до вставки. Тонкие стрелки ↑↓, ⠿, ▼, ✓ в шрифтах egui НЕ существуют.
7. **Рефактор-процесс.** План (docs/superpowers/plans/, status-заголовок) →
   ревью плана субагентом → реализация: implementer + spec review + quality
   review на задачу → финальный ревью ветки → PR squash (локальный merge в
   master запрещён). Move-only шаги: goldens байт-стабильны = доказательство.
8. **Пины.** Source-parse пины (match_arms_between / include_str) переезжают
   вместе с кодом: меняется путь, не семантика assert'а. Полный rewrite
   пинов — осознанно и с комментарием.
9. **Панели.** Порядок egui-панелей пинится из исходника
   (ui_layout_contract): при разрезе gui — панели остаются в update() ЛИБО
   расширить список файлов в пине; CentralPanel — последней.
