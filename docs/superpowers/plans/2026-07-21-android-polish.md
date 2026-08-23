# Android Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Polish the Android version — touch UI, BLE reconnect, USB hot-plug, keep screen on, lifecycle pause, ADPF performance hints.

**Architecture:** Each task is independent. Java changes in SynthActivity.java, Rust changes in gui/mod.rs, audio.rs, main.rs. All Android-specific via cfg gates.

**Tech Stack:** eframe/egui, JNI, MidiManager.DeviceCallback, ADPF PerformanceHintManager, WindowManager.LayoutParams

---

## Review notes (verified via web search)

| Task | Best practice source | Key finding |
|------|---------------------|-------------|
| Touch UI | [Android Accessibility](https://support.google.com/accessibility/android/answer/7101858) | Min 48x48dp touch targets, 8dp spacing |
| BLE reconnect | [DEV.to BLE guide](https://dev.to/ble_advertiser/robust-ble-preventing-disconnections-and-implementing-auto-reconnect-on-android-12-with-in8) | Exponential backoff, close old GATT before retry |
| USB hot-plug | [Android MidiManager docs](https://developer.android.com/reference/android/media/midi/MidiManager) | registerDeviceCallback + getDevices() before registration |
| Keep screen on | [Android docs](https://developer.android.com/develop/background-work/background-tasks/awake/screen-on) | FLAG_KEEP_SCREEN_ON, NOT WAKE_LOCK. Auto-cleared on background. |
| Lifecycle | [NVIDIA Android Lifecycle](https://developer.download.nvidia.com/assets/mobile/docs/android_lifecycle_app_note.pdf) | Stop audio on pause, resume on focus+resume |
| ADPF | [Android NDK ADPF](https://developer.android.com/ndk/reference/group/a-performance-hint) | Need actual audio thread TID, not main thread |

---

### Task 1: Touch-friendly UI scaling

**Files:**
- Modify: `src/gui/mod.rs:370-390`

- [ ] **Step 1: Add Android-specific touch sizes**

In `src/gui/mod.rs`, after the existing `set_pixels_per_point(2.5)` block, add Android-specific style override:

```rust
#[cfg(target_os = "android")]
ctx.style_mut(|style| {
    style.spacing.interact_size = egui::vec2(48.0, 36.0); // Material Design min touch target
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.item_spacing = egui::vec2(10.0, 8.0);
});
```

Do NOT change `slider_width` — it's already good at 250.0 (scaled by pixels_per_point to ~100dp).

- [ ] **Step 2: Build and test**

```bash
cargo build --release && cargo test
bash android/build_rust.sh && cd android && ./gradlew assembleRelease
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
```

- [ ] **Step 3: Commit**

```bash
git add src/gui/mod.rs
git commit -m "feat: touch-friendly interact_size 48x36dp on Android"
```

---

### Task 2: BLE MIDI auto-reconnect

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`

Best practice: check if BLE devices are actually delivering data, retry with backoff. Simple approach: re-call `openPairedBluetoothMidiDevices()` periodically. `openBluetoothDevice()` is idempotent if device is already open.

- [ ] **Step 1: Add reconnect timer with backoff**

In SynthActivity.java, add fields:

```java
private final Handler mReconnectHandler = new Handler(Looper.getMainLooper());
private int mReconnectAttempts = 0;
private static final int MAX_RECONNECT_ATTEMPTS = 10;
private static final long RECONNECT_BASE_DELAY_MS = 3000;

private final Runnable mReconnectRunnable = new Runnable() {
    @Override
    public void run() {
        if (mReconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
            Log.i(TAG, "BLE reconnect: max attempts reached, stopping");
            return;
        }
        mReconnectAttempts++;
        Log.i(TAG, "BLE reconnect attempt " + mReconnectAttempts);
        openPairedBluetoothMidiDevices();

        // Exponential backoff: 3s, 6s, 12s, 24s... capped at 30s
        long delay = Math.min(RECONNECT_BASE_DELAY_MS * (1L << (mReconnectAttempts - 1)), 30000);
        mReconnectHandler.postDelayed(this, delay);
    }
};
```

- [ ] **Step 2: Start timer in onCreate, stop in onDestroy**

In `onCreate()`, after `openPairedBluetoothMidiDevices()`:
```java
mReconnectHandler.postDelayed(mReconnectRunnable, RECONNECT_BASE_DELAY_MS);
```

In `onDestroy()`:
```java
mReconnectHandler.removeCallbacks(mReconnectRunnable);
```

- [ ] **Step 3: Reset counter when bridge successfully connects**

In `connectBridge()`, after successfully adding a bridge:
```java
mReconnectAttempts = 0; // Reset backoff on success
```

- [ ] **Step 4: Build, deploy, test**

```bash
cd android && ./gradlew assembleRelease
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
```

Test: start app, turn off BLE keyboard, wait, turn on — should reconnect.

- [ ] **Step 5: Commit**

```bash
git add android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "feat: BLE MIDI auto-reconnect with exponential backoff"
```

---

### Task 3: USB MIDI hot-plug via DeviceCallback

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`
- Modify: `src/amidi.rs` (add close method for AMidi on disconnect)

- [ ] **Step 1: Add JNI close function for AMidi**

In `src/main.rs`, add:

```rust
/// JNI: close AMidi ports when USB device disconnected
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_SynthActivity_closeUsbMidiNative<'local>(
    _env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
) {
    if let Some(amidi_port) = AMIDI_PORT.get() {
        amidi_port.close();
        log::info!("[amidi] USB MIDI ports closed");
    }
}
```

- [ ] **Step 2: Register DeviceCallback in SynthActivity.java**

Add field and method:

```java
private MidiManager.DeviceCallback mDeviceCallback;

private static native void closeUsbMidiNative();

private void registerMidiDeviceCallback() {
    if (mMidiManager == null) return;

    mDeviceCallback = new MidiManager.DeviceCallback() {
        @Override
        public void onDeviceAdded(MidiDeviceInfo info) {
            if (info.getType() == MidiDeviceInfo.TYPE_USB) {
                String name = info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME);
                Log.i(TAG, "USB MIDI hot-plug: connected " + name);
                openUsbMidiDevices();
            }
        }

        @Override
        public void onDeviceRemoved(MidiDeviceInfo info) {
            if (info.getType() == MidiDeviceInfo.TYPE_USB) {
                String name = info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME);
                Log.i(TAG, "USB MIDI hot-plug: disconnected " + name);
                closeUsbMidiNative(); // Close AMidi ports so audio callback stops polling dead port
            }
        }
    };

    mMidiManager.registerDeviceCallback(mDeviceCallback,
        new Handler(Looper.getMainLooper()));
    Log.i(TAG, "USB MIDI hot-plug callback registered");
}
```

- [ ] **Step 3: Call in onCreate, unregister in onDestroy**

In `onCreate()`, after `openUsbMidiDevices()`:
```java
registerMidiDeviceCallback();
```

In `onDestroy()`:
```java
if (mMidiManager != null && mDeviceCallback != null) {
    mMidiManager.unregisterDeviceCallback(mDeviceCallback);
}
```

- [ ] **Step 4: Build both platforms, test**

```bash
cargo build --release && cargo test
bash android/build_rust.sh && cd android && ./gradlew assembleRelease
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
```

Plug/unplug USB MIDI keyboard. Check logcat for "hot-plug" messages.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "feat: USB MIDI hot-plug detection + AMidi cleanup on disconnect"
```

---

### Task 4: Keep screen on

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`

Google docs: "Use FLAG_KEEP_SCREEN_ON, never WAKE_LOCK in an activity. Auto-cleared when app backgrounds."

- [ ] **Step 1: Add one line in onCreate**

In `onCreate()`, after `super.onCreate()`:

```java
getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
```

- [ ] **Step 2: Remove WAKE_LOCK permission from manifest (not needed)**

In `AndroidManifest.xml`, remove:
```xml
<uses-permission android:name="android.permission.WAKE_LOCK" />
```

FLAG_KEEP_SCREEN_ON doesn't need WAKE_LOCK permission.

- [ ] **Step 3: Build and deploy**

```bash
cd android && ./gradlew assembleRelease
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
```

- [ ] **Step 4: Commit**

```bash
git add android/app/src/main/java/com/minimidisynth/SynthActivity.java android/app/src/main/AndroidManifest.xml
git commit -m "feat: keep screen on via FLAG_KEEP_SCREEN_ON"
```

---

### Task 5: Lifecycle — pause audio on background

**Files:**
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`
- Modify: `src/main.rs`
- Modify: `src/audio.rs`

Best practice: stop rendering and audio when app is paused. Use atomic flag — audio callback writes silence when paused. No stream teardown/rebuild needed.

- [ ] **Step 1: Add paused AtomicBool in AudioBackend**

In `src/audio.rs`, add field to `AudioBackend`:
```rust
pub paused: Arc<AtomicBool>,
```

Initialize in `new()`:
```rust
let paused = Arc::new(AtomicBool::new(false));
let paused_cb = paused.clone();
```

In audio callback, at the very start:
```rust
// If app is paused (backgrounded), output silence
if paused_cb.load(Ordering::Relaxed) {
    for sample in data.iter_mut() { *sample = 0.0; }
    return;
}
```

Return in `Ok`:
```rust
Ok((Self { stream, disconnected, paused }, final_sr))
```

- [ ] **Step 2: Add OnceLock for paused flag**

In `src/main.rs`:
```rust
#[cfg(target_os = "android")]
static AUDIO_PAUSED: std::sync::OnceLock<Arc<AtomicBool>> = std::sync::OnceLock::new();
```

In `init_common()`, after AudioBackend::new:
```rust
#[cfg(target_os = "android")]
{
    let _ = AUDIO_PAUSED.set(audio_backend.paused.clone());
}
```

Add JNI functions:
```rust
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_SynthActivity_setAudioPaused<'local>(
    _env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    paused: jni::sys::jboolean,
) {
    if let Some(flag) = AUDIO_PAUSED.get() {
        flag.store(paused != 0, Ordering::Relaxed);
        log::info!("[lifecycle] audio paused: {}", paused != 0);
    }
}
```

- [ ] **Step 3: Call from Java lifecycle**

In SynthActivity.java:
```java
private static native void setAudioPaused(boolean paused);

@Override
protected void onPause() {
    super.onPause();
    setAudioPaused(true);
    Log.i(TAG, "onPause — audio paused");
}

@Override
protected void onResume() {
    super.onResume();
    setAudioPaused(false);
    Log.i(TAG, "onResume — audio resumed");
}
```

- [ ] **Step 4: Update reconnect_audio in gui/settings.rs**

`reconnect_audio()` creates new AudioBackend — update AUDIO_PAUSED OnceLock:

Actually, OnceLock can only be set once. Use the paused flag from the App struct instead. Add `paused: Arc<AtomicBool>` to gui App, set from reconnect_audio.

Simpler: store in gui App:
```rust
pub audio_paused: std::sync::Arc<std::sync::atomic::AtomicBool>,
```

In reconnect_audio, after creating new AudioBackend:
```rust
self.audio_paused = audio_backend.paused.clone();
#[cfg(target_os = "android")]
{
    let _ = crate::AUDIO_PAUSED.set(audio_backend.paused.clone());
    // Note: OnceLock can't be re-set. Use a different mechanism.
}
```

Hmm, OnceLock problem again. Use `AtomicPtr` or share the same `Arc<AtomicBool>` from the start:

Better approach: create `paused` Arc BEFORE AudioBackend, pass it in. Same Arc survives reconnect.

In `init_common()`:
```rust
let audio_paused = Arc::new(AtomicBool::new(false));
```

Pass to AudioBackend::new, store in OnceLock. On reconnect, reuse same Arc.

- [ ] **Step 5: Build and test**

```bash
cargo build --release && cargo test
bash android/build_rust.sh && cd android && ./gradlew assembleRelease
```

- [ ] **Step 6: Commit**

```bash
git add src/audio.rs src/main.rs src/gui/mod.rs src/gui/settings.rs \
        android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "feat: pause audio when app backgrounded, resume on foreground"
```

---

### Task 6: ADPF Performance Hints

**Files:**
- Modify: `src/audio.rs`
- Modify: `src/main.rs`

ADPF needs the **actual audio thread TID** (not main thread). We get it from inside the audio callback via `libc::gettid()` on first invocation, store in atomic, then Java creates hint session.

- [ ] **Step 1: Capture audio thread TID**

In `src/audio.rs`, add to AudioBackend:
```rust
pub audio_thread_id: Arc<AtomicI32>,
```

In audio callback, on first invocation:
```rust
static FIRST_CALL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
if FIRST_CALL.swap(false, Ordering::Relaxed) {
    #[cfg(target_os = "android")]
    {
        let tid = unsafe { libc::gettid() };
        audio_tid.store(tid, Ordering::Relaxed);
    }
}
```

Wait — `libc` is only available on Linux target. On Android we disabled libc dependency. Need to use `ndk` crate or raw syscall.

Actually simpler: use `std::thread::current().id()` — but that's not the Linux TID that ADPF needs.

Use `unsafe { libc::syscall(libc::SYS_gettid) as i32 }` — but libc is not available on Android in our build.

**Alternative:** Get TID from Java side. cpal/Oboe creates the audio thread. We can't easily get its TID.

**Simplest approach:** Skip TID from audio thread. Create ADPF hint session for the **process** instead — less precise but still helps. Or use NDK API `APerformanceHint_createSession` directly from Rust if available.

**Decision:** ADPF is complex and the benefit on Dimensity 9400+ is marginal. Defer to Phase 4. Mark as skipped.

**SKIP this task.** Record in tech debt for testing on lower-end devices.

---

## Summary

| Task | Description | Status |
|------|-------------|--------|
| 1 | Touch UI interact_size | Ready |
| 2 | BLE reconnect with backoff | Ready |
| 3 | USB hot-plug + AMidi cleanup | Ready |
| 4 | Keep screen on | Ready (1 line) |
| 5 | Lifecycle pause audio | Ready (complex but valuable) |
| 6 | ~~ADPF hints~~ | **Skipped** — needs audio thread TID, complex, marginal benefit on flagship |
