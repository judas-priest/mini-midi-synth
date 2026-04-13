# Аудит: чего не хватает по сравнению с Surge XT

Дата: 2026-04-11
Последнее обновление: 2026-04-13

---

## Критические (архитектурные пробелы)

### Модуляция
- **12 LFO вместо 4** — Surge имеет 6 voice LFO + 6 scene LFO на сцену. Scene LFO непрерывные, не перезапускаются на нотах.
- **Formula LFO (Lua-скриптинг)** — каждый LFO-слот может быть заменён Lua-скриптом с произвольной логикой. Полностью отсутствует.
- **Неограниченные modulation destinations** — в Surge любой параметр является целью. У нас фиксированный список из ~30 destination.
- **8 макро-ручек** как именованные источники модуляции (вместо 4 безымянных CC).

### FX-цепочка
- ✅ **16-слотовая FX-архитектура** — реализована (`fx_chain.rs`): 32 типа эффектов, GUI с 16 слотами, сериализация в пресеты, backwards compatibility.
- ✅ **Полная библиотека Airwindows** — 33 алгоритма (было 6): Drive, HardVacuum, Spiral2, Fracture, Mojo, ADClip7, Loud, IronOxide5, ToTape6, ChromeOxide, Pressure4, ButterComp2, VariMu, PowerSag, Galactic, Verbity, Capacitor, Focus, YLowpass, DubSub, Melt, Pop, BitGlitter, DeRez2, BussColors4, Hombre, Slew2.

### Фильтры
- ✅ **Waveshaper между двумя фильтрами** — реализован в `voice.rs` для Serial routing: Osc → F1 → [WS] → F2. Контролы в GUI.
- **8 режимов роутинга фильтров** вместо 3 — Serial 2/3 с feedback, Stereo (L/R независимо), Ring, Wide.

---

## Значительные

### Осцилляторы
- **Twist** — полный порт Mutable Instruments Plaits (16 движков): Waveforms, Waveshaper, 2-op FM, Formant/PD, Harmonic, Wavetable, Chords, Vowels/Speech, Granular Cloud, Filtered Noise, Particle Noise, Inharmonic String, Modal Resonator, Analog Kick, Analog Snare, Analog Hi-Hat.
- **Modern** — multi-point BLEP anti-aliasing, чище чем single-point PolyBLEP на высоких частотах.
- **Audio Input** — внешний аудио-сигнал как осциллятор (для вокодинга, фильтрации и т.д.).
- **Oscillator drift** — per-voice аналоговая нестабильность строя через отдельный drift LFO.
- **Все 3 осциллятора любого типа** — у нас Osc 2 и 3 ограничены простыми типами (Sine/Saw/Square/Triangle/FM).
- **Per-oscillator keytrack disable** — фиксированная высота тона для Osc 2/3.
- **Per-oscillator retrigger** — управление сбросом фазы на note-on.
- **Unison до 16 голосов** на осциллятор (у нас 8 суммарно на голос).

### Огибающие
- **AHDSR вместо ADSR** — сегмент Hold между Attack и Decay.
- ✅ **Fix envelope curve bug** — кривые Linear и Quadratic переписаны на phase tracking (0→1 линейно). Sqrt и Exponential оставлены на IIR. Retrigger корректно вычисляет начальную фазу.
- **Retrigger modes** — в моно-режиме огибающая может продолжаться с текущего уровня вместо перезапуска с нуля.
- **LFO trigger modes**: Free Run, Keytrigger, Random Start, Random Stop (у нас всегда key-triggered).

### Голосовая архитектура
- **Fingered portamento** — глайд только при легато (когда клавиша уже зажата). У нас portamento всегда включён при активации.
- **Single Trigger mode** — в моно нет re-attack при легато (огибающая продолжается).
- **Latch mode** — нота держится до следующей.
- **Keyboard split** — MIDI-диапазон разбит между Scene A и Scene B внутри патча.

---

## Умеренные

- **Микротюнинг (Scala/KBM)** — загрузка .scl и .kbm файлов для нестандартных темпераментов. У нас только стандартный 12-TET.
- **MPE (MIDI Polyphonic Expression)** — per-note pitch bend, pressure, slide как источники модуляции. Есть poly aftertouch, но не полный MPE.
- **Lua wavetable scripting** + загрузка .wav/.wt файлов пользователем. У нас 8 хардкод-фреймов (sine→saw).
- **Асимметричный pitch bend** — отдельные диапазоны для up и down (например, +2 / -12 полутонов).
- **Scene morph** — кросс-фейд между двумя полностью разными звуковыми конфигурациями в одном патче.
- **Step Sequencer: tied/held шаги** — легато между шагами.
- **LFO деформация по режимам** — Surge имеет 3 различных deform-варианта для sine, triangle, ramp, envelope (резкость, fold, квадрат и т.д.).

---

## Что есть в mini_midi_synth, чего нет в Surge

- Drum Step Sequencer (16 шагов × 8 слотов)
- MIDI Looper (запись, overdub, undo)
- MIDI File Player (импорт .mid файлов)
- Pitch Step Sequencer со scale quantization (хроматика, major, minor, pentatonic, blues, dorian, mixolydian)
- Больше физических моделей осцилляторов (BassGuitar, Brass, Saxophone, Accordion, CommutedPiano и др.)

---

## Приоритетный список для реализации

| Статус | Приоритет | Фича | Сложность |
|--------|-----------|------|-----------|
| ✅ | 🔴 Высокий | Waveshaper между фильтрами | Низкая |
| ✅ | 🔴 Высокий | Fix envelope curve bug (нормализованная фаза) | Низкая |
| ✅ | 🔴 Высокий | Полная Airwindows библиотека (33 алгоритма) | Высокая |
| ✅ | 🔴 Высокий | 16-слотовая FX-цепочка | Высокая |
| ⬜ | 🟡 Средний | Scene LFO (2 доп. LFO на слой) | Средняя |
| ⬜ | 🟡 Средний | Hold-сегмент (AHDSR) | Низкая |
| ⬜ | 🟡 Средний | Fingered portamento | Низкая |
| ⬜ | 🟡 Средний | 8 макро-ручек | Средняя |
| ⬜ | 🟡 Средний | LFO trigger modes (Free Run, Random) | Низкая |
| ⬜ | 🟡 Средний | Микротюнинг Scala/KBM | Средняя |
| ⬜ | 🟡 Средний | Asymmetric pitch bend | Низкая |
| ⬜ | 🟢 Низкий | Twist/Plaits порт | Очень высокая |
| ⬜ | 🟢 Низкий | Lua Formula LFO | Очень высокая |
| ⬜ | 🟢 Низкий | MPE | Высокая |
| ⬜ | 🟢 Низкий | Неограниченный mod matrix | Высокая |
| ⬜ | 🟢 Низкий | Audio Input осциллятор | Средняя |
| ⬜ | 🟢 Низкий | Lua wavetable scripting | Высокая |
