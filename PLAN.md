# План: Live Looper + Keybindings

## Цель

Дать возможность одному человеку с MIDI клавиатурой (SMK-37 Pro, 37 клавиш, без velocity) играть живой бит: drums (секвенсор) + bass (looper layer 1) + keys (looper layer 2). Переключение part и управление looper — с компьютерной клавиатуры.

---

## Блок 1: Looper BPM sync (опциональная синхронизация)

### Проблема
`looper.bpm` = 120.0 (хардкод при создании), `drum_engine.sequencer.bpm` — отдельное значение. При quantize looper считает grid по своему BPM, не совпадающему с драмами.

### Решение
- Добавить флаг `looper_sync_bpm: bool` в GUI state + config (default: true)
- Добавить `ControlEvent::LooperSetBpm { bpm: f32 }`
- При `looper_sync_bpm == true`: каждый раз когда GUI меняет drum BPM, также отправлять `LooperSetBpm` с тем же значением
- При `looper_sync_bpm == false`: показывать отдельный DragValue для looper BPM
- Engine handler: `self.looper.bpm = bpm`

### Файлы
- `src/gui/mod.rs` — поле `looper_sync_bpm: bool`
- `src/gui/drums.rs` — UI toggle + логика sync
- `src/synth/mod.rs` — новый вариант `ControlEvent::LooperSetBpm`, handler
- `src/config.rs` — `UiSettings::looper_sync_bpm` (persist)
- `src/main.rs` — init

### Риск: минимальный

---

## Блок 2: Part ID в LoopEvent

### Проблема
`LoopEvent._pad: u8` не используется. При playback (mod.rs:2200-2219) looper шлёт ноты во **все** parts — переключение пресета между overdub слоями бесполезно, всё играет текущим тембром.

### Решение

**Запись:**
- Переименовать `LoopEvent._pad` → `LoopEvent.part_id`
- `record_event()` получает дополнительный аргумент `part: u8`
- В `handle_event` (mod.rs:1483) передавать текущий `active_part` (новое поле engine или атомик из GUI)

**Воспроизведение:**
- `looper.tick()` возвращает `[(u8, u8, u8); MAX_SIMULTANEOUS]` → `(note, velocity, part_id)`
- В tick_block (mod.rs:2200) вместо цикла по всем parts — note_on только в `parts[part_id]`
- Проверять `part.enabled`, `min_note/max_note`, `vel_min/vel_max` как при обычном MIDI input

**Active part tracking:**
- Добавить `active_part: Arc<AtomicU8>` в engine, shared с GUI
- GUI обновляет при переключении part (через keybind или клик)
- Engine читает при `record_event` для `part_id`

### Файлы
- `src/synth/looper.rs` — `_pad` → `part_id`, сигнатуры `record_event`, `tick`
- `src/synth/mod.rs` — поле `active_part`, передача part_id при записи, роутинг при playback
- `src/gui/mod.rs` — `active_part_atom`, обновление при смене part
- `src/main.rs` — init атомика

### Риск: средний (меняет формат looper events, hot path)

---

## Блок 3: Keybindings система

### Дизайн

**Действия (enum KeyAction):**
```
SwitchPart(u8)      // 0-7
LooperRecord
LooperTogglePlay
LooperUndo
LooperClear
ToggleDrumsSynth
```

**Хранение:**
```rust
// config.rs
pub keybinds: HashMap<String, String>  // "F1" → "SwitchPart(0)"
```

Сериализация: `egui::Key` → строка ("F1", "Space", "R", ...).
Десериализация: строка → `KeyAction` enum.

**Дефолты:**
| Клавиша | Действие |
|---------|----------|
| F1-F8   | SwitchPart(0)-SwitchPart(7) |
| Space   | LooperTogglePlay |
| R       | LooperRecord |
| Z       | LooperUndo |
| X       | LooperClear |
| Tab     | ToggleDrumsSynth |

**Обработка (GUI frame loop):**
```rust
for (key, action) in &self.keybinds {
    if ctx.input(|i| i.key_pressed(*key)) && !self.text_editing {
        match action {
            SwitchPart(p) => { self.active_part = p; self.active_part_atom.store(p, ...); }
            LooperRecord => { self.ctrl_tx.push(ControlEvent::LooperRecord); }
            ...
        }
    }
}
```

Важно: НЕ обрабатывать keybinds когда фокус в текстовом поле (preset search, perf name).

**GUI настроек:**
- Окно "Keybinds" (кнопка в top bar или вкладка)
- Таблица: Action | Key | [Rebind]
- Клик "Rebind" → режим захвата → следующее нажатие = новый бинд
- Кнопка "Reset defaults"

### Файлы (новый + изменения)
- `src/gui/keybinds.rs` — **новый**: enum KeyAction, парсинг, GUI окно настроек
- `src/gui/mod.rs` — поле `keybinds`, `show_keybinds_window`, обработка в frame loop, `mod keybinds`
- `src/config.rs` — `UiSettings::keybinds: HashMap<String, String>`
- `src/main.rs` — init keybinds из config

### Риск: низкий (новый код, не трогает audio path)

---

## Порядок выполнения

| # | Блок | Зависимости | Сложность |
|---|------|-------------|-----------|
| 1 | BPM sync | нет | Низкая (30 мин) |
| 2 | Part ID в LoopEvent | нет | Средняя (1-2 ч) |
| 3 | Keybindings | Блок 2 (SwitchPart нужен active_part_atom) | Средняя (1-2 ч) |

Блоки 1 и 2 независимы — можно параллельно.
Блок 3 зависит от active_part_atom из Блока 2.

---

## Чеклист завершения

- [ ] Drum sequencer играет, looper quantize привязан к тому же BPM
- [ ] Записал bass в looper → переключил part на piano → overdub записывает piano
- [ ] При playback bass играет bass-пресетом, piano — piano-пресетом
- [ ] F1-F8 переключают parts с компьютерной клавиатуры
- [ ] Space/R/Z/X управляют looper с клавиатуры
- [ ] Бинды настраиваются в GUI и сохраняются в config.json
- [ ] Sync BPM toggle в GUI, сохраняется в config
- [ ] Все 26 тестов проходят
