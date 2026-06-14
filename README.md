# Mini MIDI Synth

Полифонический синтезатор с драм-машиной, MIDI-лупером и live performance workflow. Написан на Rust, работает на Linux (x86_64, aarch64).

## Возможности

### Синтез
- 20+ алгоритмов осцилляторов: классические (Saw, Square, Sine, Triangle), FM, Supersaw, Karplus-Strong, Phase Distortion, WaveFolder, Sync, Alias, Window, Pulse, формантный, орган, пианино (физическое моделирование), электропиано, бас, саксофон, аккордеон, скрипка, духовые, Mutable Instruments Plaits
- 35+ типов фильтров: SVF (LP/HP/BP/Notch/AP), Ladder, K35, OBXd, Tripole, CutoffWarp, ResWarp, Polivoks, Comb, S&H
- 4 LFO с tempo sync, 2 Scene LFO, MSEG (multi-segment envelope generator)
- Модуляционная матрица (16 слотов, 20+ источников и назначений)
- 8 макро-ручек с XY Pad
- Pitch sequencer (2 независимых), Step sequencer для LFO
- Арпеджиатор (Up/Down/UpDown/Random/Order, 1-4 октавы, sync к BPM)
- SF2 Soundfont поддержка (Rustysynth)

### Эффекты (30+ модулей)
Chorus, Flanger, Phaser, Tremolo, Rotary Speaker, BBD Ensemble, Delay (stereo, ping-pong), Floaty Delay, Reverb (algorithmic), Reverb 2, Spring Reverb, Convolution Reverb, Nimbus (granular), Overdrive, Tape Saturation, Bitcrusher, Wave Shaper, Ring Modulation, Frequency Shifter, Neuron, Bonsai, Combulator, Treemonster, Vocoder, Resonator, Exciter, Conditioner, Compressor, EQ (parametric + 11-band graphic), MS Tool, Airwindows

FX Chain: настраиваемый порядок эффектов (drag-and-drop слоты).

### Драм-машина
- 8 слотов ударных с синтезом или SF2 сэмплами
- Step sequencer (16 шагов, 8 паттернов)
- Swing, BPM, live record через MIDI
- Импорт MIDI файлов как паттернов

### MIDI Лупер
- Part-aware запись: каждый overdub-слой помнит на каком парте (звуке) был записан
- Overdub: неограниченные слои поверх основного лупа
- Quantize: Off, 1/4, 1/8, 1/16
- Длина: 1, 2, 4, 8 тактов, sync BPM с драм-машиной
- Per-layer Mute/Solo — вырубай/солируй отдельные слои на лету
- Undo последнего слоя
- Timeline визуализация с цветными нотами по партам и playhead
- Layer list с именами пресетов, [S]olo/[M]ute кнопками

### Парты и слои
- 8 партов (Zone A: A1-A4, Zone B: B1-B4)
- Split mode: разделение клавиатуры на 2 зоны
- Per-part: volume, pan, transpose, velocity range, mute
- Performance save/load (все парты + настройки)

### GUI
- Init Preset / Randomize — сброс или случайный звук одной кнопкой
- XY Pad для макросов (двухосевое управление модуляцией)
- MIDI Learn — правый клик на параметр → крути ручку → привязано
- Filter frequency response curve (визуализация АЧХ фильтра)
- Осциллоскоп (waveform scope)
- Настраиваемые горячие клавиши
- Key zone map (визуализация диапазонов партов)
- Preset search

---

## Управление

### Горячие клавиши (настраиваемые)

| Клавиша | Действие |
|---------|----------|
| F1-F8 | Переключение партов A1-B4 |
| Space | Looper Play/Stop |
| R | Looper Record / Stop Record |
| Z | Looper Undo (последний слой) |
| X | Looper Clear |
| Tab | Переключение Drums / Synth режима |

Настройка: кнопка ⌨ в верхней панели → окно Keybindings → клик на клавишу → нажать новую.

### MIDI (SMK-37 Pro)

| Контрол | Функция |
|---------|---------|
| Ноты 123/124 | SEQ Play/Stop, Record |
| CC 48-55 (K1-K8) | Filter Cutoff, Resonance, Env Amount, Release, Attack, Portamento, LFO Filter, Chorus |
| CC 64-67 (F1-F4) | Master Volume, Tone, Reverb, Delay |
| Program Change | Переключение пресетов |
| Navigate (SysEx) | Навигация |

CC mapping настраивается через MIDI Learn (правый клик на любой параметр) или в Settings.

---

## Live Workflow

### Базовый джем

1. Настрой звуки на партах:
   - **F1** → выбери пресет бас
   - **F2** → выбери пресет пианино
   - **F3** → выбери пресет пэд
2. Включи драмы: **Tab** → Drums → Play
3. Запиши бас-луп:
   - **F1** (переключись на бас)
   - **R** (запись)
   - Играй бас-линию 4 такта
   - **R** (стоп записи, луп крутится)
4. Наложи пианино:
   - **F2** (переключись на пианино)
   - **R** (overdub)
   - Играй аккорды
   - **R** (стоп)
5. Наложи пэд:
   - **F3** → **R** → играй → **R**
6. Играй соло поверх лупа на любом парте

### Live управление слоями

- **[M]** на слое → mute (звук выключен, луп продолжает крутиться синхронно)
- **[M]** ещё раз → unmute (звук возвращается ровно в такт)
- **[S]** на слое → solo (все остальные молчат)
- **[S]** ещё раз → solo снят
- **Undo** → удалить последний записанный слой
- **"Recording to" dropdown** → сменить парт для следующей записи, не уходя из looper

### Арпеджиатор

1. Включи Arp в параметрах (секция Arp → checkbox)
2. Выбери: Mode (Up/Down/UpDown/Random), Rate (1/8, 1/16...), Octaves (1-4), Gate
3. Зажми аккорд — арпеджиатор играет ноты по паттерну
4. R → записывается арпеджио (не зажатые клавиши, а результат арпеджиатора)

---

## Сборка

### Требования

```bash
# Debian/Ubuntu
sudo apt install -y \
  libasound2-dev \
  libjack-jackd2-dev \
  pkg-config \
  build-essential \
  libgl1-mesa-dev \
  libxkbcommon-dev \
  libwayland-dev \
  libx11-dev

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### С GUI

```bash
cargo build --release
./target/release/mini_midi_synth
```

### Headless (без экрана)

```bash
cargo build --release --no-default-features
./target/release/mini_midi_synth --headless
```

Headless режим читает USB клавиатуру через evdev (`/dev/input/`). Те же горячие клавиши из `config.json`. Требуется: пользователь в группе `input` или запуск от root.

```bash
sudo usermod -aG input $USER
```

### Orange Pi 5 (aarch64)

```bash
# На самом Orange Pi:
sudo apt install libasound2-dev pkg-config build-essential
cargo build --release --no-default-features
```

Или кросс-компиляция:
```bash
rustup target add aarch64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu --no-default-features
```

---

## Конфигурация

Файл: `~/.config/mini_midi_synth/config.json`

Сохраняется автоматически. Содержит:
- Аудио настройки (sample rate, buffer size)
- MIDI порт
- CC mapping
- Горячие клавиши
- Последний пресет, громкости, FX параметры
- SF2 пути
- Looper sync BPM

Пресеты: встроенные (в бинарнике) + пользовательские в `~/.config/mini_midi_synth/presets/`

Performances (мульти-парт конфигурации): `~/.config/mini_midi_synth/performances/`

Drum kits: `~/.config/mini_midi_synth/drum_kits/`

---

## Архитектура

```
┌──────────────┐     rtrb      ┌──────────────────┐
│   GUI/CLI    │ ──ctrl_tx──→  │   Audio Thread    │
│  (egui /     │ ←─feedback──  │  (SynthEngine)    │
│   evdev)     │               │                   │
│              │   rtrb        │  Parts[8]         │
│  Keybinds    │ ──midi_tx──→  │  DrumEngine       │
│  CC Map      │               │  Arpeggiator      │
│  Params      │   atomics     │  MidiLooper       │
│  Looper UI   │ ←──atoms───   │  FX Chain         │
│              │               │  Sampler (SF2)    │
└──────────────┘               └──────────────────┘
```

- **Zero-alloc audio path**: фиксированные массивы, никаких Vec/String/Box в `tick_block`
- **Lock-free**: rtrb ring buffers для ControlEvent/MidiEvent, atomics для состояния
- **LooperDisplay**: `Arc<Mutex<>>` с `try_lock` — аудио поток никогда не блокируется

---

## Лицензия

Проприетарный код. Не для распространения.
