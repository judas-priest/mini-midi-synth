# Задача: сделать mini_midi_synth кроссплатформенным (Linux x86_64, Linux aarch64, Android)

## Контекст

### Что такое mini_midi_synth
Полифонический синтезатор на Rust с драм-машиной, MIDI-лупером и live performance workflow. Сейчас работает на Linux (x86_64). GUI на egui (eframe, glow backend). Аудио через cpal (PipeWire/ALSA). MIDI вход через midir (ALSA backend).

### Структура проекта
```
src/
├── main.rs          — точка входа, инициализация аудио/MIDI/GUI
├── audio.rs         — cpal аудио pipeline
├── midi.rs          — MIDI вход (midir)
├── input.rs         — обработка ввода
├── config.rs        — конфигурация
├── cc_map.rs        — маппинг MIDI CC
├── key_action.rs    — действия клавиш
├── preset.rs        — пресеты
├── synth/           — ядро синтезатора (50+ файлов)
│   ├── mod.rs       — основной синт-движок
│   ├── oscillator.rs — 20+ алгоритмов осцилляторов
│   ├── filter.rs    — 35+ типов фильтров
│   ├── fx_chain.rs  — цепочка эффектов (30+ модулей)
│   ├── drum.rs      — драм-машина
│   ├── looper.rs    — MIDI лупер
│   ├── arpeggiator.rs — арпеджиатор
│   ├── mod_matrix.rs — модуляционная матрица
│   ├── sampler.rs   — SF2 Soundfont (Rustysynth)
│   └── ...          — эффекты, LFO, MSEG, envelope и т.д.
└── gui/             — egui интерфейс (11 файлов)
    ├── mod.rs       — основное GUI окно
    ├── keyboard.rs  — экранная клавиатура
    ├── drums.rs     — UI драм-машины
    ├── params.rs    — параметры синта
    ├── fx_chain.rs  — UI эффектов
    ├── theme.rs     — тема/стили
    └── ...
```

### Зависимости (Cargo.toml)
- `cpal 0.15` (features: jack) — аудио
- `midir 0.10` — MIDI вход
- `wmidi 4` — MIDI парсинг
- `eframe 0.31` (glow, x11, wayland, persistence) — GUI
- `rustysynth` (vendored) — SF2 Soundfont
- `mi-plaits-dsp` (vendored) — Mutable Instruments алгоритмы
- `rtrb` — lock-free ring buffer
- `serde/serde_json` — сериализация
- `midly` — MIDI файлы
- `libc` — suppress ALSA errors

Feature flags: `gui` (default) — без него headless режим.

## Целевые платформы

### 1. Linux x86_64 (основная, уже работает)
- **Устройство**: Arch Linux PC
- **Аудио**: PipeWire (через cpal ALSA backend)
- **MIDI**: USB MIDI клавиатура M-VAVE SMK-37 PRO (midir, ALSA)
- **GUI**: eframe (X11/Wayland, glow)

### 2. Linux aarch64 (Orange Pi 5)
- **Устройство**: Orange Pi 5, Armbian (Debian-based), aarch64
- **Аудио**: ALSA (через cpal)
- **MIDI**: USB MIDI (midir, ALSA)
- **GUI**: eframe (X11, glow) или headless
- **Таргет**: `aarch64-unknown-linux-gnu`

### 3. Android aarch64 (ПРИОРИТЕТ этой задачи)
- **Устройство**: OnePlus Ace 5 Ultra (PLC110, MediaTek Dimensity 9400+, 12GB RAM)
- **ОС**: Android 16, ColorOS 16
- **Аудио**: AAudio (через cpal, низкая латентность)
- **MIDI**: USB MIDI (M-VAVE SMK-37 PRO по Type-C OTG) + Bluetooth MIDI
- **GUI**: eframe Android backend (glow/OpenGL ES)
- **Таргет**: `aarch64-linux-android`

## MIDI-клавиатура: M-VAVE SMK-37 PRO
- 37 клавиш, velocity-sensitive
- 16 RGB пэдов с velocity и aftertouch
- 8 бесконечных энкодеров
- 4 фейдера (расширяемые до 8 через bank)
- Pitch wheel + Mod wheel
- Встроенный DX-7 FM синтезатор
- Секвенсор (8 паттернов, 64 шага)
- Арпеджиатор
- **USB-C** (class compliant) + **Bluetooth MIDI**
- Встроенная батарея
- Совместим с Android из коробки

## Что нужно сделать

### Фаза 1: Рефакторинг — выделить платформо-независимое ядро
1. Разделить `main.rs` на `lib.rs` (ядро) + `main.rs` (Linux entry point)
2. Вынести аудио-инициализацию за trait: `trait AudioBackend`
3. Вынести MIDI за trait: `trait MidiBackend`
4. Synth-ядро (`synth/`) уже платформо-независимое — трогать не нужно
5. GUI (`gui/`) на egui — тоже кроссплатформенное, но нужно проверить Android-совместимость eframe

### Фаза 2: Android сборка
1. Настроить cross-compilation: NDK, cargo-ndk, таргет `aarch64-linux-android`
2. Заменить `cpal` features: убрать `jack`, добавить AAudio/Oboe backend для Android
3. MIDI на Android: `midir` НЕ поддерживает Android. Нужен враппер через `android.media.midi` API (JNI) или найти Rust-crate для Android MIDI
4. eframe Android: проверить `eframe` Android backend (winit + glutin + glow на Android)
5. Android activity entry point вместо `fn main()`
6. Убрать `suppress_alsa_errors()` и другие Linux-specific вещи за `#[cfg(target_os = "linux")]`

### Фаза 3: Адаптация GUI под тач-экран
1. Увеличить элементы управления для пальцев (клавиши, пэды, ручки)
2. Мультитач для полифонии на экранной клавиатуре
3. Учесть разрешение 2800×1272 и 6.83" экран
4. Landscape-ориентация по умолчанию для клавиатуры

### Фаза 4: Bluetooth MIDI
1. Подключение M-VAVE SMK-37 PRO через Bluetooth MIDI
2. Android BLE MIDI API через JNI

## Критичные вопросы для исследования

Перед началом работы **загугли и найди актуальную информацию (2025-2026)** по каждому пункту:

1. **eframe на Android** — текущий статус поддержки, примеры работающих приложений, ограничения. Ищи: `eframe android 2025 2026 example`, `egui android app`, `winit android backend`. Есть ли альтернативы (iced, makepad, slint)?

2. **cpal на Android** — поддержка AAudio/Oboe, латентность, примеры. Ищи: `cpal android aaudio 2025 2026`, `rust audio android low latency`. Если cpal не поддерживает — смотри `oboe-rs` (Rust bindings к Oboe).

3. **MIDI на Android из Rust** — midir не работает на Android. Ищи: `rust android midi usb 2025 2026`, `android.media.midi jni rust`, `ndk-glue midi`. Возможно нужен тонкий JNI-слой на Kotlin/Java, вызывающий `android.media.midi.MidiManager`.

4. **Bluetooth MIDI на Android из Rust** — BLE MIDI. Ищи: `android bluetooth midi rust`, `ble midi android api`. Скорее всего только через JNI к Android API.

5. **cargo-ndk и сборка APK** — актуальный тулчейн. Ищи: `cargo ndk android apk 2025 2026`, `xbuild android rust`, `cargo-apk vs ndk-glue`. Что лучше: `cargo-apk`, `xbuild`, `cargo-ndk` + вручную?

6. **Rust + Android: архитектура** — best practices для Rust-приложений на Android. Ищи: `rust android app architecture 2025`, `rust mobile app production`. Чистый Rust (через winit/eframe) vs Kotlin shell + Rust .so через JNI?

7. **Vendored зависимости** (`rustysynth`, `mi-plaits-dsp`) — совместимость с Android/aarch64. Проверить что нет x86-специфичного кода (SSE/AVX). Если есть — нужны NEON-альтернативы.

8. **Производительность на Dimensity 9400+** — хватит ли для real-time синтеза с эффектами? Спойлер: конечно хватит, но проверь что нет лишних аллокаций в аудио-потоке.

## Ограничения и особенности

- **НЕ использовать Flutter/React Native/etc** — только Rust. Максимум тонкий JNI-слой для Android-специфичных API (MIDI, Bluetooth)
- **Один Cargo workspace** — общий код, платформенные различия через `#[cfg()]` и feature flags
- **Минимум зависимостей** — не тащить тяжёлые фреймворки
- **Латентность критична** — аудио должно быть < 10ms, MIDI < 5ms
- **Оффлайн** — приложение должно работать без интернета
- **Размер APK** — держать разумным (< 50MB)

## Фаза 5: NPU-ускоренные AI-фичи (бонус, после рабочего Android-билда)

### NPU на борту: MediaTek NPU 890 (8-е поколение)
OnePlus Ace 5 Ultra имеет MediaTek Dimensity 9400+ с NPU 890 — самый мощный мобильный NPU MediaTek:
- ~40-50 TOPS INT8 (оценка, MediaTek не публикует точные цифры)
- На 100% быстрее diffusion vs предыдущее поколение
- На 80% быстрее LLM prompt processing
- На 35% энергоэффективнее в AI-задачах
- Поддерживает on-device LoRA training, видеогенерацию, LLM до 33B
- Доступ: **Android NNAPI** (стандартный) или **MediaTek NeuroPilot SDK** (проприетарный)

### Идеи для синта с NPU

**1. Голос → MIDI (pitch detection)**
Пользователь напевает в микрофон → NPU распознаёт pitch в реалтайме → ноты идут в синт-движок. Модель CREPE (~10MB) или PYIN — на NPU inference <1ms на аудио-фрейм. Фактически MIDI-клавиатура голосом.

**2. Neural Amp Modeling / тембр-трансфер**
Простой синт-звук → NPU в реалтайме превращает тембр в гитару/скрипку/орган. Модели DDSP (Google, ~20MB) или NAM (~5MB). Real-time на NPU, на CPU было бы слишком медленно для low-latency.

**3. AI-пресеты голосом/текстом**
"Тёплый аналоговый бас с хорусом и реверберацией" → маленькая LLM (Qwen 0.5B Q4, ~300MB) генерирует JSON-пресет для синта. Можно на NPU телефона или на Orange Pi 5 по сети.

**4. Intelligent FX**
NPU анализирует аудиопоток и адаптивно подкручивает параметры эффектов — авто-EQ, адаптивный компрессор, smart reverb под стиль игры.

### Архитектура доступа к NPU из Rust

```
Rust synth engine
    ↓ JNI
Android Java/Kotlin thin layer
    ↓
NNAPI (android.hardware.neuralnetworks)
    ↓
MediaTek NPU 890
```

- Модели в формате **TFLite** (.tflite) — конвертируются из PyTorch/ONNX
- **NNAPI Delegate** в TFLite автоматически отправляет на NPU
- Из Rust: JNI → Java → TFLite Interpreter с NNAPI delegate
- Альтернатива: `nnapi-rs` crate (прямой доступ без Java, но менее зрелый)

### Что гуглить для NPU-фич

1. **TFLite из Rust на Android** — ищи: `tflite rust android jni 2025 2026`, `tflite-rs android ndk`
2. **NNAPI из Rust** — ищи: `nnapi rust crate android`, `android neural networks api rust bindings`
3. **CREPE pitch detection TFLite** — ищи: `crepe pitch detection tflite model android real-time`, `pitch detection neural network mobile`
4. **DDSP real-time mobile** — ищи: `ddsp tflite android real-time audio synthesis 2025 2026`, `google ddsp mobile`
5. **NeuroPilot SDK** — ищи: `mediatek neuropilot sdk developer access 2025 2026`, `neuropilot vs nnapi performance`
6. **Аудио-модели на NPU: латентность** — ищи: `neural audio processing mobile npu latency benchmark`, `real-time audio inference android npu`

### Важно
- NPU-фичи — это БОНУС, не блокер. Сначала рабочий синт на Android без AI
- Аудио-модели крошечные (5-20MB) — NPU с запасом в 1000x по мощности
- Главная сложность не в производительности, а в тулчейне (Rust → JNI → NNAPI → NPU)
- Fallback на CPU всегда должен быть (для устройств без NPU, для Linux PC/OPi5)

## Результат

Один проект `mini_midi_synth` который:
```bash
# Linux PC (Arch)
cargo run --release

# Orange Pi 5
cross build --target aarch64-unknown-linux-gnu --release

# Android APK
cargo ndk -t arm64-v8a build --release  # или аналог
# → .apk для установки на OnePlus Ace 5 Ultra
```

Подключаешь M-VAVE SMK-37 PRO по USB-C к любому из устройств → играешь через один и тот же синтезатор.
