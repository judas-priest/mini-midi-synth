# Android Cross-Platform Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make mini_midi_synth build and run on Android (aarch64) while keeping Linux x86_64 and Linux aarch64 working.

**Architecture:** Platform differences isolated behind `#[cfg()]` and feature flags. Synth engine (`synth/`) is already platform-independent. Audio: cpal (AAudio on Android). MIDI: midir 0.11 (AMidi on Android). GUI: eframe with `android-native-activity`. Packaged as APK via Gradle + cargo-ndk.

**Tech Stack:** Rust, eframe 0.31 (`android-native-activity`), cpal 0.15, midir 0.11, cargo-ndk, Gradle, Android SDK/NDK

---

## Research Summary

| Component | Finding | Decision |
|-----------|---------|----------|
| **eframe 0.31** | Supports Android via `android-native-activity` feature. **No `android_app` field on NativeOptions** — must pass `AndroidApp` via `event_loop_builder` callback using `winit::platform::android::EventLoopBuilderExtAndroid::with_android_app()`. | Use eframe 0.31, event_loop_builder API |
| **cpal 0.15** | Supports Android (AAudio via Oboe internally). `jack` feature won't compile on Android — must be conditional. `realtime` feature may not exist in 0.15. | Conditional cpal features; omit `realtime` for now |
| **midir 0.11** | Supports Android (API 29+, NDK AMidi + JNI). USB + BLE MIDI via MidiManager. Manifest: `<uses-feature android:name="android.software.midi">`. | Upgrade midir 0.10 → 0.11 |
| **Denormals** | `no_denormals` crate handles x86 MXCSR + aarch64 FPCR. | Replace manual asm |
| **Build** | Gradle shell + cargo-ndk. NativeActivity, no custom Java needed. `hasCode="false"`. | Minimal Gradle, no Kotlin |
| **input.rs** | Linux-only (evdev/epoll). | Gate `#[cfg(target_os = "linux")]` |
| **File picker** | `zenity`/`kdialog` in midi_seq.rs — desktop only. | Gate `#[cfg(not(target_os = "android"))]` |
| **dirs crate** | Returns None/wrong paths on Android. 7 call sites in preset.rs, config.rs, gui/mod.rs. | Centralize into `config::data_dir()` with Android env var fallback |

## File Map

### Files to modify:

| File | Change |
|------|--------|
| `Cargo.toml` | Feature flags, conditional deps, upgrade midir, add no_denormals, add [lib] section |
| `src/main.rs` | Add `android_main`, gate Linux-specific code, unify `run_gui` |
| `src/audio.rs` | Replace MXCSR asm with `no_denormals` |
| `src/input.rs` | Gate entire module `#[cfg(target_os = "linux")]` |
| `src/config.rs` | Add `data_dir()` helper, make `config_path()` Android-aware |
| `src/preset.rs` | Use `config::data_dir()` for all 5 dir functions |
| `src/gui/mod.rs` | Gate `cpal::HostId`, use `config::data_dir()` for sf2_dir |
| `src/gui/settings.rs` | Gate audio host dropdown, xdg-open |
| `src/gui/midi_seq.rs` | Gate zenity/kdialog file picker |

### Files to create:

| File | Purpose |
|------|---------|
| `android/build.gradle.kts` | Root Gradle build |
| `android/settings.gradle.kts` | Gradle settings |
| `android/gradle.properties` | Gradle config |
| `android/local.properties` | SDK path |
| `android/app/build.gradle.kts` | App module |
| `android/app/src/main/AndroidManifest.xml` | NativeActivity, MIDI feature |
| `android/build_rust.sh` | cargo-ndk build helper |
| `.cargo/config.toml` | NDK linker config |

---

## Phase 1: Platform-Independent Refactoring (Linux stays working)

### Task 1: Replace manual MXCSR with `no_denormals` crate

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/audio.rs:133-188`

- [ ] **Step 1: Add dependency**

In `Cargo.toml` `[dependencies]` section, add:
```toml
no_denormals = "0.1"
```

- [ ] **Step 2: Replace MXCSR blocks in audio callback**

In `src/audio.rs`, the audio callback has two `#[cfg(target_arch = "x86_64")]` blocks (lines 135-142 and 184-187) that set/restore MXCSR. Remove both blocks and wrap the entire callback body with `no_denormals`:

```rust
move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
    unsafe { no_denormals::no_denormals(|| {
        // If JACK changed the sample rate, update the synth immediately.
        let new_sr = jack_sr_cb.load(Ordering::Relaxed);
        if new_sr != synth_sr {
            synth_sr = new_sr;
            synth.set_sample_rate(new_sr as f32);
        }

        while let Ok(event) = ctrl_rx.pop() {
            synth.handle_control(event);
        }

        let total_frames = data.len() / channels;
        let mut frame_offset = 0;

        while frame_offset < total_frames {
            let block_len = (total_frames - frame_offset).min(crate::synth::BLOCK_SIZE);

            while let Ok(event) = midi_rx.pop() {
                synth.handle_event(event);
            }

            let mut bl = [0.0f32; crate::synth::BLOCK_SIZE];
            let mut br = [0.0f32; crate::synth::BLOCK_SIZE];
            synth.tick_block(&mut bl[..block_len], &mut br[..block_len]);

            for i in 0..block_len {
                let left = soft_limit(bl[i]);
                let right = soft_limit(br[i]);
                let base = (frame_offset + i) * channels;
                if channels >= 2 {
                    data[base] = left;
                    data[base + 1] = right;
                    for s in data[base + 2..base + channels].iter_mut() {
                        *s = 0.0;
                    }
                } else {
                    data[base] = (left + right) * 0.5;
                }
            }

            frame_offset += block_len;
        }
    })};
},
```

- [ ] **Step 3: Build and test**

Run: `cargo build --release && cargo test`
Expected: Compiles, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/audio.rs Cargo.lock
git commit -m "$(cat <<'EOF'
refactor: replace manual MXCSR asm with no_denormals crate

Supports both x86_64 (MXCSR DAZ+FTZ) and aarch64 (FPCR FTZ).
EOF
)"
```

---

### Task 2: Gate Linux-specific code

**Files:**
- Modify: `src/main.rs` (module declaration, run_headless)
- Modify: `src/gui/midi_seq.rs` (file picker)
- Modify: `src/gui/settings.rs` (xdg-open)

- [ ] **Step 1: Gate `input` module**

In `src/main.rs`, change:
```rust
mod input;
```
to:
```rust
#[cfg(target_os = "linux")]
mod input;
```

- [ ] **Step 2: Gate evdev usage in `run_headless`**

In `run_headless()`, wrap the keyboard thread spawn (line ~362):
```rust
#[cfg(target_os = "linux")]
let kb_rx = input::spawn_keyboard_thread(
    &c.config.ui.keybinds,
    shutdown_flag.clone(),
);
#[cfg(not(target_os = "linux"))]
let kb_rx: Option<(std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<key_action::KeyAction>)> = None;
```

- [ ] **Step 3: Gate sigaction in `run_headless`**

Wrap the signal handler block (lines ~256-266) with `#[cfg(unix)]`:
```rust
#[cfg(unix)]
{
    extern "C" fn signal_handler(_sig: i32) {
        SHUTDOWN.store(true, Ordering::SeqCst);
    }
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
    }
}
```

- [ ] **Step 4: Gate file picker in midi_seq.rs**

In `src/gui/midi_seq.rs`, wrap `open_midi_file_dialog()` (lines 323-365):
```rust
#[cfg(not(target_os = "android"))]
fn open_midi_file_dialog() -> Option<String> {
    // ... existing code unchanged ...
}

#[cfg(target_os = "android")]
fn open_midi_file_dialog() -> Option<String> {
    None // No native file picker on Android yet
}
```

- [ ] **Step 5: Gate xdg-open in settings.rs**

In `src/gui/settings.rs` line 550, wrap the `xdg-open` call:
```rust
#[cfg(not(target_os = "android"))]
{
    let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
}
```

- [ ] **Step 6: Build and test**

Run: `cargo build --release && cargo test`
Expected: Compiles, all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/main.rs src/gui/midi_seq.rs src/gui/settings.rs
git commit -m "$(cat <<'EOF'
refactor: gate Linux-specific code for Android portability

Gate evdev input, sigaction, zenity/kdialog file picker, xdg-open.
EOF
)"
```

---

### Task 3: Gate `cpal::HostId` in GUI

**Files:**
- Modify: `src/gui/mod.rs`
- Modify: `src/gui/settings.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Gate import and field**

In `src/gui/mod.rs` line 18, change:
```rust
use cpal::HostId;
```
to:
```rust
#[cfg(not(target_os = "android"))]
use cpal::HostId;
```

Gate the `App` struct field (line 224):
```rust
#[cfg(not(target_os = "android"))]
pub available_hosts: Vec<(cpal::HostId, &'static str)>,
```

- [ ] **Step 2: Gate audio host selection in settings.rs**

Find the audio backend ComboBox in settings.rs and wrap it:
```rust
#[cfg(not(target_os = "android"))]
{
    // existing host selection code
}
```

- [ ] **Step 3: Gate CommonInit fields and App construction in main.rs**

In `CommonInit` struct, fields `available_hosts`, `host_name`, `host_idx`, `is_jack` are already `#[cfg(feature = "gui")]`. Add Android gates where they're used in App construction:

```rust
#[cfg(not(target_os = "android"))]
current_host: c.host_name.to_string(),
#[cfg(target_os = "android")]
current_host: "AAudio".to_string(),

#[cfg(not(target_os = "android"))]
available_hosts: c.available_hosts.clone(),

#[cfg(not(target_os = "android"))]
is_jack: c.is_jack,
#[cfg(target_os = "android")]
is_jack: false,
```

- [ ] **Step 4: Build and test**

Run: `cargo build --release && cargo test`

- [ ] **Step 5: Commit**

```bash
git add src/gui/mod.rs src/gui/settings.rs src/main.rs
git commit -m "$(cat <<'EOF'
refactor: gate cpal::HostId and audio host UI for Android
EOF
)"
```

---

### Task 4: Centralize data directories for Android

**Files:**
- Modify: `src/config.rs`
- Modify: `src/preset.rs`
- Modify: `src/gui/mod.rs`

There are 7 places using `dirs::config_dir()` / `dirs::data_dir()`:
- `config.rs:148` — `config_path()`
- `preset.rs:449` — `user_preset_dir()`
- `preset.rs:493` — `external_preset_dir()`
- `preset.rs:501` — `wavetable_dir()`
- `preset.rs:558` — `drum_kit_dir()`
- `preset.rs:705` — `performance_dir()`
- `gui/mod.rs:200` — `sf2_dir()`

- [ ] **Step 1: Add `data_dir()` and `config_dir()` helpers to config.rs**

Add to `src/config.rs`:

```rust
/// Base config directory (~/.config/mini_midi_synth or Android internal).
pub fn app_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        std::env::var("MINI_SYNTH_DATA_DIR").ok().map(PathBuf::from)
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::config_dir().map(|d| d.join("mini_midi_synth"))
    }
}

/// Base data directory (~/.local/share/mini_midi_synth or Android internal).
pub fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        std::env::var("MINI_SYNTH_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("."))
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mini_midi_synth")
    }
}
```

Update `config_path()`:
```rust
fn config_path() -> Option<PathBuf> {
    app_config_dir().map(|d| d.join("config.json"))
}
```

- [ ] **Step 2: Update preset.rs**

Replace all `dirs::` calls:
```rust
fn user_preset_dir() -> Option<PathBuf> {
    crate::config::app_config_dir().map(|d| d.join("presets"))
}

pub fn external_preset_dir() -> PathBuf {
    crate::config::app_data_dir().join("presets")
}

pub fn wavetable_dir() -> PathBuf {
    crate::config::app_data_dir().join("wavetables")
}

fn drum_kit_dir() -> Option<PathBuf> {
    crate::config::app_config_dir().map(|d| d.join("drum_kits"))
}

fn performance_dir() -> Option<PathBuf> {
    crate::config::app_config_dir().map(|d| d.join("performances"))
}
```

- [ ] **Step 3: Update sf2_dir in gui/mod.rs**

```rust
fn sf2_dir() -> std::path::PathBuf {
    crate::config::app_data_dir().join("sf2")
}
```

- [ ] **Step 4: Build and test**

Run: `cargo build --release && cargo test`
Expected: On Linux, paths are identical to before.

- [ ] **Step 5: Commit**

```bash
git add src/config.rs src/preset.rs src/gui/mod.rs
git commit -m "$(cat <<'EOF'
refactor: centralize data dirs, support Android internal storage
EOF
)"
```

---

### Task 5: Upgrade midir and set up Cargo.toml features

**Files:**
- Modify: `Cargo.toml`

This is the final Cargo.toml configuration. One clear approach:

- [ ] **Step 1: Apply all Cargo.toml changes**

The complete `Cargo.toml` should become:

```toml
[package]
name = "mini_midi_synth"
version = "0.1.0"
edition = "2021"

[lib]
name = "mini_midi_synth"
path = "src/main.rs"
crate-type = ["cdylib"]

[[bin]]
name = "mini_midi_synth"
path = "src/main.rs"

[features]
default = ["gui", "desktop"]
gui = ["dep:eframe"]
desktop = ["eframe/x11", "eframe/wayland"]
android-app = ["eframe/android-native-activity"]

[dependencies]
eframe = { version = "0.31", default-features = false, features = ["default_fonts", "glow", "persistence"], optional = true }
midir = "0.11"
wmidi = "4"
rtrb = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
dirs = "6"
anyhow = "1"
rustysynth = { path = "vendor/rustysynth" }
mi-plaits-dsp = { path = "vendor/mi-plaits-dsp-rs" }
midly = "0.5"
no_denormals = "0.1"

# Desktop-only
[target.'cfg(target_os = "linux")'.dependencies]
cpal = { version = "0.15", features = ["jack"] }
libc = "0.2"

# Android-only (cpal without jack)
[target.'cfg(target_os = "android")'.dependencies]
cpal = { version = "0.15" }

# All other non-Linux non-Android platforms (fallback)
[target.'cfg(not(any(target_os = "linux", target_os = "android")))'.dependencies]
cpal = { version = "0.15" }

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"

[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 2
```

**Key decisions:**
- `[lib]` with `crate-type = ["cdylib"]` — needed for Android .so. On desktop, `cargo build` builds the `[[bin]]` target; `cargo build --lib` builds the cdylib (unused on desktop but harmless).
- `desktop` feature activates x11/wayland. `android-app` activates android-native-activity.
- cpal is target-conditional: `jack` only on Linux.
- midir upgraded to 0.11.
- `libc` only on Linux.

- [ ] **Step 2: Build for Linux (default features)**

Run: `cargo build --release`
Expected: Compiles with features `gui` + `desktop`.

Run: `cargo test`
Expected: All tests pass.

- [ ] **Step 3: Verify feature combination for Android build**

Run: `cargo check --no-default-features --features gui,android-app --target aarch64-linux-android 2>&1 | head -5`
Expected: May fail (no NDK yet) but should show the right feature resolution.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "$(cat <<'EOF'
build: Android-ready Cargo.toml with conditional features

- desktop feature: x11, wayland, jack, libc
- android-app feature: android-native-activity
- midir 0.10 → 0.11 (Android AMidi support)
- [lib] cdylib for Android .so output
EOF
)"
```

---

### Task 6: Add `android_main` entry point and unify `run_gui`

**Files:**
- Modify: `src/main.rs`

One unified `run_gui` function serves both desktop and Android. The `android_main` entry point sets the data dir and calls `run_gui`.

- [ ] **Step 1: Gate `make_piano_icon()` behind non-Android**

```rust
#[cfg(feature = "gui")]
#[cfg(not(target_os = "android"))]
fn make_piano_icon() -> eframe::egui::IconData {
    // ... existing code unchanged ...
}
```

- [ ] **Step 2: Modify `run_gui` to accept Android app via cfg'd parameter**

Change the function signature:
```rust
#[cfg(feature = "gui")]
fn run_gui(
    #[cfg(target_os = "android")]
    android_app: winit::platform::android::activity::AndroidApp,
) -> Result<()> {
```

Note: `#[cfg]` on function parameters is stable since Rust 1.69.

- [ ] **Step 3: Modify viewport and NativeOptions in `run_gui`**

Replace the viewport + NativeOptions construction (near the end of `run_gui`):

```rust
    #[cfg(not(target_os = "android"))]
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth")
        .with_inner_size([c.config.ui.window_width, c.config.ui.window_height])
        .with_min_inner_size([500.0, 300.0])
        .with_icon(make_piano_icon());

    #[cfg(target_os = "android")]
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth");

    #[cfg(target_os = "android")]
    let android_app_clone = android_app.clone();

    let options = eframe::NativeOptions {
        viewport,
        #[cfg(not(target_os = "android"))]
        persist_window: true,
        #[cfg(target_os = "android")]
        event_loop_builder: Some(Box::new(move |builder| {
            use winit::platform::android::EventLoopBuilderExtAndroid;
            builder.with_android_app(android_app_clone);
        })),
        ..Default::default()
    };
```

- [ ] **Step 4: Add `android_main` at end of main.rs**

```rust
// ---------------------------------------------------------------------------
// Android entry point
// ---------------------------------------------------------------------------
#[cfg(target_os = "android")]
#[cfg(feature = "gui")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    // Set data dir from Android internal storage
    if let Some(path) = app.internal_data_path() {
        std::env::set_var("MINI_SYNTH_DATA_DIR", path.to_string_lossy().as_ref());
    }
    run_gui(app).expect("Failed to start synth");
}
```

- [ ] **Step 5: Update `main()` — no changes needed**

`main()` calls `run_gui()` without arguments. Since the `android_app` parameter is cfg'd away on non-Android, `run_gui()` has no parameters on Linux. The existing `main()` code works as-is.

- [ ] **Step 6: Build and test on Linux**

Run: `cargo build --release && cargo test`
Expected: Compiles (all Android code is cfg'd away).

- [ ] **Step 7: Commit**

```bash
git add src/main.rs
git commit -m "$(cat <<'EOF'
feat: add android_main and unify run_gui for desktop/Android

run_gui accepts AndroidApp on Android via cfg'd parameter.
AndroidApp passed to eframe via event_loop_builder callback.
EOF
)"
```

---

## Phase 2: Android Build Setup

### Task 7: Set up Gradle project

**Files to create:** `android/` directory

NativeActivity doesn't need Kotlin code. The Gradle project is minimal — just enough to package the .so into an APK.

- [ ] **Step 1: Create root Gradle files**

`android/settings.gradle.kts`:
```kotlin
rootProject.name = "mini_midi_synth"
include(":app")
```

`android/build.gradle.kts`:
```kotlin
plugins {
    id("com.android.application") version "8.7.0" apply false
}
```

`android/gradle.properties`:
```properties
android.useAndroidX=true
org.gradle.jvmargs=-Xmx2048m
```

`android/local.properties`:
```properties
sdk.dir=/home/dima/.local/opt/android-sdk
```

- [ ] **Step 2: Create app module**

`android/app/build.gradle.kts`:
```kotlin
plugins {
    id("com.android.application")
}

android {
    namespace = "com.minimidisynth"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.minimidisynth"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        ndk {
            abiFilters += "arm64-v8a"
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}
```

- [ ] **Step 3: Create AndroidManifest.xml**

`android/app/src/main/AndroidManifest.xml`:
```xml
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android">

    <uses-feature android:name="android.software.midi" android:required="false" />
    <uses-feature android:name="android.hardware.usb.host" android:required="false" />
    <uses-permission android:name="android.permission.WAKE_LOCK" />

    <application
        android:label="Mini MIDI Synth"
        android:hasCode="false"
        android:theme="@android:style/Theme.NoTitleBar.Fullscreen">

        <activity
            android:name="android.app.NativeActivity"
            android:configChanges="orientation|keyboardHidden|screenSize|screenLayout"
            android:screenOrientation="landscape"
            android:exported="true">

            <meta-data
                android:name="android.app.lib_name"
                android:value="mini_midi_synth" />

            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
```

Note: `hasCode="false"` — pure native app, no Java/Kotlin code needed.

- [ ] **Step 4: Create build_rust.sh**

`android/build_rust.sh`:
```bash
#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo ndk \
    -t arm64-v8a \
    -o android/app/src/main/jniLibs \
    build --release \
    --no-default-features \
    --features gui,android-app

echo "Done: android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so"
```

```bash
chmod +x android/build_rust.sh
```

- [ ] **Step 5: Set up Gradle wrapper**

Copy from the pedometer project or generate:
```bash
cd android
cp -r /home/dima/Projects/pedometer/gradle ./gradle
cp /home/dima/Projects/pedometer/gradlew ./gradlew
cp /home/dima/Projects/pedometer/gradlew.bat ./gradlew.bat
chmod +x gradlew
cd ..
```

- [ ] **Step 6: Verify Gradle setup**

```bash
cd android && ./gradlew tasks 2>&1 | head -10 && cd ..
```

- [ ] **Step 7: Commit**

```bash
git add android/
git commit -m "$(cat <<'EOF'
build: add Android Gradle project shell

NativeActivity + cargo-ndk, minSdk 29, arm64-v8a, landscape.
EOF
)"
```

---

### Task 8: Install Android build prerequisites

- [ ] **Step 1: Install Rust target and cargo-ndk**

```bash
rustup target add aarch64-linux-android
cargo install cargo-ndk
```

- [ ] **Step 2: Ensure NDK is installed**

```bash
ls /home/dima/.local/opt/android-sdk/ndk/ 2>/dev/null
```

If no NDK, install one:
```bash
/home/dima/.local/opt/android-sdk/cmdline-tools/latest/bin/sdkmanager "ndk;27.2.12479018"
```

- [ ] **Step 3: Create `.cargo/config.toml`**

```toml
[target.aarch64-linux-android]
linker = "/home/dima/.local/opt/android-sdk/ndk/NDK_VERSION/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android29-clang"
```

Replace `NDK_VERSION` with the actual installed NDK version directory name.

Also set `ANDROID_NDK_HOME` env var (cargo-ndk needs it):
```bash
export ANDROID_NDK_HOME=/home/dima/.local/opt/android-sdk/ndk/NDK_VERSION
```

- [ ] **Step 4: Test cross-compilation (expect errors)**

```bash
cargo ndk -t arm64-v8a build --no-default-features --features gui,android-app 2>&1 | tail -30
```

This will likely fail with compilation errors — Task 9 handles fixing those.

- [ ] **Step 5: Commit config**

```bash
git add .cargo/config.toml
git commit -m "build: add cargo config for Android NDK"
```

---

### Task 9: Fix Android compilation errors

Iteratively fix whatever breaks when cross-compiling for Android.

**Expected issues:**
1. `libc` references in code that isn't fully gated
2. `winit::platform::android` import path differences
3. eframe `event_loop_builder` type mismatch
4. `cpal::HostId` references missed in Task 3
5. `dirs` crate returning None (logic, not compilation)
6. `midir` 0.11 API differences from 0.10

- [ ] **Step 1: Build, capture all errors**

```bash
cargo ndk -t arm64-v8a build --no-default-features --features gui,android-app 2>&1 | tee /tmp/android_build.log
```

- [ ] **Step 2: Fix errors one by one**

For each error: identify the platform-specific API, gate with `#[cfg]`, provide Android alternative or no-op.

- [ ] **Step 3: Iterate until `cargo ndk` succeeds**

- [ ] **Step 4: Verify Linux still builds**

```bash
cargo build --release && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
fix: resolve Android cross-compilation errors
EOF
)"
```

---

### Task 10: Build APK and deploy

- [ ] **Step 1: Build Rust .so**

```bash
./android/build_rust.sh
ls -la android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so
```

- [ ] **Step 2: Build APK**

```bash
cd android && ./gradlew assembleRelease && cd ..
ls -la android/app/build/outputs/apk/release/app-release.apk
```

- [ ] **Step 3: Deploy to device**

```bash
unset LD_PRELOAD && adb install -r android/app/build/outputs/apk/release/app-release.apk
```

- [ ] **Step 4: Launch and watch logs**

```bash
unset LD_PRELOAD && adb shell am start -n com.minimidisynth/android.app.NativeActivity
```

```bash
unset LD_PRELOAD && adb logcat -s "mini_midi_synth" "eframe" "RustStdoutStderr" | head -100
```

- [ ] **Step 5: Commit working state**

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat: first working Android APK

GUI via eframe glow, audio via cpal AAudio, MIDI via midir AMidi.
EOF
)"
```

---

## Phase 3: Android Polish (Post-MVP)

### Task 11: Touch-friendly UI scaling
- `ctx.set_pixels_per_point(2.5)` on Android for 2800x1272 @ 6.83"
- Increase slider widths, button sizes, piano key touch targets
- Test with finger interaction

### Task 12: Android lifecycle
- Stop audio on app pause, resume on app resume
- Save config on `on_exit`

### Task 13: USB MIDI hot-plug
- Android USB permission dialog
- MIDI device auto-detection on connect/disconnect

### Task 14: Bluetooth MIDI
- midir 0.11 should handle BLE MIDI via MidiManager if device is paired at OS level
- Test with SMK-37 PRO Bluetooth
- If midir doesn't work for BLE: custom JNI bridge to Android BLE MIDI API

---

## Build Commands Summary

```bash
# Linux PC (unchanged)
cargo run --release

# Linux aarch64 (Orange Pi 5)
cross build --target aarch64-unknown-linux-gnu --release

# Android: Build .so
./android/build_rust.sh

# Android: Build APK
cd android && ./gradlew assembleRelease

# Android: Deploy
unset LD_PRELOAD && adb install -r android/app/build/outputs/apk/release/app-release.apk
```
