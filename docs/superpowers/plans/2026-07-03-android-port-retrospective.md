# Android Port Retrospective

## What Worked Well

- **Synth engine — zero changes.** Entire `src/synth/` directory untouched. Pure Rust DSP code is fully cross-platform.
- **eframe GUI renders perfectly** on Android via glow (OpenGL ES). All 280 presets, effects, looper, drum machine, sequencer — everything works.
- **cpal audio** — AAudio via Oboe, 48kHz stereo, works out of the box after fixing libc++_shared.so loading.
- **APK size — 5.9 MB.** Tiny. Release build with LTO + strip.
- **`#[cfg]` gating approach** — clean separation of platform code. Desktop build completely unaffected.
- **Shared `parse_and_push()`** — MIDI parsing in one place, used by both midir (desktop) and JNI bridge (Android). No duplication.
- **OnceLock for JNI globals** — safe, no `static mut`, no UB.
- **`no_denormals` crate** — replaced manual x86 MXCSR asm, works on both x86_64 and aarch64.
- **Cargo feature flags** — `desktop` vs `android-app` cleanly separate platform deps (jack, x11, wayland vs android-native-activity).

## What Was Painful

### BLE MIDI — the biggest time sink
- **AMidi (NDK) does NOT receive BLE MIDI data.** Port connects, polling thread receives zero bytes. Completely undocumented limitation. Wasted hours debugging.
- **Had to build Java MidiReceiver → JNI → Rust bridge.** Works reliably but adds complexity (MidiBridge.java, SynthActivity.java, JNI function in main.rs).
- **midir AMidi polling thread caused audio artifacts.** Tight loop with 2ms sleep consumed CPU and created buzzing/whistling in audio output. Had to completely disable midir on Android.
- **BLE connection is flaky.** Device shows as paired but `LE:N` (not connected). `openBluetoothDevice()` sometimes fails to establish GATT connection. Restarting the app usually fixes it.
- **SMK-37 Pro has two BT profiles** — `SMK-37 Pro` (BR/EDR audio/SBC) and `SMK-37 Pro_BLE` (LE MIDI). If Classic BT connects first, audio routes to keyboard speaker instead of phone. Must disconnect Classic BT.
- **BLUETOOTH_CONNECT permission** — runtime permission needed on Android 12+, NativeActivity can't show permission dialogs properly.

### Build / Deployment
- **libc++_shared.so** — cpal uses Oboe (C++), which needs the C++ runtime. NativeActivity with `hasCode="false"` doesn't auto-load dependency .so files. Had to create SynthActivity wrapper with `System.loadLibrary("c++_shared")`.
- **`System.loadLibrary("mini_midi_synth")`** also needed in SynthActivity for JNI native method resolution. Without it, `UnsatisfiedLinkError`.
- **Debug vs Release APK** — `run-as` only works with debug APK. Can't push files (SF2) to app internal storage with release APK. Workaround: install debug, push files, reinstall release.
- **Scoped storage (Android 11+)** — `READ_EXTERNAL_STORAGE` doesn't work with targetSdk 35. SF2 scanning from `/sdcard/Download/` needs special handling.

### Minor Issues
- **eframe `persist_window` field** — doesn't exist in NativeOptions on Android, had to cfg-gate.
- **`dirs` crate** — returns None on Android. Had to create `app_config_dir()` / `app_data_dir()` with env var fallback.
- **512 compiler warnings** — `log` crate imported globally but logger backend only on Android. Harmless but noisy.
- **Phone speaker** — distorts at high volume. Synth sounds need headphones/external speaker for decent quality.

## Architecture Decisions

| Decision | Why |
|----------|-----|
| Java MidiBridge → JNI instead of AMidi for BLE MIDI | AMidi doesn't deliver BLE MIDI data |
| Disable midir entirely on Android | AMidi polling thread causes audio artifacts |
| `OnceLock` for JNI globals | Safe alternative to `static mut` |
| Shared `parse_and_push()` | Eliminates MIDI parsing duplication between midir and JNI paths |
| `SynthActivity extends NativeActivity` | Need Java code for libc++ loading + BLE MIDI bridge |
| `hasCode="true"` | Required because SynthActivity has Java code |
| Scan `/sdcard/Download/` for SF2 | Can't easily push to internal storage without root/debug APK |
| Feature flags `desktop` / `android-app` | Clean separation, no ifdef spaghetti |

## Memory Audit Summary

**No leaks found.** Full audit conducted:
- Ring buffers: fixed size (256 MIDI, 256 control, 64 feedback)
- Audio DSP: fixed-size arrays, no allocations in RT thread
- Arc usage: no cycles, proper reference counting
- Threads: properly joined/dropped
- Android: MidiDevice/MidiBridge closed in onDestroy()
- No `Box::leak`, `mem::forget`, or unbounded growth
- Undo stack bounded to 50 entries

## Stats

- **18 commits** on `new` branch
- **Files modified:** ~15 Rust files, 5 Java/Gradle files created
- **APK:** 5.9 MB
- **Desktop:** still builds, 26/26 tests pass
- **Time:** ~1 session (research + implementation + debugging BLE MIDI)
