# BLE MIDI Java Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Receive BLE MIDI data on Android via Java MidiReceiver and forward it to the Rust synth engine through JNI, bypassing AMidi which doesn't deliver BLE MIDI data.

**Architecture:** Java `MidiDevice.openOutputPort()` → `MidiReceiver.onSend(byte[])` receives raw MIDI bytes → calls JNI native method → Rust parses bytes with `wmidi` and pushes `MidiEvent` into the existing `SharedMidiTx` ring buffer → audio thread consumes events. Desktop is unaffected — midir handles MIDI there. MIDI parsing is extracted into a shared function used by both midir callback and JNI bridge.

**Tech Stack:** Java (MidiReceiver, MidiManager), JNI (`jni` crate), Rust (wmidi, rtrb), existing SharedMidiTx

---

## Why

AMidi (`AMidiOutputPort_receive`) does not deliver BLE MIDI data on Android. The port connects successfully but the polling thread receives zero bytes. This is confirmed by testing — logcat shows zero `Received` callbacks despite active BLE connection. The fix: read MIDI via Java `MidiReceiver` API, forward to Rust via JNI.

## Review of design decisions

| Decision | Rationale |
|----------|-----------|
| **`OnceLock` for globals** (not `static mut`) | `static mut` is unsafe, UB-prone, deprecated. `OnceLock` is safe, stable since Rust 1.70 |
| **Shared `parse_midi_message()` function** | MIDI parsing logic is identical in midir callback and JNI bridge. Extract to avoid duplication |
| **`extern "system"` for JNI** | Standard for `jni` crate. On Android/Linux same as `extern "C"` |
| **Loop over bytes in JNI** | Android docs: MidiReceiver.onSend() "can contain multiple messages or partial messages". `wmidi::MidiMessage::try_from()` handles extra trailing bytes (ignores them), `bytes_size()` gives consumed length |
| **No `System.loadLibrary` in MidiBridge** | Native lib already loaded by NativeActivity. JNI symbol resolution works from same class loader |
| **Keep midir for USB MIDI** | AMidi works fine for USB MIDI. BLE-only bypass via Java bridge |

## File Map

### Files to create:

| File | Purpose |
|------|---------|
| `android/app/src/main/java/com/minimidisynth/MidiBridge.java` | Java MidiReceiver → JNI native call |

### Files to modify:

| File | Change |
|------|--------|
| `Cargo.toml` | Add `jni` crate as Android-only dependency |
| `src/midi.rs` | Extract `parse_midi_message()` shared function, add `push_raw_midi()` for JNI use |
| `src/main.rs` | Add `OnceLock` global for JNI access to SharedMidiTx, set in `init_common()`, add JNI function |
| `android/app/src/main/java/com/minimidisynth/SynthActivity.java` | Connect MidiBridge to BLE MIDI output ports |

---

### Task 1: Extract shared MIDI parsing and add JNI entry point

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/midi.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Add `jni` crate to Android dependencies**

In `Cargo.toml`, in the `[target.'cfg(target_os = "android")'.dependencies]` section, add:

```toml
jni = { version = "0.21", default-features = false }
```

- [ ] **Step 2: Extract `parse_midi_message()` in `src/midi.rs`**

Add this function before `connect_by_name()`:

```rust
/// Parse a single MIDI message from raw bytes and push it to the ring buffer.
/// Used by both the midir callback and the Android JNI bridge.
/// Returns the number of bytes consumed, or 0 if parsing failed.
pub fn parse_and_push(
    data: &[u8],
    tx: &SharedMidiTx,
    note_state: &NoteState,
    pad_state: &PadState,
) -> usize {
    let Ok(msg) = MidiMessage::try_from(data) else { return 0 };
    let size = msg.bytes_size();

    let event = match msg {
        MidiMessage::NoteOn(ch, note, velocity) => {
            let channel = ch.index();
            let n = u8::from(note);
            let v = u8::from(velocity);
            let state = if channel == 9 { pad_state } else { note_state };
            if v == 0 {
                state[n as usize].store(0, Ordering::Relaxed);
            } else {
                state[n as usize].store(v, Ordering::Relaxed);
            }
            Some(MidiEvent::NoteOn { channel, note: n, velocity: v })
        }
        MidiMessage::NoteOff(ch, note, _) => {
            let channel = ch.index();
            let n = u8::from(note);
            let state = if channel == 9 { pad_state } else { note_state };
            state[n as usize].store(0, Ordering::Relaxed);
            Some(MidiEvent::NoteOff { channel, note: n })
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
    };

    if let Some(ev) = event {
        if let Ok(mut tx) = tx.try_lock() {
            let _ = tx.push(ev);
        }
    }

    size
}
```

- [ ] **Step 3: Replace duplicate match in midir callback**

In the `connect_by_name()` function, replace the entire `move |_timestamp, data, _| { ... }` callback body with:

```rust
move |_timestamp, data, _| {
    midi::parse_and_push(data, &tx, &note_state, &pad_state);
},
```

Wait — the closure captures `tx`, `note_state`, `pad_state` by move. Since `parse_and_push` takes references, this still works:

```rust
move |_timestamp, data, _| {
    crate::midi::parse_and_push(data, &tx, &note_state, &pad_state);
},
```

But we're inside `midi.rs` already, so just:

```rust
move |_timestamp, data, _| {
    parse_and_push(data, &tx, &note_state, &pad_state);
},
```

- [ ] **Step 4: Add `OnceLock` global and JNI function in `src/main.rs`**

At the top of `src/main.rs`, after the imports, add:

```rust
#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
static JNI_MIDI_TX: OnceLock<midi::SharedMidiTx> = OnceLock::new();
#[cfg(target_os = "android")]
static JNI_NOTE_STATE: OnceLock<midi::NoteState> = OnceLock::new();
#[cfg(target_os = "android")]
static JNI_PAD_STATE: OnceLock<midi::PadState> = OnceLock::new();
```

In `init_common()`, after `let midi_tx_shared: midi::SharedMidiTx = Arc::new(Mutex::new(midi_tx));` (line ~139), add:

```rust
#[cfg(target_os = "android")]
{
    let _ = JNI_MIDI_TX.set(midi_tx_shared.clone());
    let _ = JNI_NOTE_STATE.set(note_state.clone());
    let _ = JNI_PAD_STATE.set(pad_state.clone());
    log::info!("[init] JNI MIDI bridge globals set");
}
```

At the bottom of `src/main.rs` (before or after `android_main`), add the JNI function:

```rust
/// JNI entry point: called from Java MidiBridge.onMidiData(byte[])
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_MidiBridge_onMidiData(
    mut env: jni::JNIEnv,
    _class: jni::objects::JClass,
    data: jni::sys::jbyteArray,
) {
    let Ok(bytes) = env.convert_byte_array(data) else { return };
    if bytes.is_empty() { return }

    let (Some(tx), Some(ns), Some(ps)) = (
        JNI_MIDI_TX.get(), JNI_NOTE_STATE.get(), JNI_PAD_STATE.get()
    ) else { return };

    let mut offset = 0;
    while offset < bytes.len() {
        let consumed = midi::parse_and_push(&bytes[offset..], tx, ns, ps);
        if consumed == 0 { break }
        offset += consumed;
    }
}
```

- [ ] **Step 5: Build and test desktop**

```bash
cargo build --release && cargo test
```
Expected: Compiles, 26/26 tests pass. JNI code is cfg'd away.

- [ ] **Step 6: Cross-compile for Android**

```bash
bash android/build_rust.sh
```
Expected: Compiles. `Java_com_minimidisynth_MidiBridge_onMidiData` exported in .so.

Verify:
```bash
nm -D target/aarch64-linux-android/release/libmini_midi_synth.so | grep MidiBridge
```
Expected: shows the JNI symbol.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml Cargo.lock src/midi.rs src/main.rs
git commit -m "$(cat <<'EOF'
feat: JNI MIDI bridge for BLE MIDI on Android

Extract shared parse_and_push() from MIDI parsing.
Add JNI native method for Java MidiBridge.onMidiData(byte[]).
Uses OnceLock for thread-safe global state access.
EOF
)"
```

---

### Task 2: Java MidiBridge + SynthActivity wiring

**Files:**
- Create: `android/app/src/main/java/com/minimidisynth/MidiBridge.java`
- Modify: `android/app/src/main/java/com/minimidisynth/SynthActivity.java`

- [ ] **Step 1: Create `MidiBridge.java`**

```java
package com.minimidisynth;

import android.media.midi.MidiOutputPort;
import android.media.midi.MidiReceiver;
import android.util.Log;

import java.io.IOException;

/**
 * Bridges Java MIDI data to native Rust code via JNI.
 * Attaches to a MidiOutputPort as a MidiReceiver, forwards
 * raw MIDI bytes to the native synth engine.
 */
public class MidiBridge extends MidiReceiver {
    private static final String TAG = "MiniMidiSynth";

    /** JNI native method — implemented in Rust (main.rs) */
    public static native void onMidiData(byte[] data);

    private final MidiOutputPort mPort;
    private final String mDeviceName;

    public MidiBridge(MidiOutputPort port, String deviceName) {
        mPort = port;
        mDeviceName = deviceName;
    }

    @Override
    public void onSend(byte[] data, int offset, int count, long timestamp)
            throws IOException {
        if (count <= 0) return;

        byte[] slice;
        if (offset == 0 && count == data.length) {
            slice = data;
        } else {
            slice = new byte[count];
            System.arraycopy(data, offset, slice, 0, count);
        }

        try {
            onMidiData(slice);
        } catch (Exception e) {
            Log.e(TAG, "JNI onMidiData error", e);
        }
    }

    public void close() {
        try {
            mPort.close();
        } catch (IOException e) {
            Log.w(TAG, "Error closing MIDI port for " + mDeviceName, e);
        }
    }
}
```

- [ ] **Step 2: Update `SynthActivity.java`**

Full replacement of the file:

```java
package com.minimidisynth;

import android.app.NativeActivity;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.midi.MidiDevice;
import android.media.midi.MidiDeviceInfo;
import android.media.midi.MidiManager;
import android.media.midi.MidiOutputPort;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

/**
 * Wrapper around NativeActivity:
 * 1) Pre-loads libc++_shared.so (C++ runtime for cpal/Oboe)
 * 2) Opens paired Bluetooth MIDI devices via MidiManager
 * 3) Connects MidiBridge (Java MidiReceiver → JNI → Rust) to each device
 */
public class SynthActivity extends NativeActivity {
    private static final String TAG = "MiniMidiSynth";

    static {
        System.loadLibrary("c++_shared");
    }

    private MidiManager mMidiManager;
    private final List<MidiDevice> mOpenDevices = new ArrayList<>();
    private final List<MidiBridge> mBridges = new ArrayList<>();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        openPairedBluetoothMidiDevices();
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onDestroy() {
        for (MidiBridge bridge : mBridges) {
            bridge.close();
        }
        mBridges.clear();
        for (MidiDevice dev : mOpenDevices) {
            try { dev.close(); } catch (Exception e) {
                Log.w(TAG, "Error closing MIDI device", e);
            }
        }
        mOpenDevices.clear();
        super.onDestroy();
    }

    private void openPairedBluetoothMidiDevices() {
        mMidiManager = (MidiManager) getSystemService(Context.MIDI_SERVICE);
        if (mMidiManager == null) {
            Log.w(TAG, "MidiManager not available");
            return;
        }

        BluetoothManager btManager = (BluetoothManager) getSystemService(Context.BLUETOOTH_SERVICE);
        if (btManager == null) {
            Log.w(TAG, "BluetoothManager not available");
            return;
        }

        BluetoothAdapter adapter = btManager.getAdapter();
        if (adapter == null || !adapter.isEnabled()) {
            Log.w(TAG, "Bluetooth not enabled");
            return;
        }

        if (checkSelfPermission(android.Manifest.permission.BLUETOOTH_CONNECT)
                != PackageManager.PERMISSION_GRANTED) {
            Log.i(TAG, "Requesting BLUETOOTH_CONNECT permission");
            requestPermissions(
                new String[]{android.Manifest.permission.BLUETOOTH_CONNECT}, 1);
            return;
        }

        openBondedDevices(adapter);
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == 1 && grantResults.length > 0
                && grantResults[0] == PackageManager.PERMISSION_GRANTED) {
            BluetoothManager btManager = (BluetoothManager) getSystemService(Context.BLUETOOTH_SERVICE);
            if (btManager != null && btManager.getAdapter() != null) {
                openBondedDevices(btManager.getAdapter());
            }
        }
    }

    private void openBondedDevices(BluetoothAdapter adapter) {
        Set<BluetoothDevice> bonded = adapter.getBondedDevices();
        if (bonded == null || bonded.isEmpty()) {
            Log.i(TAG, "No bonded Bluetooth devices");
            return;
        }

        Handler handler = new Handler(Looper.getMainLooper());
        for (BluetoothDevice device : bonded) {
            String name = device.getName();
            Log.i(TAG, "Trying BLE MIDI: " + name + " [" + device.getAddress() + "]");
            try {
                mMidiManager.openBluetoothDevice(device,
                    new MidiManager.OnDeviceOpenedListener() {
                        @Override
                        public void onDeviceOpened(MidiDevice midiDevice) {
                            if (midiDevice == null) {
                                Log.w(TAG, "BLE MIDI failed: " + name);
                                return;
                            }
                            mOpenDevices.add(midiDevice);
                            Log.i(TAG, "BLE MIDI opened: " + name);
                            connectBridge(midiDevice, name);
                        }
                    }, handler);
            } catch (Exception e) {
                Log.w(TAG, "openBluetoothDevice error for " + name, e);
            }
        }
    }

    private void connectBridge(MidiDevice device, String name) {
        MidiDeviceInfo info = device.getInfo();
        MidiDeviceInfo.PortInfo[] ports = info.getPorts();
        for (MidiDeviceInfo.PortInfo pi : ports) {
            // TYPE_OUTPUT = data FROM device TO app (e.g. key presses)
            if (pi.getType() == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                MidiOutputPort outPort = device.openOutputPort(pi.getPortNumber());
                if (outPort != null) {
                    MidiBridge bridge = new MidiBridge(outPort, name);
                    outPort.connect(bridge);
                    mBridges.add(bridge);
                    Log.i(TAG, "BLE MIDI bridge: " + name + " port " + pi.getPortNumber());
                }
            }
        }
    }
}
```

- [ ] **Step 3: Build APK**

```bash
cd android && ./gradlew assembleRelease
```
Expected: BUILD SUCCESSFUL.

- [ ] **Step 4: Commit**

```bash
git add android/app/src/main/java/com/minimidisynth/MidiBridge.java \
        android/app/src/main/java/com/minimidisynth/SynthActivity.java
git commit -m "$(cat <<'EOF'
feat: Java MidiBridge receives BLE MIDI via MidiReceiver → JNI

MidiReceiver.onSend() gets raw bytes from BLE keyboard,
calls native MidiBridge.onMidiData(byte[]) to push into synth.
SynthActivity connects bridge to all BLE output ports.
EOF
)"
```

---

### Task 3: Deploy, test, clean up debug logs

**Files:**
- Modify: `src/midi.rs` (remove debug logs)
- Modify: `src/audio.rs` (remove debug peak logging)
- Modify: `src/main.rs` (remove 2s BLE delay)

- [ ] **Step 1: Build .so + APK and deploy**

```bash
bash android/build_rust.sh
cd android && ./gradlew assembleRelease
unset LD_PRELOAD && adb install -r app/build/outputs/apk/release/app-release.apk
```

- [ ] **Step 2: Launch and verify logs**

```bash
unset LD_PRELOAD && adb logcat -c && \
adb shell am force-stop com.minimidisynth && sleep 1 && \
adb shell am start -n com.minimidisynth/com.minimidisynth.SynthActivity && \
sleep 8 && adb logcat -d -s "MiniMidiSynth"
```

Expected logs:
- `BLE MIDI opened: SMK-37 Pro`
- `BLE MIDI bridge: SMK-37 Pro port 0`
- `JNI MIDI bridge globals set`

- [ ] **Step 3: Press keys and verify sound**

Press notes on SMK-37 Pro. Check audio peak is non-zero:

```bash
adb logcat -d -s "MiniMidiSynth" | grep -E "peak=[^0]|NoteOn"
```

- [ ] **Step 4: Remove debug logs from `src/midi.rs`**

Remove these lines:
- `log::info!("[midi] Connecting to port: {port_display}");` (line ~84)
- `log::info!("[midi] Received {} bytes", data.len());` (line ~91)
- `log::info!("MIDI NoteOn ch={} note={} vel={}", ...);` (line ~95)

- [ ] **Step 5: Remove debug peak logging from `src/audio.rs`**

Remove the `static AUDIO_LOG_COUNT` declaration and the peak logging block (the `if count < 5 || count % 1000 == 0` block).

- [ ] **Step 6: Remove 2s BLE MIDI delay from `src/main.rs`**

Remove the `#[cfg(target_os = "android")]` block with `std::thread::sleep(Duration::from_secs(2))`.

- [ ] **Step 7: Build both platforms, test desktop**

```bash
cargo build --release && cargo test
bash android/build_rust.sh
```
Expected: Both build, 26/26 tests pass.

- [ ] **Step 8: Commit**

```bash
git add src/midi.rs src/audio.rs src/main.rs
git commit -m "$(cat <<'EOF'
chore: remove debug logging from MIDI and audio

BLE MIDI working via Java bridge. Clean up temporary diagnostics.
EOF
)"
```
