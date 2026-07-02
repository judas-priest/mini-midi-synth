# Android Integration Notes for Rust Audio/MIDI Apps

Полный список граблей при портировании Rust синтезатора (eframe + cpal + midir) на Android.
Документ для Claude и будущих проектов.

---

## 1. Сборка и запуск

### libc++_shared.so — крэш при старте
**Симптом:** `dlopen failed: cannot locate symbol "__cxa_pure_virtual"`
**Причина:** cpal использует Oboe (C++ библиотека). NativeActivity загружает только основной .so, не его зависимости.
**Решение:**
- `cargo ndk --link-libcxx-shared` — копирует libc++_shared.so в jniLibs
- В Java: `System.loadLibrary("c++_shared")` в static init ПЕРЕД загрузкой основной библиотеки
- `android:hasCode="true"` в манифесте (нужен Java код)

### JNI native method resolution
**Симптом:** `UnsatisfiedLinkError` при вызове native метода из Java
**Причина:** NativeActivity загружает .so через свой механизм, но JNI символы не автоматически доступны другим Java классам.
**Решение:** `System.loadLibrary("mini_midi_synth")` в SynthActivity static init.

### main() на Android
**Симптом:** Ошибка линковки bin target при `cargo ndk build`
**Решение:**
- `#[cfg(target_os = "android")] fn main() {}` — заглушка
- `[[bin]] required-features = ["desktop"]` в Cargo.toml
- `[lib] crate-type = ["cdylib"]` + `[[bin]]` оба указывают на src/main.rs

### adb и LD_PRELOAD
**Симптом:** TLS ошибки при любой adb команде
**Решение:** `unset LD_PRELOAD` перед КАЖДОЙ adb командой.

---

## 2. Аудио (cpal / Oboe / AAudio)

### Аудио работает из коробки
cpal 0.15 на Android использует Oboe → AAudio. Стрим открывается, callback вызывается, звук идёт. Никаких изменений в аудио коде не потребовалось.

### НИКАКИХ логов внутри аудио callback
**Симптом:** Щелчки, писк, артефакты в звуке.
**Причина:** `log::info!()` внутри RT-thread блокирует на mutex логгера. Даже один вызов на каждый callback — уже слышно.
**Решение:** ПОЛНОСТЬЮ убрать любые log/println/eprintln из аудио callback. Для отладки — atomic counter + проверка в GUI thread.

### Маршрутизация аудио на Bluetooth
**Симптом:** Звук есть, но не слышно (или тихо).
**Причина:** Если BT-устройство (наушники, клавиатура с динамиком) подключено как A2DP, Android маршрутизирует ВСЁ аудио на него.
**Диагностика:** `adb shell "dumpsys audio" | grep "Devices:"` — если видно `bt_a2dp(80)`, звук идёт в Bluetooth.
**Решение:** Отключить BT Audio или подключить проводные наушники.

### Динамик телефона искажает
Телефонный динамик маленький и не воспроизводит низкие частоты. Синт будет звучать тонко/пискляво. Нормально — через наушники чисто.

---

## 3. BLE MIDI — главная боль

### AMidi (NDK) НЕ получает BLE MIDI данные
**Симптом:** midir подключается к BLE MIDI порту, `AMidiOutputPort_receive()` возвращает 0 байт навсегда.
**Причина:** AMidi NDK API не доставляет данные от BLE MIDI устройств. Это недокументированное ограничение. USB MIDI через AMidi работает.
**Решение:** Получать MIDI данные через Java `MidiReceiver` API и передавать в Rust через JNI.

### midir polling thread вызывает аудио артефакты
**Симптом:** Постоянный писк/жужжание в аудио.
**Причина:** midir на Android создаёт polling thread с `AMidiOutputPort_receive()` в цикле с 2ms sleep. Этот thread крутится бесполезно (данные не приходят) и жрёт CPU, вызывая underrun в аудио callback.
**Решение:** ПОЛНОСТЬЮ отключить midir на Android:
```rust
#[cfg(not(target_os = "android"))]
let midi_conn = midi::connect(...);
#[cfg(target_os = "android")]
let midi_conn: Option<MidiInputConnection<()>> = None;
```

### Архитектура BLE MIDI на Android
Единственный рабочий путь:
```
BLE клавиатура
    ↓ Bluetooth LE
Android MidiManager.openBluetoothDevice() [Java]
    ↓ MidiDevice.openOutputPort()
MidiReceiver.onSend(byte[] data) [Java]
    ↓ JNI call
Rust: parse MIDI bytes → push в rtrb ring buffer
    ↓
Audio thread: Consumer → SynthEngine
```

### Ключевые файлы для BLE MIDI bridge:
- `SynthActivity.java` — открывает BLE устройства, подключает MidiBridge к output портам
- `MidiBridge.java` — extends MidiReceiver, вызывает JNI `onMidiData(byte[])`
- `src/midi.rs` — `parse_and_push()` парсит MIDI байты и пушит в ring buffer
- `src/main.rs` — JNI функция `Java_com_minimidisynth_MidiBridge_onMidiData`, OnceLock глобалы

### OnceLock для JNI глобалов
```rust
static JNI_MIDI_TX: OnceLock<SharedMidiTx> = OnceLock::new();
// НЕ static mut! OnceLock — safe, stable since Rust 1.70
```
Устанавливается в `init_common()`, читается в JNI callback.

### Спаренное ≠ подключённое
**Симптом:** BLE MIDI bridge подключён, но данные не приходят.
**Причина:** Устройство спарено в настройках Android, но BLE соединение не активно.
**Диагностика:** `adb shell "dumpsys bluetooth_manager | grep 'DEVICE_NAME'"` — если `LE:N`, BLE не подключено.
**Решение:** `openBluetoothDevice()` должен инициировать подключение. Если не помогает — перезапустить приложение. Клавиатура должна быть включена и в зоне действия.

### SMK-37 Pro — два BT профиля
- `SMK-37 Pro` (BR/EDR) — Classic Bluetooth Audio (SBC/A2DP)
- `SMK-37 Pro_BLE` (LE) — BLE MIDI

Это РАЗНЫЕ устройства с разными MAC-адресами. Для MIDI нужен именно `_BLE` профиль.

### MidiReceiver.onSend() — формат данных
Android MIDI API transport-agnostic. `onSend()` получает **стандартные MIDI байты** (не BLE-MIDI пакеты). BLE-обёртка снимается драйвером. wmidi парсит их корректно.

### Данные могут содержать несколько сообщений
Android docs: "can contain multiple messages or partial messages". Нужен loop с `bytes_size()`:
```rust
let mut offset = 0;
while offset < bytes.len() {
    let consumed = parse_and_push(&bytes[offset..], ...);
    if consumed == 0 { break }
    offset += consumed;
}
```

---

## 4. Хранение файлов

### dirs crate на Android
`dirs::config_dir()` и `dirs::data_dir()` возвращают `None` на Android.
**Решение:** Env var `MINI_SYNTH_DATA_DIR` устанавливается в `android_main()` из `app.internal_data_path()`.

### Internal storage недоступно через adb
**Симптом:** `adb push` в `/data/user/0/com.app/files/` — Permission denied.
**Причина:** Release APK не debuggable, `run-as` не работает.
**Решение для SF2 файлов:**
1. Собрать debug APK: `./gradlew assembleDebug && adb install -r ...debug.apk`
2. `adb push file.sf2 /data/local/tmp/`
3. `adb shell "cat /data/local/tmp/file.sf2 | run-as com.app sh -c 'mkdir -p files/sf2 && cat > files/sf2/file.sf2'"`
4. Переустановить release APK (internal storage сохраняется)

Или: сканировать `/sdcard/Download/` как fallback для SF2 файлов (нужен permission).

---

## 5. Разное

### eframe на Android
- Версия 0.31: `android-native-activity` feature
- `NativeOptions.android_app = Some(app)` — передача AndroidApp
- `persist_window` — не существует на Android, cfg-gate
- GUI рендерится через glow (OpenGL ES) — работает идеально
- Шрифты мелкие для тачскрина — нужен `ctx.set_pixels_per_point()`

### Denormal flushing (aarch64)
x86 MXCSR asm не компилируется на ARM. Используй `no_denormals` crate — работает на обоих архитектурах.

### Cargo.toml feature flags
```toml
[features]
default = ["gui", "desktop"]
gui = ["dep:eframe"]
desktop = ["eframe/x11", "eframe/wayland"]
android-app = ["eframe/android-native-activity"]

[target.'cfg(target_os = "linux")'.dependencies]
cpal = { version = "0.15", features = ["jack"] }
libc = "0.2"

[target.'cfg(target_os = "android")'.dependencies]
cpal = { version = "0.15" }
```
Десктоп: `cargo build` (default features). Android: `cargo ndk build --no-default-features --features gui,android-app`.

### Полезные команды отладки
```bash
# Логи приложения
unset LD_PRELOAD && adb logcat -s "MiniMidiSynth"

# BLE статус устройства
adb shell "dumpsys bluetooth_manager | grep 'DEVICE_NAME'"

# Куда идёт аудио
adb shell "dumpsys audio" | grep "Devices:"

# Скриншот
adb shell screencap -p /sdcard/s.png && adb pull /sdcard/s.png /tmp/s.png
```
