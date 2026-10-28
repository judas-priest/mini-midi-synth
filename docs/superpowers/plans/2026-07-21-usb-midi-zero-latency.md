# USB MIDI Zero-Latency via AMidi in Audio Callback

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read USB MIDI data directly inside the cpal audio callback via AMidi NDK API, eliminating the 5-15ms Java MidiReceiver latency.

**Architecture:** Java opens USB MidiDevice and passes it to Rust via JNI. Rust calls `AMidiDevice_fromJava()` + `AMidiOutputPort_open()` to get a native port handle stored in `AtomicPtr`. The audio callback polls `AMidiOutputPort_receive()` (non-blocking, RT-safe) inside each block iteration. BLE MIDI continues through Java MidiBridge → JNI as before.

**Tech Stack:** AMidi NDK FFI, JNI (`jni` crate), `AtomicPtr` for RT-safe port handle, cpal audio callback

---

## Review notes (issues found and fixed in this plan)

1. **`AtomicPtr` ordering**: `store(Release)` + `load(Acquire)` — not `Relaxed` on ARM
2. **No MIDI parsing duplication**: extract `parse_midi_message() -> Option<(MidiEvent, usize)>`, use in `parse_and_push`, `parse_midi_to_synth`, and audio callback
3. **AMidi poll inside per-block loop** — not before it. Events must be distributed across blocks
4. **`#[cfg]` inside closure**: use a regular `if cfg!(target_os = "android")` won't work for types. Instead, create two versions of `AudioBackend::new` or pass `Option<Arc<AmidiPort>>` on all platforms
5. **NoteState for USB MIDI**: pass `note_state`/`pad_state` to AudioBackend for GUI display
6. **All USB output ports**: open all, not just first
7. **Race in open_from_java**: store null first, then close old port

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `src/amidi.rs` | Create | AMidi FFI + AtomicPtr wrapper |
| `src/midi.rs` | Modify | Extract `parse_midi_message()` shared parser |
| `src/audio.rs` | Modify | Accept AmidiPort + NoteState, poll in callback |
| `src/main.rs` | Modify | `mod amidi`, OnceLock, JNI function, pass args |
| `src/gui/settings.rs` | Modify | Update `reconnect_audio()` |
| `SynthActivity.java` | Modify | Pass USB MidiDevice to native |

---

### Task 1: Extract shared MIDI parser, create amidi.rs

**Files:**
- Create: `src/amidi.rs`
- Modify: `src/midi.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Extract `parse_midi_message()` in `src/midi.rs`**

Add before `parse_and_push`:

```rust
/// Parse a single MIDI message from raw bytes.
/// Returns (MidiEvent, bytes_consumed) or None on parse failure.
/// Shared by parse_and_push (ring buffer path) and audio callback (direct path).
pub fn parse_midi_message(data: &[u8]) -> Option<(MidiEvent, usize)> {
    let msg = MidiMessage::try_from(data).ok()?;
    let consumed = msg.bytes_size();

    let event = match msg {
        MidiMessage::NoteOn(ch, note, velocity) => {
            let channel = ch.index();
            let n = u8::from(note);
            let v = u8::from(velocity);
            Some(MidiEvent::NoteOn { channel, note: n, velocity: v })
        }
        MidiMessage::NoteOff(ch, note, _) => {
            Some(MidiEvent::NoteOff { channel: ch.index(), note: u8::from(note) })
        }
        MidiMessage::PitchBendChange(ch, bend) => {
            let raw = u16::from(bend) as f32;
            let normalized = (raw - PITCH_BEND_CENTER) / PITCH_BEND_CENTER;
            Some(MidiEvent::PitchBend { channel: ch.index(), value: normalized })
        }
        MidiMessage::ControlChange(ch, cc, val) => {
            let channel = ch.index();
            let cc_num = u8::from(cc);
            let v = u8::from(val);
            if cc_num == 1 {
                Some(MidiEvent::ModWheel { channel, value: v as f32 / MIDI_MAX_VAL })
            } else if (1..=119).contains(&cc_num) && cc_num != 32 {
                Some(MidiEvent::ControlChange { channel, cc: cc_num, value: v })
            } else {
                None
            }
        }
        MidiMessage::ProgramChange(ch, program) => {
            Some(MidiEvent::ProgramChange { channel: ch.index(), program: u8::from(program) })
        }
        MidiMessage::ChannelPressure(ch, pressure) => {
            Some(MidiEvent::Aftertouch { channel: ch.index(), value: u8::from(pressure) as f32 / MIDI_MAX_VAL })
        }
        MidiMessage::PolyphonicKeyPressure(ch, note, pressure) => {
            Some(MidiEvent::PolyAftertouch {
                channel: ch.index(),
                note: u8::from(note),
                pressure: u8::from(pressure) as f32 / MIDI_MAX_VAL,
            })
        }
        MidiMessage::SysEx(payload) => {
            if payload.len() == 5
                && u8::from(payload[0]) == 0x35
                && u8::from(payload[1]) == 0x59
                && u8::from(payload[2]) == 0x10
            {
                let pressed = u8::from(payload[4]) == 0x7F;
                Some(MidiEvent::Navigate { pressed })
            } else {
                None
            }
        }
        _ => None,
    }?;

    Some((event, consumed))
}
```

- [ ] **Step 2: Rewrite `parse_and_push` to use `parse_midi_message`**

```rust
pub fn parse_and_push(
    data: &[u8],
    tx: &SharedMidiTx,
    note_state: &NoteState,
    pad_state: &PadState,
) -> usize {
    let Some((event, consumed)) = parse_midi_message(data) else { return 0 };

    // Update GUI note display
    match &event {
        MidiEvent::NoteOn { channel, note, velocity } => {
            let state = if *channel == 9 { pad_state } else { note_state };
            if *velocity == 0 {
                state[*note as usize].store(0, Ordering::Relaxed);
            } else {
                state[*note as usize].store(*velocity, Ordering::Relaxed);
            }
        }
        MidiEvent::NoteOff { channel, note } => {
            let state = if *channel == 9 { pad_state } else { note_state };
            state[*note as usize].store(0, Ordering::Relaxed);
        }
        _ => {}
    }

    if let Ok(mut tx) = tx.try_lock() {
        let _ = tx.push(event);
    }

    consumed
}
```

- [ ] **Step 3: Create `src/amidi.rs`**

```rust
//! Direct AMidi NDK access for zero-latency USB MIDI in audio callback.
//! AMidiOutputPort_receive() is non-blocking and RT-safe per Google docs.

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicPtr, Ordering};
#[cfg(target_os = "android")]
use std::ptr;

#[cfg(target_os = "android")]
#[allow(non_camel_case_types)]
type media_status_t = i32;

#[cfg(target_os = "android")]
#[repr(C)]
struct AMidiDevice { _opaque: [u8; 0] }
#[cfg(target_os = "android")]
#[repr(C)]
pub struct AMidiOutputPort { _opaque: [u8; 0] }

#[cfg(target_os = "android")]
const AMIDI_OPCODE_DATA: i32 = 1;

#[cfg(target_os = "android")]
#[link(name = "amidi")]
extern "C" {
    fn AMidiDevice_fromJava(
        env: *mut jni::sys::JNIEnv,
        midi_device_obj: jni::sys::jobject,
        out_device: *mut *mut AMidiDevice,
    ) -> media_status_t;
    fn AMidiDevice_release(device: *mut AMidiDevice) -> media_status_t;
    fn AMidiOutputPort_open(
        device: *const AMidiDevice,
        port_number: i32,
        out_port: *mut *mut AMidiOutputPort,
    ) -> media_status_t;
    fn AMidiOutputPort_close(port: *mut AMidiOutputPort);
    fn AMidiOutputPort_receive(
        port: *mut AMidiOutputPort,
        opcode: *mut i32,
        buffer: *mut u8,
        max_bytes: usize,
        num_bytes: *mut usize,
        timestamp: *mut i64,
    ) -> isize;
}

/// Thread-safe wrapper for AMidiOutputPort pointer.
/// Set from JNI thread (Release), read from audio callback (Acquire).
#[cfg(target_os = "android")]
pub struct AmidiPort {
    port: AtomicPtr<AMidiOutputPort>,
    device: AtomicPtr<AMidiDevice>,
}

#[cfg(target_os = "android")]
// Safety: AMidiOutputPort_receive is documented as safe to call from any thread.
// Port/device pointers are only dereferenced through AMidi functions which are thread-safe.
unsafe impl Send for AmidiPort {}
#[cfg(target_os = "android")]
unsafe impl Sync for AmidiPort {}

#[cfg(target_os = "android")]
impl AmidiPort {
    pub const fn new() -> Self {
        Self {
            port: AtomicPtr::new(ptr::null_mut()),
            device: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Open AMidi port from a Java MidiDevice object.
    /// Called from JNI thread (not audio thread).
    pub fn open_from_java(
        &self,
        env: *mut jni::sys::JNIEnv,
        midi_device: jni::sys::jobject,
        port_number: i32,
    ) -> bool {
        // Atomically remove old port first (audio thread will see null and skip)
        let old_port = self.port.swap(ptr::null_mut(), Ordering::AcqRel);
        let old_device = self.device.swap(ptr::null_mut(), Ordering::AcqRel);

        // Close old resources after nulling pointers (audio thread no longer uses them)
        if !old_port.is_null() { unsafe { AMidiOutputPort_close(old_port); } }
        if !old_device.is_null() { unsafe { AMidiDevice_release(old_device); } }

        let mut device: *mut AMidiDevice = ptr::null_mut();
        let status = unsafe { AMidiDevice_fromJava(env, midi_device, &mut device) };
        if status != 0 || device.is_null() {
            log::warn!("[amidi] AMidiDevice_fromJava failed: {status}");
            return false;
        }

        let mut port: *mut AMidiOutputPort = ptr::null_mut();
        let status = unsafe { AMidiOutputPort_open(device, port_number, &mut port) };
        if status != 0 || port.is_null() {
            log::warn!("[amidi] AMidiOutputPort_open failed: {status}");
            unsafe { AMidiDevice_release(device); }
            return false;
        }

        // Store new pointers — audio thread will pick them up via Acquire load
        self.device.store(device, Ordering::Release);
        self.port.store(port, Ordering::Release);
        log::info!("[amidi] USB MIDI port {port_number} opened");
        true
    }

    /// Poll for MIDI data. Non-blocking, RT-safe.
    /// Call from audio callback. Returns num bytes or None.
    #[inline]
    pub fn receive(&self, buf: &mut [u8]) -> Option<usize> {
        let port = self.port.load(Ordering::Acquire);
        if port.is_null() { return None; }

        let mut opcode: i32 = 0;
        let mut nbytes: usize = 0;
        let mut timestamp: i64 = 0;

        let rc = unsafe {
            AMidiOutputPort_receive(
                port, &mut opcode, buf.as_mut_ptr(),
                buf.len(), &mut nbytes, &mut timestamp,
            )
        };

        if rc > 0 && opcode == AMIDI_OPCODE_DATA && nbytes > 0 {
            Some(nbytes)
        } else {
            None
        }
    }

    pub fn close(&self) {
        let port = self.port.swap(ptr::null_mut(), Ordering::AcqRel);
        if !port.is_null() { unsafe { AMidiOutputPort_close(port); } }
        let device = self.device.swap(ptr::null_mut(), Ordering::AcqRel);
        if !device.is_null() { unsafe { AMidiDevice_release(device); } }
    }
}

#[cfg(target_os = "android")]
impl Drop for AmidiPort {
    fn drop(&mut self) { self.close(); }
}

/// Stub for non-Android platforms — all methods are no-ops.
#[cfg(not(target_os = "android"))]
pub struct AmidiPort;

#[cfg(not(target_os = "android"))]
impl AmidiPort {
    pub const fn new() -> Self { Self }
    #[inline]
    pub fn receive(&self, _buf: &mut [u8]) -> Option<usize> { None }
    pub fn close(&self) {}
}
```

Key fixes vs original:
- `load(Acquire)` not `Relaxed` — correct on ARM
- `open_from_java`: null old pointers FIRST, then close (no race with audio thread)
- Non-Android stub — no `#[cfg]` needed in audio.rs

- [ ] **Step 4: Add `mod amidi` to `src/main.rs`**

After `mod midi;`:
```rust
mod amidi;
```

No `#[cfg]` needed — `amidi.rs` has cfg inside, and stub for non-Android.

- [ ] **Step 5: Build and test**

```bash
cargo build --release && cargo test
bash android/build_rust.sh
```

- [ ] **Step 6: Commit**

```bash
git add src/amidi.rs src/midi.rs src/main.rs
git commit -m "feat: AMidi FFI wrapper + shared parse_midi_message"
```

---

### Task 2: Poll AMidi in audio callback

**Files:**
- Modify: `src/audio.rs`
- Modify: `src/main.rs`
- Modify: `src/gui/settings.rs`

- [ ] **Step 1: Add AmidiPort + NoteState to AudioBackend::new**

```rust
pub fn new(
    config: AudioConfig,
    mut synth: SynthEngine,
    mut midi_rx: Consumer<MidiEvent>,
    mut ctrl_rx: Consumer<ControlEvent>,
    amidi_port: std::sync::Arc<crate::amidi::AmidiPort>,
    note_state: crate::midi::NoteState,
    pad_state: crate::midi::PadState,
) -> Result<(Self, u32)> {
```

- [ ] **Step 2: Poll AMidi INSIDE per-block loop**

Inside the callback, right next to `while let Ok(event) = midi_rx.pop()`, add AMidi polling:

```rust
while frame_offset < total_frames {
    let block_len = (total_frames - frame_offset).min(crate::synth::BLOCK_SIZE);

    // USB MIDI via AMidi (non-blocking, RT-safe, zero-latency)
    {
        let mut amidi_buf = [0u8; 256];
        while let Some(nbytes) = amidi_port.receive(&mut amidi_buf) {
            let mut off = 0;
            while off < nbytes {
                if let Some((event, consumed)) = crate::midi::parse_midi_message(&amidi_buf[off..nbytes]) {
                    // Update GUI note display
                    match &event {
                        crate::synth::MidiEvent::NoteOn { channel, note, velocity } => {
                            let st = if *channel == 9 { &pad_state } else { &note_state };
                            if *velocity == 0 { st[*note as usize].store(0, Ordering::Relaxed); }
                            else { st[*note as usize].store(*velocity, Ordering::Relaxed); }
                        }
                        crate::synth::MidiEvent::NoteOff { channel, note } => {
                            let st = if *channel == 9 { &pad_state } else { &note_state };
                            st[*note as usize].store(0, Ordering::Relaxed);
                        }
                        _ => {}
                    }
                    synth.handle_event(event);
                    off += consumed;
                } else {
                    break;
                }
            }
        }
    }

    // BLE MIDI + desktop MIDI via ring buffer
    while let Ok(event) = midi_rx.pop() {
        synth.handle_event(event);
    }

    // ... tick_block, soft_limit, etc ...
```

Note: `AmidiPort::receive` on non-Android is a no-op stub returning `None`, so the `while let` loop body never executes. No `#[cfg]` needed.

- [ ] **Step 3: Update `init_common()` in `src/main.rs`**

Add OnceLock:
```rust
static AMIDI_PORT: std::sync::OnceLock<std::sync::Arc<crate::amidi::AmidiPort>> = std::sync::OnceLock::new();
```

In `init_common()`, before AudioBackend::new:
```rust
let amidi_port = {
    let port = std::sync::Arc::new(crate::amidi::AmidiPort::new());
    let _ = AMIDI_PORT.set(port.clone());
    port
};
```

Update AudioBackend::new call:
```rust
let (audio_backend, actual_sr) = audio::AudioBackend::new(
    audio_config, engine, midi_rx, ctrl_rx,
    amidi_port,
    note_state.clone(),
    pad_state.clone(),
)?;
```

- [ ] **Step 4: Update `reconnect_audio()` in `src/gui/settings.rs`**

```rust
let amidi_port = crate::AMIDI_PORT.get().cloned()
    .unwrap_or_else(|| std::sync::Arc::new(crate::amidi::AmidiPort::new()));

let (audio_backend, actual_sr) = crate::audio::AudioBackend::new(
    audio_config, engine, midi_rx, ctrl_rx,
    amidi_port,
    self.note_state.clone(),
    self.pad_state.clone(),
)?;
```

- [ ] **Step 5: Build and test**

```bash
cargo build --release && cargo test
bash android/build_rust.sh
```

- [ ] **Step 6: Commit**

```bash
git add src/audio.rs src/main.rs src/gui/settings.rs
git commit -m "feat: poll AMidi in audio callback for zero-latency USB MIDI"
```

---

### Task 3: JNI function + Java side

**Files:**
- Modify: `src/main.rs`
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`

- [ ] **Step 1: Add JNI function in `src/main.rs`**

```rust
/// JNI: Java passes opened USB MidiDevice for direct AMidi access in audio callback.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_SynthActivity_openUsbMidiNative<'local>(
    env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    midi_device: jni::objects::JObject<'local>,
    port_number: jni::sys::jint,
) {
    let Some(amidi_port) = AMIDI_PORT.get() else {
        log::warn!("[amidi] AMIDI_PORT not initialized yet");
        return;
    };

    if amidi_port.open_from_java(env.get_raw(), midi_device.as_raw(), port_number) {
        log::info!("[amidi] USB MIDI port {port_number} opened for zero-latency");
    } else {
        log::warn!("[amidi] Failed to open USB MIDI port {port_number}");
    }
}
```

- [ ] **Step 2: Update `SynthActivity.java` — open ALL USB output ports via AMidi**

Add native method declaration:
```java
private static native void openUsbMidiNative(MidiDevice device, int portNumber);
```

Replace `openUsbMidiDevices()`:
```java
private void openUsbMidiDevices() {
    if (mMidiManager == null) {
        mMidiManager = (MidiManager) getSystemService(Context.MIDI_SERVICE);
    }
    if (mMidiManager == null) return;

    MidiDeviceInfo[] infos = mMidiManager.getDevices();
    for (MidiDeviceInfo info : infos) {
        if (info.getType() != MidiDeviceInfo.TYPE_USB) continue;

        String name = info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME);
        if (name == null) name = "USB MIDI";
        Log.i(TAG, "Found USB MIDI: " + name);

        String finalName = name;
        mMidiManager.openDevice(info, new MidiManager.OnDeviceOpenedListener() {
            @Override
            public void onDeviceOpened(MidiDevice midiDevice) {
                if (midiDevice == null) {
                    Log.w(TAG, "USB MIDI open failed: " + finalName);
                    return;
                }
                mOpenDevices.add(midiDevice);
                Log.i(TAG, "USB MIDI opened: " + finalName);

                // Open ALL output ports via AMidi for zero-latency
                MidiDeviceInfo.PortInfo[] ports = midiDevice.getInfo().getPorts();
                for (MidiDeviceInfo.PortInfo pi : ports) {
                    if (pi.getType() == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                        Log.i(TAG, "AMidi: opening port " + pi.getPortNumber());
                        openUsbMidiNative(midiDevice, pi.getPortNumber());
                    }
                }
            }
        }, new Handler(Looper.getMainLooper()));
    }
}
```

- [ ] **Step 3: Build APK**

```bash
cd android && ./gradlew assembleRelease
```

- [ ] **Step 4: Commit**

```bash
git add src/main.rs android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "feat: JNI + Java for USB MIDI via AMidi zero-latency"
```

---

### Task 4: Deploy and test

- [ ] **Step 1: Build .so + APK, deploy**

```bash
bash android/build_rust.sh
cd android && ./gradlew assembleRelease
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
```

- [ ] **Step 2: Verify USB MIDI via AMidi in logs**

```bash
unset LD_PRELOAD && adb logcat -c && adb shell am force-stop com.minimidisynth && sleep 1 \
&& adb shell am start -n com.minimidisynth/com.minimidisynth.SynthActivity \
&& sleep 8 && adb logcat -d -s "MiniMidiSynth" | grep -i "amidi\|USB MIDI"
```

Expected:
```
Found USB MIDI: SMK-37 Pro Midi
USB MIDI opened: SMK-37 Pro Midi
AMidi: opening port 0
[amidi] USB MIDI port 0 opened
AMidi: opening port 1
[amidi] USB MIDI port 1 opened
```

- [ ] **Step 3: Press keys, verify zero-latency sound**

- [ ] **Step 4: Verify desktop still works**

```bash
cargo build --release && cargo test
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: zero-latency USB MIDI via AMidi in audio callback"
```

---

## Summary

| Path | Latency | Mechanism |
|------|---------|-----------|
| USB MIDI (Android) | ~0ms | AMidi → audio callback → synth directly |
| BLE MIDI (Android) | ~5-15ms | Java MidiReceiver → JNI → ring buffer → audio callback |
| Desktop (midir) | ~1-3ms | midir callback → ring buffer → audio callback |
| Audio callback overhead (no USB) | 0 | `AmidiPort::receive` stub returns `None` immediately |
