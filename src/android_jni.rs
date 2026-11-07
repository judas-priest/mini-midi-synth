//! Android entry points: `android_main` and the JNI bridge functions.
//!
//! Desktop builds never compile this module (`#[cfg(target_os = "android")]`
//! on the `mod` declaration in `lib.rs`).

use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "android")]
pub static AUDIO_PAUSED: OnceLock<Arc<AtomicBool>> = OnceLock::new();
#[cfg(target_os = "android")]
pub static JNI_MIDI_TX: OnceLock<crate::midi::SharedMidiTx> = OnceLock::new();
#[cfg(target_os = "android")]
pub static JNI_NOTE_STATE: OnceLock<crate::midi::NoteState> = OnceLock::new();
#[cfg(target_os = "android")]
pub static JNI_PAD_STATE: OnceLock<crate::midi::PadState> = OnceLock::new();

#[cfg(target_os = "android")]
#[cfg(feature = "gui")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("MiniMidiSynth"),
    );

    // Catch panics — without this, Rust panics silently kill the process
    // with no error in logcat (looks like instant close, no crash dialog)
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {info}");
        log::error!("{msg}");
        // Also write to stderr in case android_logger fails
        eprintln!("{msg}");
        // Give logger time to flush before abort kills the process
        std::thread::sleep(std::time::Duration::from_millis(100));
    }));

    log::info!("android_main started");

    // Set data dir from Android internal storage
    if let Some(path) = app.internal_data_path() {
        log::info!("data dir: {}", path.display());
        crate::config::set_android_data_dir(path.to_path_buf());
    }
    if let Err(e) = crate::run_gui(app) {
        log::error!("run_gui failed: {e}");
    }
}

/// JNI: Java passes opened USB MidiDevice + output port numbers for AMidi.
/// Opens all specified ports at once (no per-port race condition).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_SynthActivity_openUsbMidiNative<'local>(
    env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    midi_device: jni::objects::JObject<'local>,
    port_numbers: jni::objects::JIntArray<'local>,
) {
    let Some(amidi_port) = crate::init::AMIDI_PORT.get() else {
        log::warn!("[amidi] AMIDI_PORT not initialized yet");
        return;
    };

    let len = match env.get_array_length(&port_numbers) {
        Ok(l) => l as usize,
        Err(_) => { log::warn!("[amidi] Failed to get port_numbers length"); return; }
    };
    let mut port_nums = vec![0i32; len];
    if env.get_int_array_region(&port_numbers, 0, &mut port_nums).is_err() {
        log::warn!("[amidi] Failed to read port_numbers array");
        return;
    }

    let opened = amidi_port.open_device(env.get_raw(), midi_device.as_raw(), &port_nums);
    log::info!("[amidi] Opened {opened}/{} USB MIDI output ports", port_nums.len());
}

/// JNI: close AMidi ports when USB device disconnected
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_SynthActivity_closeUsbMidiNative<'local>(
    _env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
) {
    if let Some(amidi_port) = crate::init::AMIDI_PORT.get() {
        amidi_port.close();
        log::info!("[amidi] USB MIDI ports closed (device disconnected)");
    }
}

/// JNI entry point: called from Java MidiBridge.onMidiData(byte[])
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_MidiBridge_onMidiData<'local>(
    env: jni::JNIEnv<'local>,
    _class: jni::objects::JClass<'local>,
    data: jni::objects::JByteArray<'local>,
) {
    let Ok(bytes) = env.convert_byte_array(&data) else {
        log::warn!("[jni_midi] convert_byte_array failed");
        return;
    };
    if bytes.is_empty() { return }

    let (Some(tx), Some(ns), Some(ps)) = (
        JNI_MIDI_TX.get(), JNI_NOTE_STATE.get(), JNI_PAD_STATE.get()
    ) else {
        log::warn!("[jni_midi] OnceLock not set yet");
        return;
    };

    let mut offset = 0;
    while offset < bytes.len() {
        let consumed = crate::midi::parse_and_push(&bytes[offset..], tx, ns, ps);
        if consumed == 0 { break }
        offset += consumed;
    }
}

/// JNI: pause/resume audio when app is backgrounded/foregrounded.
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
