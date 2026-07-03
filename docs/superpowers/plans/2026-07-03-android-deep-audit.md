# Android Deep Audit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix all bugs, memory leaks, bad practices, and debug leftovers in the Android port. Leave code production-ready.

**Architecture:** Audit all Android-specific code (Java + Rust), fix issues in-place, verify desktop still works after each fix.

**Tech Stack:** Rust, Java, Android MIDI API, JNI, cpal, eframe

---

## Found Issues Summary

| # | Severity | File | Issue |
|---|----------|------|-------|
| 1 | **HIGH** | `src/midi.rs:186` | `log::info!` inside midir RT callback on desktop — fires on every MIDI message, can cause glitches |
| 2 | **HIGH** | `src/midi.rs:179` | `log::info!` on every MIDI reconnect — harmless but noisy on desktop (no logger backend) |
| 3 | **MEDIUM** | `src/main.rs:801` | `std::env::set_var` is unsafe in multi-threaded context (Rust 2024 edition will error). Called from android_main before other threads, but still bad practice |
| 4 | **MEDIUM** | `SynthActivity.java:42` | `openPairedBluetoothMidiDevices()` before `super.onCreate()` — no guaranteed Activity context |
| 5 | **MEDIUM** | `SynthActivity.java:82-88` | `requestPermissions()` called before `super.onCreate()` — may crash on some devices |
| 6 | **MEDIUM** | `SynthActivity.java:126,147` | `mOpenDevices` and `mBridges` modified from callback thread (Handler main looper) but no synchronization |
| 7 | **LOW** | `MidiBridge.java:18` | `onMidiData` is `public static native` — should be package-private to prevent accidental external calls |
| 8 | **LOW** | `src/main.rs:194,203` | `eprintln!` on Android goes nowhere (stderr not connected to logcat) |
| 9 | **LOW** | `AndroidManifest.xml:7` | `READ_EXTERNAL_STORAGE` is deprecated on API 33+, does nothing on targetSdk 35 |
| 10 | **LOW** | `Cargo.toml:57` | `panic = "abort"` in release — panic hook logs but process aborts before flush |
| 11 | **CLEANUP** | Various | Debug log leftovers from development session |

---

### Task 1: Remove debug logging from RT-critical paths

**Files:**
- Modify: `src/midi.rs:179,186`
- Modify: `src/main.rs:194,203`

The midir callback runs on an RT thread on desktop. `log::info!` on every MIDI message = mutex contention = audio glitches. On Android, `eprintln!` goes to /dev/null.

- [ ] **Step 1: Remove `log::info!` from midir callback**

In `src/midi.rs`, remove line 186:
```rust
                log::info!("[midi] Received {} bytes", data.len());
```

Keep line 179 (`log::info!("[midi] Connecting to port: ...")`) — it fires once, not per-message.

- [ ] **Step 2: Replace `eprintln!` with `log::info!` in init_common**

In `src/main.rs`, replace lines 194 and 203:
```rust
// Replace:
eprintln!("[init] Audio: sample_rate={actual_sr}");
// With:
log::info!("[init] Audio: sample_rate={actual_sr}");

// Replace:
eprintln!("[init] MIDI ports: {midi_port_names:?}");
// With:
log::info!("[init] MIDI ports: {midi_port_names:?}");
```

On Android these go to logcat via android_logger. On desktop without a logger backend they're no-ops (silent, no overhead).

- [ ] **Step 3: Build and test**

```bash
cargo build --release && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add src/midi.rs src/main.rs
git commit -m "fix: remove debug logging from MIDI RT callback"
```

---

### Task 2: Fix `set_var` safety in android_main

**Files:**
- Modify: `src/main.rs:800-803`
- Modify: `src/config.rs:148-157,160-173`

`std::env::set_var` is unsafe in multi-threaded programs (UB if another thread reads env). In Rust 2024 edition it will require `unsafe`. Currently android_main runs before other threads, but it's fragile. Better: pass the path directly.

- [ ] **Step 1: Add a static for Android data dir**

In `src/config.rs`, add at the top:

```rust
#[cfg(target_os = "android")]
static ANDROID_DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_android_data_dir(path: std::path::PathBuf) {
    let _ = ANDROID_DATA_DIR.set(path);
}
```

- [ ] **Step 2: Update `app_config_dir` and `app_data_dir`**

Replace the `#[cfg(target_os = "android")]` blocks:

```rust
pub fn app_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        ANDROID_DATA_DIR.get().cloned()
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::config_dir().map(|d| d.join("mini_midi_synth"))
    }
}

pub fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        ANDROID_DATA_DIR.get().cloned()
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mini_midi_synth")
    }
}
```

- [ ] **Step 3: Update android_main to use the new function**

In `src/main.rs`, replace:
```rust
    if let Some(path) = app.internal_data_path() {
        std::env::set_var("MINI_SYNTH_DATA_DIR", path.to_string_lossy().as_ref());
        log::info!("data dir: {}", path.display());
    }
```
With:
```rust
    if let Some(path) = app.internal_data_path() {
        log::info!("data dir: {}", path.display());
        crate::config::set_android_data_dir(path.to_path_buf());
    }
```

- [ ] **Step 4: Build and test both platforms**

```bash
cargo build --release && cargo test
bash android/build_rust.sh
```

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/main.rs
git commit -m "fix: replace env var with OnceLock for Android data dir"
```

---

### Task 3: Fix SynthActivity lifecycle and thread safety

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`

Two issues:
1. `openPairedBluetoothMidiDevices()` called before `super.onCreate()` — Activity context may not be fully available
2. `mOpenDevices`/`mBridges` modified from async callback without synchronization

- [ ] **Step 1: Move BLE init after super.onCreate()**

```java
@Override
protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    // BLE MIDI init AFTER super.onCreate() — Activity context is now fully available.
    // native_main runs in a separate thread, so it won't block on this.
    openPairedBluetoothMidiDevices();
}
```

- [ ] **Step 2: Move permission request to a post-delayed handler**

Replace the `requestPermissions` block in `openPairedBluetoothMidiDevices()`:

```java
if (checkSelfPermission(android.Manifest.permission.BLUETOOTH_CONNECT)
        != PackageManager.PERMISSION_GRANTED) {
    Log.i(TAG, "Requesting BLUETOOTH_CONNECT permission");
    // Post to handler — requesting permissions before Activity is fully
    // visible can cause issues on some devices
    new Handler(Looper.getMainLooper()).post(() ->
        requestPermissions(
            new String[]{android.Manifest.permission.BLUETOOTH_CONNECT}, 1));
    return;
}
```

- [ ] **Step 3: Synchronize list access**

Lists are only accessed from the main looper thread (onCreate, onDestroy, onDeviceOpened callback all run on main). The Handler callback in `openBondedDevices` uses `new Handler(Looper.getMainLooper())` — so all access is single-threaded. No synchronization needed, but add a comment:

```java
// All access to mOpenDevices and mBridges happens on the main looper thread:
// - onCreate (main)
// - onDestroy (main)
// - OnDeviceOpenedListener callback (posted to main looper via handler)
// No synchronization needed.
private final List<MidiDevice> mOpenDevices = new ArrayList<>();
private final List<MidiBridge> mBridges = new ArrayList<>();
```

- [ ] **Step 4: Build APK**

```bash
cd android && ./gradlew assembleRelease
```

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "fix: move BLE MIDI init after super.onCreate(), add thread safety docs"
```

---

### Task 4: Fix MidiBridge visibility and manifest cleanup

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/MidiBridge.java:18`
- Modify: `android/app/src/main/AndroidManifest.xml:7`

- [ ] **Step 1: Make native method package-private**

In `MidiBridge.java`, change:
```java
public static native void onMidiData(byte[] data);
```
To:
```java
static native void onMidiData(byte[] data);
```

Package-private — only classes in `com.minimidisynth` can call it. JNI resolution works regardless of visibility modifier.

- [ ] **Step 2: Remove deprecated READ_EXTERNAL_STORAGE**

In `AndroidManifest.xml`, remove:
```xml
<uses-permission android:name="android.permission.READ_EXTERNAL_STORAGE" />
```

This does nothing on targetSdk 35 (Android 13+). The `/sdcard/Download/` scan works via default file access on Android 11+ for non-media files.

- [ ] **Step 3: Build APK**

```bash
cd android && ./gradlew assembleRelease
```

- [ ] **Step 4: Commit**

```bash
git add android/app/src/main/java/com/minimidisynth/MidiBridge.java \
        android/app/src/main/AndroidManifest.xml
git commit -m "fix: MidiBridge visibility, remove deprecated permission"
```

---

### Task 5: Fix panic hook + abort interaction

**Files:**
- Modify: `src/main.rs:793-795`

With `panic = "abort"` in release profile, the panic hook runs but `log::error!` may not flush before abort. Add explicit flush.

- [ ] **Step 1: Add stderr flush and sleep to panic hook**

```rust
std::panic::set_hook(Box::new(|info| {
    let msg = format!("PANIC: {info}");
    log::error!("{msg}");
    // Also write to stderr in case android_logger fails
    eprintln!("{msg}");
    // Give logger time to flush before abort kills the process
    std::thread::sleep(std::time::Duration::from_millis(100));
}));
```

- [ ] **Step 2: Build and test**

```bash
cargo build --release && cargo test
```

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "fix: ensure panic message is flushed before abort"
```

---

### Task 6: Verify all fixes on device

**Files:** None (test only)

- [ ] **Step 1: Build full release .so + APK**

```bash
bash android/build_rust.sh
cd android && ./gradlew assembleRelease
```

- [ ] **Step 2: Deploy and launch**

```bash
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
adb shell am force-stop com.minimidisynth
adb shell am start -n com.minimidisynth/com.minimidisynth.SynthActivity
```

- [ ] **Step 3: Check logs are clean**

```bash
adb logcat -c && sleep 10 && adb logcat -d -s "MiniMidiSynth"
```

Expected: Only startup logs (android_main started, data dir, BLE MIDI bridges). No per-message spam, no warnings.

- [ ] **Step 4: Play notes, verify no artifacts**

Press keys on BLE MIDI keyboard. Sound should be clean, no buzzing.

- [ ] **Step 5: Verify desktop still works**

```bash
cargo run --release &
sleep 3 && kill %1
cargo test
```

Expected: 26/26 tests pass, app launches without errors.

- [ ] **Step 6: Final commit**

```bash
git add -A
git commit -m "chore: Android deep audit complete — all issues fixed"
```

---

## Verified Clean (no action needed)

These were checked and are already correct:

- **Ring buffers**: fixed size (256 MIDI, 256 control, 64 feedback)
- **Audio DSP**: fixed-size arrays, zero allocations in RT thread
- **Arc usage**: no cycles, proper reference counting
- **Thread lifecycle**: midir disabled on Android, audio stream owned by AudioBackend
- **JNI OnceLock**: set once, never grows, no race condition
- **MidiDevice/MidiBridge cleanup**: closed in onDestroy()
- **Undo stack**: bounded to 50 entries (VecDeque)
- **Preset dedup**: dedup on save by name
- **No `Box::leak`, `mem::forget`**: none found
- **Synth engine**: zero Android-specific changes, fully platform-independent
