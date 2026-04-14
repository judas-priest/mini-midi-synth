# Headless Mode Implementation Plan

## Цель

Запустить `mini_midi_synth` на Orange Pi / Raspberry Pi без дисплея:
```
MIDI клавиатура → SBC (headless) → USB audio → колонки
```

---

## Аудит: что уже хорошо

- `audio.rs` — полностью независим от GUI, cpal stream в отдельном OS потоке ✓
- `midi.rs` — полностью независим от GUI, midir callback в отдельном OS потоке ✓
- `synth/` — не знает про GUI вообще ✓
- Вся синхронизация через lock-free ring-buffers (rtrb) и atomics ✓

## Проблема

`main()` плотно связан с `eframe::run_native()` — без GUI приложение не запускается.
`eframe` тянет за собой: winit, wgpu/glow, x11, wayland — тяжёлые зависимости, не нужные на headless SBC.

---

## Архитектура решения

### Feature flags в Cargo.toml

```toml
[features]
default = ["gui"]
gui = [
    "eframe/default_fonts",
    "eframe/glow",
    "eframe/x11",
    "eframe/wayland",
    "eframe/persistence",
]

[dependencies]
eframe = { version = "0.31", default-features = false, optional = true }
# остальные зависимости без изменений
```

Сборка:
- `cargo build --release` → с GUI (как сейчас)
- `cargo build --release --no-default-features` → headless бинарник

---

## Шаги реализации

### Шаг 1 — Cargo.toml: сделать eframe опциональным

```toml
[features]
default = ["gui"]
gui = []

[dependencies]
eframe = { version = "0.31", default-features = false,
           features = ["default_fonts", "glow", "x11", "wayland", "persistence"],
           optional = true }
```

### Шаг 2 — main.rs: разделить точку входа

Текущая структура main():
1. Config load
2. Audio init
3. MIDI init
4. SynthEngine init
5. App struct init (GUI state)  ← только для GUI
6. eframe::run_native()         ← только для GUI

Новая структура:

```rust
fn main() -> Result<()> {
    #[cfg(feature = "gui")]
    return run_gui();

    #[cfg(not(feature = "gui"))]
    return run_headless();
}
```

### Шаг 3 — run_headless()

```rust
#[cfg(not(feature = "gui"))]
fn run_headless() -> Result<()> {
    // 1. Загрузка конфига (без изменений)
    let config = Config::load();

    // 2. Ring buffers (без изменений)
    let (midi_tx, midi_rx) = rtrb::RingBuffer::new(256);
    let (ctrl_tx, ctrl_rx) = rtrb::RingBuffer::new(256);
    let (feedback_tx, feedback_rx) = rtrb::RingBuffer::new(64);

    // 3. Note state atomics (без изменений)
    let note_state = Arc::new(std::array::from_fn(|_| AtomicU8::new(0)));

    // 4. SynthEngine (без изменений)
    let engine = SynthEngine::new(config.sample_rate);

    // 5. Audio backend (без изменений)
    let _audio = AudioBackend::new(&config, engine, midi_rx, ctrl_rx)?;

    // 6. MIDI connect (без изменений)
    let _midi_conn = midi::connect(config.midi_port, midi_tx, note_state.clone())?;

    // 7. Загрузить дефолтный пресет
    // ctrl_tx.push(ControlEvent::LoadPreset(default_preset))?;

    // 8. Просто ждём — аудио и MIDI работают в своих потоках
    println!("mini_midi_synth running headless. Press Ctrl+C to exit.");
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        // drain feedback если нужно (можно игнорировать)
    }
}
```

### Шаг 4 — Обернуть GUI код в #[cfg(feature = "gui")]

Файлы которые нужно обернуть:
- `src/gui/` — весь модуль целиком
- `src/main.rs` — `make_piano_icon()`, `run_gui()`, импорты eframe

```rust
// main.rs
#[cfg(feature = "gui")]
mod gui;

#[cfg(feature = "gui")]
fn make_piano_icon() -> eframe::egui::IconData { ... }

#[cfg(feature = "gui")]
fn run_gui() -> Result<()> {
    // текущий код main() с eframe::run_native()
}
```

---

## Конфигурация для headless запуска

При старте без GUI приложение должно знать:
- какой MIDI порт использовать
- какой пресет загрузить
- какое аудио устройство использовать

Всё это уже есть в `~/.config/mini_midi_synth/config.json` — менять ничего не нужно.
GUI при следующем запуске сохраняет настройки → headless их читает.

---

## Сборка под ARM (Orange Pi / Raspberry Pi)

### Кросс-компиляция с хоста (быстрее):
```bash
# Установить target
rustup target add aarch64-unknown-linux-gnu

# Сборка headless
cargo build --release --no-default-features --target aarch64-unknown-linux-gnu
```

### Компиляция прямо на SBC (проще):
```bash
# На Orange Pi / RPi
git clone / scp проект
cargo build --release --no-default-features
```

Headless бинарник не требует libGL, libX11, libwayland — только libasound (ALSA).

---

## Автозапуск на SBC (systemd)

```ini
# /etc/systemd/system/synth.service
[Unit]
Description=mini_midi_synth headless
After=sound.target

[Service]
ExecStart=/home/user/mini_midi_synth --no-default-features
Restart=always
User=user
Environment=ALSA_CARD=1

[Install]
WantedBy=multi-user.target
```

```bash
systemctl enable synth
systemctl start synth
```

---

## Объём работы

| Файл | Изменения |
|---|---|
| `Cargo.toml` | Сделать eframe optional, добавить feature "gui" |
| `src/main.rs` | Разделить на `run_gui()` и `run_headless()`, обернуть GUI код в cfg |
| `src/gui/mod.rs` | Добавить `#[cfg(feature = "gui")]` на модуль |

**Итого: ~50-100 строк изменений, не трогая синтезатор вообще.**

---

## Что НЕ нужно менять

- `src/audio.rs` — без изменений
- `src/midi.rs` — без изменений
- `src/synth/` — без изменений
- `src/preset.rs` — без изменений
- `src/config.rs` — без изменений
- `src/cc_map.rs` — без изменений
