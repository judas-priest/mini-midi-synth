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
    use crate::cc_map::CcMap;
    use crate::synth::drum::NUM_DRUM_SLOTS;

    let c = init::init_common()?;

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
        Arc::new(Mutex::new(c.midi_conn));

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

    let mut app = gui::App {
        patches: c.patches,
        note_state: c.note_state,
        pad_state: c.pad_state,
        ctrl_tx: c.ctrl_tx,
        sample_rate: c.actual_sr,
        config: c.config.clone(),
        #[cfg(not(target_os = "android"))]
        current_host: c.host_name.to_string(),
        #[cfg(target_os = "android")]
        current_host: "AAudio".to_string(),
        #[cfg(not(target_os = "android"))]
        available_hosts: c.available_hosts.clone(),
        #[cfg(not(target_os = "android"))]
        supported_sample_rates: c.supported_sr,
        midi_port_names: c.midi_port_names,
        midi_connected_port: c.midi_connected_name,
        show_settings: false,
        #[cfg(not(target_os = "android"))]
        selected_host_idx: c.host_idx,
        selected_sample_rate: c.config.audio.sample_rate,
        selected_buffer_size: c.config.audio.buffer_size,
        selected_midi_port: c.config.midi.port_name.clone(),
        is_jack: c.is_jack,
        parts: std::iter::once(layer_a).chain(std::iter::once(layer_b)).chain(parts_extra).collect(),
        active_part: 0,
        on_midi_reconnect: Some(on_midi_reconnect),
        collapsed_categories: c.config.ui.collapsed_categories.iter().cloned().collect(),
        feedback_rx: Some(c.feedback_rx),
        cc_map: CcMap::from_config(&c.config),
        midi_learn_target: None,
        pending_remove_part: None,
        _program_change_atom: c.program_change_atom,
        sustain_atom: c.handles.sustain_atom,
        master_rec_state: c.handles.master_rec_state_atom,
        master_rec_cycle: c.handles.master_rec_cycle_atom,
        sustain_release: c.config.ui.sustain_release,
        sf2_release_scale: c.config.ui.sf2_release_scale,
        show_help: false,
        global_params: {
            let mut gp = std::collections::BTreeMap::new();
            gp.insert("master_volume".to_string(), c.config.ui.master_volume);
            gp.insert("master_tone".to_string(), c.config.ui.master_tone);
            gp.insert("reverb_mix".to_string(), c.config.ui.fader_reverb);
            gp.insert("delay_mix".to_string(), c.config.ui.fader_delay);
            gp.insert("pitch_bend_range".to_string(), c.config.ui.pitch_bend_range as f32);
            gp
        },
        pickup_indicators: std::collections::BTreeMap::new(),
        show_drums: false,
        show_looper: false,
        drum_patterns: [synth::drum::DrumPattern::default(); 8],
        drum_params: [synth::drum::DrumSlotParams::default(); NUM_DRUM_SLOTS],
        drum_slot_mutes_view: [false; NUM_DRUM_SLOTS],
        drum_remap: core::array::from_fn(|i| 36 + i as u8),
        drum_pad_vel: [0u8; 16],
        drum_kit_names: {
            let mut n: [String; 26] = Default::default();
            for (i, entry) in n.iter_mut().enumerate() {
                let note = 30 + i as u8;
                *entry = if (36..=51).contains(&note) {
                    crate::synth::drum::DRUM_NAMES[(note - 36) as usize].to_string()
                } else { String::new() };
            }
            n
        },
        show_drum_remap: false,
        remap_pad: None,
        drum_volume: c.config.ui.drum_volume,
        drum_fixed_velocity: c.config.ui.drum_fixed_velocity,
        drum_bpm: 120.0,
        drum_swing: 0.0,
        drum_playing: false,
        drum_recording: false,
        drum_current_pattern: 0,
        drum_step_atom: c.handles.drum_step_atom,
        drum_play_atom: c.handles.drum_play_atom,
        drum_rec_atom: c.handles.drum_rec_atom,
        drum_pattern_atom: c.handles.drum_pattern_atom,
        drum_chain_on: false,
        drum_chain_mask: 0,
        drum_metronome: c.config.ui.drum_metronome,
        drum_prev_step: 0,
        drum_beat_flash: None,
        perf_window: 1,
        tracks_window: 0,
        tracks_sel: None,
        tracks_rename_focus: false,
        pat_window: 0,
        mute_window: 0,
        drum_undo_atom: c.handles.drum_undo_atom,
        drum_redo_atom: c.handles.drum_redo_atom,
        console: gui::led_console::LedConsole::default(),
        console_rx: Some(c.console_rx),
        flash: gui::note_flash::NoteFlashState::new(),
        flash_rx: c.flash_rx,
        led_snapshot: [[0; 3]; 16],
        led_dirty: true,
        led_socket: None,
        led_socket_alive: None,
        led_socket_failed_at: None,
        led_connecting: None,
        led_last_send: None,
        led_child: None,
        led_spawn_backoff: None,
        console_prev_mode: None,
        part_drag_src: None,
        part_drag_dst: None,
        tracks_del_armed: None,
        drum_kit_name: String::new(),
        drum_kit_list: preset::list_drum_kits(),
        drum_midi_import_path: String::new(),
        looper_atoms: c.handles.looper_atoms,
        looper_display: c.handles.looper_display,
        looper_bars: 4,
        looper_quantize: 0,
        looper_clear_confirm: None,
        drum_clear_confirm: None,
        fx_reset_confirm: None,
        pad_remap_reset_confirm: None,
        looper_layer_mute: [false; 256],
        looper_solo_layer: None,
        looper_source_mix: true,
        looper_part_mutes: [false; 16],
        seq_target_atom: c.handles.seq_target_atom,
        active_part_atom: c.handles.active_part_atom,
        keybinds: gui::keybinds::Keybinds::from_config(&c.config.ui.keybinds),
        show_keybinds_window: false,
        keybind_capturing: None,
        keybinds_reset_confirm: None,
        unbind_confirm: None,
        perf_name: String::new(),
        perf_list: {
            let mut perf_list = preset::list_performances();
            perf_list.insert(0, ("(Init)".into(), std::path::PathBuf::new()));
            perf_list
        },
        // #57: пересчитывается первым кадром (guard в update())
        console_list: Vec::new(),
        nav_press_time: None,
        show_pad_perf: false,
        show_tracks_manager: false,
        pad_perf_map: {
            let saved = &c.config.ui.pad_perf_map;
            std::array::from_fn(|i| saved.get(i).cloned().flatten())
        },
        pad_prev_state: [0u8; 16],
        last_config_save: std::time::Instant::now(),
        dsp_load: 0.0,
        dsp_peak: 0.0,
        global_dirty: false,
        active_knob: None,
        sf2_file_list: {
            let scan = gui::scan_sf2_files();
            for (dir, err) in &scan.unreadable {
                log::warn!("[sf2] cannot read {}: {err}", dir.display());
                eprintln!("[sf2] cannot read {}: {err}", dir.display());
            }
            scan.files
        },
        sf2_keys_selected: None,
        sf2_keys_loaded_name: String::new(),
        sf2_drums_selected: None,
        sf2_drums_loaded_name: String::new(),
        sf2_drums_enabled: c.config.sf2.drums_sf2,
        sf2_keys_soundfont: None,
        sf2_drums_soundfont: None,
        sf2_block_size: c.config.sf2.block_size,
        sf2_sample_offset_ms: c.config.sf2.sample_offset_ms,
        pitch_seq_step_atoms: c.handles.pitch_seq_step_atoms,
        pitch_seq_enabled: [false; 2],
        pitch_seq_steps: [[synth::step_seq::PitchStep::default(); 16]; 2],
        pitch_seq_length: [16; 2],
        pitch_seq_rate: [2; 2],
        pitch_seq_scale: [0; 2],
        pitch_seq_swing: [0.0; 2],
        preset_search: String::new(),
        inst_browser_part: None,
        inst_browser_index: 0,
        inst_browser_seeded: false,
        inst_browser_last_search: String::new(),
        inst_browser_scroll: false,
        scope_buf: c.handles.scope_buf,
        last_scope_write: 0,
        split_point: c.config.ui.split_point.unwrap_or(60),
        show_fx_chain: false,
        show_midi_seq: false,
        midi_seq_path: String::new(),
        midi_seq_tracks: Vec::new(),
        midi_seq_playing: false,
        midi_seq_looping: true,
        midi_seq_bpm: None,
        midi_seq_play_atom: c.handles.midi_seq_play_atom,
        midi_seq_pos_atom: c.handles.midi_seq_pos_atom,
        midi_seq_file_pick: None,
        undo_stack: std::collections::VecDeque::new(),
        redo_stack: Vec::new(),
        toasts: Vec::new(),
        audio_disconnected: c.audio_disconnected.clone(),
        audio_device_name: c._audio_handle.device_name.clone(),
        audio_host_id: c._audio_handle.host_id,
        audio_watch: crate::audio::ReconnectPolicy::default(),
        _audio_handle: Some(c._audio_handle),
        midi_tx_shared: c.midi_tx_shared.clone(),
        engine_trash_rx: c.engine_trash_rx,
        cb_trash_rx: c.cb_trash_rx,
    };

    app.load_edited_params(0);
    app.load_edited_params(1);
    app.send_initial_presets();

    #[cfg(not(target_os = "android"))]
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth")
        .with_title("Mini MIDI Synth") // TECHDEBT #27: egui-winit defaults to "egui window"
        .with_inner_size([c.config.ui.window_width, c.config.ui.window_height])
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
