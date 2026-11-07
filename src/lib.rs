#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod audio;
#[cfg(target_os = "android")]
mod android_jni;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod cc_map;
pub mod chord;
pub mod config;
#[cfg(feature = "gui")]
mod gui;
#[cfg(target_os = "linux")]
mod input;
mod headless;
mod init;
mod key_action;
pub mod metrics;
pub mod midi;
mod amidi;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod preset;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
pub mod synth;
pub mod trash;

/// Diagnostic allocator: with `--features rt-assert`, any allocation or
/// deallocation inside a region marked with `assert_no_alloc` aborts the
/// process instead of silently costing an unbounded amount of time. Not
/// enabled by default — it puts a thread-local check on every allocation in
/// the program.
#[cfg(feature = "rt-assert")]
#[global_allocator]
static RT_ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[allow(unused_imports)]
use std::sync::atomic::{AtomicU8, Ordering};
#[cfg(any(feature = "gui", target_os = "android"))]
use std::sync::Arc;
#[cfg(feature = "gui")]
use std::sync::Mutex;

pub use init::{AMIDI_PORT, EngineHandles};

#[cfg(target_os = "android")]
pub use android_jni::{AUDIO_PAUSED, JNI_MIDI_TX, JNI_NOTE_STATE, JNI_PAD_STATE};

use anyhow::Result;
#[cfg(feature = "gui")]
use midir::MidiInputConnection;

/// Desktop entry point. The `mini_midi_synth` binary calls this; on Android
/// the entry point is `android_main` instead (see `android_jni.rs`).
#[cfg(not(target_os = "android"))]
pub fn desktop_main() -> Result<()> {
    let _ = env_logger::try_init(); // RUST_LOG-gated; Android uses android_logger

    #[cfg(target_os = "linux")]
    init::suppress_alsa_errors();

    #[cfg(feature = "gui")]
    {
        let headless = std::env::args().any(|a| a == "--headless" || a == "-H");
        if headless {
            headless::run_headless()
        } else {
            run_gui()
        }
    }

    #[cfg(not(feature = "gui"))]
    headless::run_headless()
}

// ---------------------------------------------------------------------------
// GUI mode
// ---------------------------------------------------------------------------
#[cfg(feature = "gui")]
#[cfg(not(target_os = "android"))]
fn make_piano_icon() -> eframe::egui::IconData {
    const S: u32 = 64;
    let mut rgba = vec![0u8; (S * S * 4) as usize];
    let white_w = S / 4;
    let black_w = white_w / 2;
    let black_h = S * 55 / 100;

    for y in 0..S {
        for x in 0..S {
            let px = ((y * S + x) * 4) as usize;
            let ki = (x / white_w).min(3);
            let gap = x == ki * white_w + white_w - 1 && ki < 3;
            if gap {
                rgba[px] = 190; rgba[px+1] = 190; rgba[px+2] = 195;
            } else {
                rgba[px] = 245; rgba[px+1] = 245; rgba[px+2] = 250;
            }
            rgba[px+3] = 255;
            for &bi in &[1u32, 2] {
                let bx = bi * white_w - black_w / 2;
                if x >= bx && x < bx + black_w && y < black_h {
                    rgba[px] = 40; rgba[px+1] = 40; rgba[px+2] = 48;
                }
            }
        }
    }
    eframe::egui::IconData { rgba, width: S, height: S }
}

#[cfg(feature = "gui")]
pub fn run_gui(
    #[cfg(target_os = "android")]
    android_app: winit::platform::android::activity::AndroidApp,
) -> Result<()> {
    let mut c = init::init_common()?;

    // #18-резидуал: watch default-sink в фоновом потоке (fork в GUI-потоке
    // дорог на большом RSS; pactl сам по себе 10-50 мс). Смена sink =
    // audio_disconnected → штатный reconnect-цикл.
    #[cfg(all(not(target_os = "android"), target_os = "linux"))]
    {
        let disconnected = c.audio_disconnected.clone();
        std::thread::Builder::new()
            .name("audio-sink-watch".into())
            .spawn(move || {
                let mut last: Option<String> = None;
                loop {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let out = std::process::Command::new("pactl")
                        .args(["get-default-sink"])
                        .output();
                    if let Ok(out) = out {
                        if out.status.success() {
                            let cur = String::from_utf8_lossy(&out.stdout).trim().to_string();
                            if let Some(prev) = &last {
                                if *prev != cur {
                                    log::info!("[audio] default sink changed: {prev:?} -> {cur:?}");
                                    disconnected.store(true, Ordering::Relaxed);
                                }
                            }
                            last = Some(cur);
                        }
                    }
                }
            })
            .expect("spawn audio-sink-watch");
    }

    let midi_handle: Arc<Mutex<Option<MidiInputConnection<()>>>> =
        Arc::new(Mutex::new(c.midi_conn.take()));

    let midi_handle_cb = midi_handle.clone();
    let midi_tx_cb = c.midi_tx_shared.clone();
    let note_state_cb = c.note_state.clone();
    let pad_state_cb = c.pad_state.clone();
    let on_midi_reconnect = Box::new(move |port_name: &str| -> Result<(), String> {
        let new_conn = midi::connect_by_name(Some(port_name), 0, midi_tx_cb.clone(), note_state_cb.clone(), pad_state_cb.clone())
            .map_err(|e| e.to_string())?;
        let mut handle = midi_handle_cb.lock().map_err(|e| e.to_string())?;
        *handle = Some(new_conn);
        Ok(())
    });

    let make_part = |pidx: usize, en: bool, vol: f32, lo: u8, hi: u8,
                     sf2: bool, prog: u8| {
        let mut s = gui::PartState::new_empty();
        s.patch_idx = pidx; s.enabled = en; s.volume = vol;
        s.min_note = lo; s.max_note = hi; s.sf2_mode = sf2; s.sf2_program = prog;
        s
    };
    let layer_a = make_part(c.patch_idx, true,  c.config.ui.layer_a_volume, 0,   127, c.config.sf2.layer_a_sf2, c.config.sf2.layer_a_program);
    let layer_b = make_part(0,          false, c.config.ui.layer_b_volume, 60,  127, c.config.sf2.layer_b_sf2, c.config.sf2.layer_b_program);
    let empty_part = |_i: usize| make_part(0, false, 0.8, 0, 127, false, 0);
    let parts_extra: Vec<gui::PartState> = (2..crate::synth::MAX_PARTS).map(empty_part).collect();

    // Window size читается до передачи c в App::new (c перемещается).
    #[cfg(not(target_os = "android"))]
    let (window_w, window_h) = (c.config.ui.window_width, c.config.ui.window_height);

    let mut app = gui::App::new(c, on_midi_reconnect, layer_a, layer_b, parts_extra);

    app.load_edited_params(0);
    app.load_edited_params(1);
    app.send_initial_presets();

    #[cfg(not(target_os = "android"))]
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth")
        .with_title("Mini MIDI Synth") // TECHDEBT #27: egui-winit defaults to "egui window"
        .with_inner_size([window_w, window_h])
        .with_min_inner_size([500.0, 300.0])
        .with_icon(make_piano_icon());

    #[cfg(target_os = "android")]
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth");

    let options = eframe::NativeOptions {
        viewport,
        #[cfg(not(target_os = "android"))]
        persist_window: true,
        #[cfg(target_os = "android")]
        android_app: Some(android_app),
        ..Default::default()
    };

    eframe::run_native(
        "mini_midi_synth",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.options_mut(|opts| {
                opts.reduce_texture_memory = true;
                opts.max_passes = std::num::NonZeroUsize::new(1).unwrap();
            });
            let _midi = midi_handle;
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}
