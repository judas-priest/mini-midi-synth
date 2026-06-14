# План: UX и звуковые улучшения

Предыдущий план (Looper + Keybindings) выполнен полностью.

---

## Блок 1: Init Preset + Randomize

### Init Preset
- Кнопка "Init" рядом с preset selector
- Загружает `PatchParams::default()` в текущий part
- Отправляет `ControlEvent::load_patch_from(part, &init_patch)` с дефолтным Patch
- Сбрасывает `edited_params` в GUI

### Randomize
- Кнопка "Rnd" рядом с Init
- Случайные значения для ключевых параметров: osc_type, filter_cutoff/resonance, ADSR, LFO rate/depth, noise_level, detune, FX mixes (малые значения)
- Остальное — default (чтобы не генерить мусор)
- Отправляет как обычный LoadPatch

### Файлы
- `src/gui/mod.rs` или `src/gui/params.rs` — кнопки Init/Rnd
- `src/synth/patch_params.rs` — `PatchParams::random()` метод
- `src/preset.rs` — хелпер для создания init Patch

### Риск: минимальный

---

## Блок 2: XY Pad

### Дизайн
- egui виджет: прямоугольник ~200x200, drag = изменение двух макросов
- X → Macro 1 (0..1), Y → Macro 2 (0..1)
- Вся цепочка уже работает: `SetMacro` → `macro_vals` → Mod Matrix → параметры
- Отображение: точка текущей позиции, подписи осей (имена макросов)
- Выбор какие макросы привязаны к X/Y — два ComboBox (Macro 1-8)

### Файлы
- `src/gui/macros.rs` — добавить XY pad под/над grid макросов
- Не трогает audio path

### Риск: минимальный

---

## Блок 3: Арпеджиатор

### Архитектура
- Новый `src/synth/arpeggiator.rs` — `Arpeggiator` struct
- Хранит held_notes (до 16), текущий паттерн, позицию, rate
- `tick()` вызывается из `tick_block` (synth/mod.rs), возвращает Option<(note, velocity)>
- `note_on/note_off` вызываются из `handle_event` ДО роутинга в parts
- Когда arp включён: `handle_event` NoteOn/NoteOff → arp.note_on/off (не в parts)
- Arp.tick() генерирует ноты → роутятся в parts стандартным путём
- BPM берёт из drum_engine.sequencer.bpm (уже доступен)

### Параметры (в PatchParams)
- `arp_enabled: f32` (0/1)
- `arp_mode: f32` (0=Up, 1=Down, 2=UpDown, 3=Random, 4=Order)
- `arp_rate: f32` (0=1/4, 1=1/8, 2=1/16, 3=1/32, 4=1/4T, 5=1/8T)
- `arp_octaves: f32` (1-4)
- `arp_gate: f32` (0.1..1.0 — длина ноты относительно шага)

### GUI
- Секция "Arp" в params — toggle + mode/rate/octaves/gate
- Компактная горизонтальная полоска

### Совместимость
- Лупер записывает ноты ПОСЛЕ арпа (записывается результат арпеджио, не зажатые клавиши)
- Работает в headless (audio thread)
- Сохраняется в пресете

### Файлы
- `src/synth/arpeggiator.rs` — **новый**
- `src/synth/mod.rs` — поле `arpeggiator`, вызов в tick_block + handle_event
- `src/synth/patch_params.rs` — 5 новых полей
- `src/gui/params.rs` — секция Arp UI

### Риск: средний (вклинивается в note handling hot path)

---

## Блок 4: MIDI Learn

### Дизайн
- Режим: клик правой кнопкой на параметр → "MIDI Learn" → крути ручку → привязано
- GUI state: `midi_learn_target: Option<&'static str>` (param key)
- При получении `ParamFeedback::CcReceived { cc }` и `midi_learn_target.is_some()` → создать CcBinding для этого CC → target param
- Обновить `CcMap` и отправить через `ControlEvent::SetCcMap`

### Уже есть
- `CcMap` с `bindings[128]` — RT-safe, Copy
- `ParamFeedback::CcReceived { cc }` — уже отправляется из audio thread
- GUI settings для ручного редактирования CC map
- `PARAM_REGISTRY` с min/max/log для каждого параметра

### Файлы
- `src/gui/mod.rs` — поле `midi_learn_target`, context menu на параметрах
- `src/gui/params.rs` — right-click → "MIDI Learn" на слайдерах
- `src/gui/settings.rs` — индикация текущего learn mode
- Не трогает audio path (только GUI + existing CcMap system)

### Риск: низкий

---

## Блок 5: Визуализация фильтра

### Дизайн
- Frequency response curve рядом с параметрами фильтра
- Рисуется через egui painter (как looper timeline)
- Формулы: для SVF/biquad — аналитический расчёт magnitude response по cutoff/resonance/type
- Обновляется при изменении filter_cutoff, filter_resonance, filter_type
- Размер: ~300x80px, лог шкала по X (20Hz-20kHz), dB по Y

### Файлы
- `src/gui/params.rs` — `draw_filter_response()` рядом с filter controls
- Чистый GUI, не трогает audio path

### Риск: минимальный

---

## Порядок выполнения

| # | Блок | Зависимости | Сложность |
|---|------|-------------|-----------|
| 1 | Init + Randomize | нет | Низкая |
| 2 | XY Pad | нет | Низкая |
| 3 | Арпеджиатор | нет | Средняя |
| 4 | MIDI Learn | нет | Средняя |
| 5 | Фильтр визуализация | нет | Низкая-Средняя |

Все блоки независимы — можно в любом порядке.

---

## Чеклист завершения

- [ ] Init Preset сбрасывает звук в default
- [ ] Randomize генерирует играбельные случайные звуки
- [ ] XY Pad управляет двумя макросами, видно в Mod Matrix
- [ ] Arp: Up/Down/UpDown/Random работают, sync к BPM, записывается в looper
- [ ] MIDI Learn: правый клик → крутим ручку → привязано, сохраняется в config
- [ ] Filter curve обновляется при смене cutoff/resonance/type
- [ ] Headless mode не сломан
- [ ] Оба режима сборки (GUI + headless) компилируются без warnings

---

# Идеи (отложено)

Фичи которые не ложатся легко в текущую архитектуру или требуют значительных усилий:

### Звук / DSP
- **Гранулярный осциллятор** — загрузка сэмпла, grain scheduling, новый OscType. Nimbus делает гранулярный FX, но гранулярный источник звука — другая задача. Нужен sample import UI.
- **Spectral freeze/morph** — FFT на audio thread, overlap-add, отдельный буфер. CPU-интенсивно.
- **Microtuning (.scl)** — парсинг Scala файлов, пересчёт частот в осцилляторах. Средняя сложность, нишевый спрос.

### GUI
- **Drag-and-drop модуляция** — как Serum/Vital. Mod Matrix уже есть, но D&D в egui требует кастомного виджета с drag source/drop target трекингом.
- **Мини-пиано-ролл для pitch sequencer** — визуальное редактирование мелодий. Сейчас pitch seq управляется числами.
- **Preset browser с тегами** — требует миграции пресетов в новый формат с метаданными (category, tags, author).
- **Dark/Light тема** — переключение egui Visuals. Простая идея, но нужно проверить все кастомные цвета.

### Платформа
- **Web UI для headless** — HTTP сервер + HTML/JS фронтенд + WebSocket. Отдельный большой проект. Стандарт для Zynthian/MOD Duo.
- **Systemd service** — автозапуск на Orange Pi. Простой .service файл, но нужно тестировать на реальном железе.
- **Экспорт лупа в WAV** — offline render через движок или запись audio output в файл. Нужен отдельный render path или ring buffer для записи.
