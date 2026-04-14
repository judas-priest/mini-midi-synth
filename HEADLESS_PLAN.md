# Headless / CLI Mode Implementation Plan

## Цель

Запустить `mini_midi_synth` без GUI — как CLI-приложение с полным функционалом:
```
cargo run --release --no-default-features
```

---

## Что менять

### 1. Cargo.toml — eframe optional

```toml
[features]
default = ["gui"]
gui = ["eframe"]

[dependencies]
eframe = { ..., optional = true }
```

### 2. main.rs — разделить на run_gui() и run_headless()

Общий код (строки 73-144 текущего main):
- Config::load(), preset::load_all_presets()
- audio host detection, sample rates
- note_state, pad_state, ring buffers
- SynthEngine init + все atoms
- AudioBackend::new()
- MIDI list_ports + connect
- preset_idx из конфига

Выносится в структуру `CommonInit` + функцию `init_common()`.

`run_gui()` — текущий main() после init_common(), создаёт gui::App, eframe::run_native().

`run_headless()` — после init_common():

**Полный функционал без GUI:**
1. Отправить пресеты всех 8 частей (как send_initial_presets):
   - LoadPreset с params, mod_matrix, mseg, pitch_seq, wavetable для каждого layer
   - SetLayerEnabled, SetLayerVolume, SetLayerRange
   - SetGlobalParam (master_volume, master_tone, reverb_mix, delay_mix, pitch_bend_range)
   - DrumSetVolume
2. SF2 загрузка из конфига:
   - Прочитать config.sf2.keys_file_path → parse SoundFont → LoadKeysSoundFont
   - Прочитать config.sf2.drums_file_path → parse SoundFont → LoadDrumsSoundFont
   - SetLayerSf2Mode + SetLayerSf2Program для каждого layer где sf2=true
   - SetSf2BlockSize
3. MIDI auto-reconnect polling (каждые 3 сек)
4. Program Change polling (program_change_atom)
5. SIGTERM/SIGINT → All Notes Off + exit

### 3. #[cfg(feature = "gui")] gates

- `mod gui;` → `#[cfg(feature = "gui")] mod gui;`
- `make_piano_icon()` → за cfg
- `use crate::cc_map::CcMap` и `use crate::synth::drum::NUM_DRUM_SLOTS` — только в run_gui
- Импорты eframe — только в run_gui

---

## Что НЕ менять

- `src/audio.rs` — без изменений
- `src/midi.rs` — без изменений
- `src/synth/` — без изменений
- `src/preset.rs` — без изменений
- `src/config.rs` — без изменений
- `src/gui/` — без изменений (просто за cfg gate)

---

## Конфигурация

CLI читает тот же `~/.config/mini_midi_synth/config.json`:
- MIDI порт, аудио backend, sample rate, buffer size
- Последний пресет, громкости, SF2 пути, SF2 mode per layer
- GUI сохраняет настройки → CLI их читает

---

## Best Practices (из ресёрча)

### Аудио стек

| Вариант | Когда использовать |
|---|---|
| **Direct ALSA** | Dedicated synth appliance, одно приложение, минимум overhead |
| **PipeWire** | Нужна маршрутизация между приложениями |
| **JACK** | Избегать на ARM — лишний overhead |

Рекомендуемые буферы:

| Интерфейс | Буфер (frames) | Latency @48kHz |
|---|---|---|
| USB Class-Compliant | 128-256 | 2.7-5.3 ms |
| I2S HAT (Pisound, HiFiBerry) | 64-128 | 0.7-2.7 ms |
| Встроенный BCM audio RPi | 256+ | 10.7+ ms (непригоден) |

### Real-Time Linux

```bash
# CPU governor — самая важная оптимизация
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor

# CPU isolation (в cmdline.txt)
isolcpus=3

# Запуск с приоритетом
sudo chrt -f 80 taskset -c 3 ./mini_midi_synth
```

В коде:
```rust
// Memory locking — предотвращает page faults
unsafe { libc::mlockall(libc::MCL_CURRENT | libc::MCL_FUTURE); }
```

`/etc/security/limits.d/audio.conf`:
```
@audio   -  rtprio     95
@audio   -  memlock    unlimited
@audio   -  nice       -20
```

PREEMPT_RT kernel нужен только для <64 фреймов. Для 128+ стоковое ядро достаточно.

### Denormals на ARM (критично!)

```rust
#[cfg(target_arch = "aarch64")]
unsafe {
    let mut fpcr: u64;
    std::arch::asm!("mrs {}, fpcr", out(reg) fpcr);
    let new_fpcr = fpcr | (1 << 24); // FZ bit = flush-to-zero
    std::arch::asm!("msr fpcr, {}", in(reg) new_fpcr);
}
```

Без этого subnormal floats в фильтрах вызывают 10-100x CPU spike на ARM.

### MIDI Hot-Plug

- **udev rule**: `/etc/udev/rules.d/33-midiusb.rules` триггерит при USB MIDI подключении
- **Polling**: каждые 2-3 сек `midi::list_ports()` — самый простой и надёжный подход
- **Важно**: всегда `drop()` старый `MidiInputConnection` перед reconnect

### NEON оптимизация (ARM)

- f32 = 4-wide SIMD нативно (у нас уже f32 — ок)
- `#[repr(align(16))]` на аудио буферах для оптимальных NEON loads
- Cross-compile: `-C target-cpu=cortex-a72` (RPi4) / `cortex-a76` (RPi5)

```toml
# .cargo/config.toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
rustflags = ["-C", "target-cpu=cortex-a72"]
```

### Graceful Shutdown

```rust
static SHUTDOWN: AtomicBool = AtomicBool::new(false);

// При SIGTERM/SIGINT:
// 1. All Notes Off (CC 123) на все каналы
// 2. Flush pending config saves
// 3. Drop audio stream
```

### Подводные камни

1. **RPi 1/2/3**: USB и Ethernet на одной шине — сетевой трафик = xruns
2. **SD card I/O**: stall 50-200ms — всё грузить в RAM при старте, `mlockall(MCL_FUTURE)`
3. **Thermal throttling**: RPi 4 throttles на 80°C — радиатор + вентилятор
4. **PipeWire**: держит ALSA эксклюзивно — либо через PipeWire, либо отключить
5. **cpal ALSA**: known busy-spin issue (#322) — мониторить CPU
6. **midir**: каждый `MidiInput::new()` = новый ALSA client, утечка при reconnect
7. **`panic = "abort"`**: добавить `set_hook()` для логирования перед abort

### Референсные проекты

- **Zynthian** — полная synth платформа на RPi (JACK + ZynAddSubFX + FluidSynth)
- **Elk Audio OS** — dual-kernel (Xenomai RT + Linux), sub-1ms latency
- **Patchbox OS** (Blokas) — Debian с RT kernel + Pisound HAT
- **FluidSynth headless** — `fluidsynth -a alsa -m alsa_seq -i soundfont.sf2`

---

## Сборка под ARM

### Кросс-компиляция с хоста:
```bash
rustup target add aarch64-unknown-linux-gnu
sudo apt install gcc-aarch64-linux-gnu
cargo build --release --no-default-features --target aarch64-unknown-linux-gnu
```

Или через Docker:
```bash
cargo install cross
cross build --release --no-default-features --target aarch64-unknown-linux-gnu
```

### Компиляция на SBC:
```bash
cargo build --release --no-default-features
```

Headless бинарник не требует libGL, libX11, libwayland — только libasound (ALSA).

---

## systemd (для SBC)

```ini
# /etc/systemd/system/mini-midi-synth.service
[Unit]
Description=Mini MIDI Synth
After=sound.target

[Service]
Type=simple
User=synth
Group=audio
LimitRTPRIO=95
LimitMEMLOCK=infinity
CPUAffinity=3
Nice=-15
OOMScoreAdjust=-900
ExecStart=/usr/local/bin/mini_midi_synth
Restart=on-failure
RestartSec=2
WatchdogSec=30
TimeoutStopSec=10
KillSignal=SIGTERM
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
```

---

## Объём работы

| Файл | Изменения |
|---|---|
| `Cargo.toml` | eframe optional, feature "gui" |
| `src/main.rs` | Разделить на `run_gui()` + `run_headless()`, общая `init_common()` |
| `src/main.rs` | Signal handler, MIDI polling, Program Change polling |
| `src/main.rs` | `#[cfg(feature = "gui")]` на gui-зависимый код |

**Итого: ~100-150 строк изменений, синтезатор не трогаем.**

---

## Что НЕ нужно менять

- `src/audio.rs` — без изменений
- `src/midi.rs` — без изменений
- `src/synth/` — без изменений
- `src/preset.rs` — без изменений
- `src/config.rs` — без изменений
- `src/cc_map.rs` — без изменений
