mod audio;
mod cc_map;
mod config;
mod gui;
mod midi;
mod preset;
mod synth;

use std::sync::atomic::AtomicU8;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use cpal::HostId;
use midir::MidiInputConnection;

use crate::cc_map::CcMap;
use crate::config::Config;
use crate::synth::drum::NUM_DRUM_SLOTS;

/// Suppress ALSA lib error messages (not our own stderr).
#[cfg(target_os = "linux")]
fn suppress_alsa_errors() {
    #[link(name = "asound")]
    extern "C" {
        fn snd_lib_error_set_handler(handler: *const std::ffi::c_void) -> std::ffi::c_int;
    }
    unsafe {
        snd_lib_error_set_handler(std::ptr::null());
    }
}

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

fn find_host_idx(hosts: &[(HostId, &str)], name: &str) -> usize {
    hosts
        .iter()
        .position(|(_, n)| n.eq_ignore_ascii_case(name))
        .unwrap_or(0)
}

fn find_midi_port(port_names: &[String], saved_name: &str) -> Option<usize> {
    port_names.iter().position(|n| n == saved_name)
}

fn main() -> Result<()> {
    #[cfg(target_os = "linux")]
    suppress_alsa_errors();

    let config = Config::load();
    let presets = preset::load_all_presets();
    let available_hosts = audio::available_hosts();

    let host_idx = find_host_idx(&available_hosts, &config.audio.backend);
    let host_id = available_hosts[host_idx].0;
    let host_name = available_hosts[host_idx].1;
    let is_jack = host_name == "JACK";

    let supported_sr = audio::supported_sample_rates(host_id);

    let note_state = midi::new_note_state();
    let pad_state = midi::new_pad_state();

    let (midi_tx, midi_rx) = rtrb::RingBuffer::<synth::MidiEvent>::new(256);
    let (ctrl_tx, ctrl_rx) = rtrb::RingBuffer::<synth::ControlEvent>::new(256);
    let (feedback_tx, feedback_rx) = rtrb::RingBuffer::<synth::ParamFeedback>::new(64);

    let midi_tx_shared: midi::SharedMidiTx = Arc::new(Mutex::new(midi_tx));

    let program_change_atom = Arc::new(AtomicU8::new(255));

    let mut engine = synth::SynthEngine::new(config.audio.sample_rate as f32);
    engine.set_presets(presets.clone());
    engine.set_feedback_tx(feedback_tx);
    engine.set_program_change_atom(program_change_atom.clone());
    let drum_step_atom = engine.drum_engine.step_atom();
    let drum_play_atom = engine.drum_engine.play_atom();
    let drum_rec_atom = engine.drum_engine.rec_atom();
    let looper_atoms = engine.looper.atoms();
    let seq_target_atom = engine.seq_target_atom();
    let pitch_seq_step_atoms = engine.pitch_seq_step_atoms();

    let audio_config = audio::AudioConfig {
        host_id,
        sample_rate: config.audio.sample_rate,
        buffer_size: config.audio.buffer_size,
    };
    let (audio_backend, actual_sr) =
        audio::AudioBackend::new(audio_config, engine, midi_rx, ctrl_rx)?;

    // MIDI
    let midi_port_names = midi::list_ports().unwrap_or_default();
    let midi_port_idx = config
        .midi
        .port_name
        .as_deref()
        .and_then(|name| find_midi_port(&midi_port_names, name))
        .or_else(|| if midi_port_names.is_empty() { None } else { Some(0) });

    let midi_conn: Option<MidiInputConnection<()>> = midi_port_idx.and_then(|idx| {
        midi::connect(idx, midi_tx_shared.clone(), note_state.clone(), pad_state.clone()).ok()
    });

    let midi_connected_name = midi_port_idx
        .and_then(|idx| midi_port_names.get(idx))
        .filter(|_| midi_conn.is_some())
        .cloned();

    let preset_idx = config
        .ui
        .last_preset
        .as_deref()
        .and_then(|name| presets.iter().position(|p| p.name == name))
        .unwrap_or(0);

    let _audio_handle = audio_backend;

    let midi_handle: Arc<Mutex<Option<MidiInputConnection<()>>>> =
        Arc::new(Mutex::new(midi_conn));

    let midi_handle_cb = midi_handle.clone();
    let midi_tx_cb = midi_tx_shared.clone();
    let note_state_cb = note_state.clone();
    let pad_state_cb = pad_state.clone();
    let on_midi_reconnect = Box::new(move |port_idx: usize| -> Result<(), String> {
        let new_conn = midi::connect(port_idx, midi_tx_cb.clone(), note_state_cb.clone(), pad_state_cb.clone())
            .map_err(|e| e.to_string())?;
        let mut handle = midi_handle_cb.lock().map_err(|e| e.to_string())?;
        *handle = Some(new_conn);
        Ok(())
    });

    let layer_a = gui::LayerState {
        preset_idx,
        edited_params: std::collections::BTreeMap::new(),
        params_dirty: false,
        enabled: true,
        volume: config.ui.layer_a_volume,
        min_note: 0,
        max_note: 127,
        sf2_mode: config.sf2.layer_a_sf2,
        sf2_program: config.sf2.layer_a_program,
    };
    let layer_b = gui::LayerState {
        preset_idx: 0,
        edited_params: std::collections::BTreeMap::new(),
        params_dirty: false,
        enabled: false,
        volume: config.ui.layer_b_volume,
        min_note: 60,
        max_note: 127,
        sf2_mode: config.sf2.layer_b_sf2,
        sf2_program: config.sf2.layer_b_program,
    };

    let mut app = gui::App {
        _frame_count: 0,
        presets,
        note_state,
        pad_state,
        ctrl_tx,
        sample_rate: actual_sr,
        config: config.clone(),
        current_host: host_name.to_string(),
        available_hosts: available_hosts.clone(),
        supported_sample_rates: supported_sr,
        midi_port_names,
        midi_connected_port: midi_connected_name,
        show_settings: false,
        selected_host_idx: host_idx,
        selected_sample_rate: config.audio.sample_rate,
        selected_buffer_size: config.audio.buffer_size,
        selected_midi_port: config.midi.port_name.clone(),
        settings_status: String::new(),
        is_jack,
        layers: vec![layer_a, layer_b],
        active_layer: 0,
        on_midi_reconnect: Some(on_midi_reconnect),
        collapsed_categories: config.ui.collapsed_categories.iter().cloned().collect(),
        feedback_rx: Some(feedback_rx),
        cc_map: CcMap::from_config(&config),
        midi_learn_target: None,
        _program_change_atom: program_change_atom,
        show_help: false,
        global_params: {
            let mut gp = std::collections::BTreeMap::new();
            gp.insert("master_volume".to_string(), config.ui.master_volume);
            gp.insert("master_tone".to_string(), config.ui.master_tone);
            gp.insert("reverb_mix".to_string(), config.ui.fader_reverb);
            gp.insert("delay_mix".to_string(), config.ui.fader_delay);
            gp
        },
        pickup_indicators: std::collections::BTreeMap::new(),
        show_drums: false,
        show_looper: false,
        drum_patterns: [synth::drum::DrumPattern::default(); 8],
        drum_params: [synth::drum::DrumSlotParams::default(); NUM_DRUM_SLOTS],
        drum_volume: config.ui.drum_volume,
        drum_bpm: 120.0,
        drum_swing: 0.0,
        drum_playing: false,
        drum_recording: false,
        drum_current_pattern: 0,
        drum_step_atom,
        drum_play_atom,
        drum_rec_atom,
        drum_kit_name: String::new(),
        drum_kit_list: preset::list_drum_kits(),
        drum_kit_status: String::new(),
        drum_midi_import_path: String::new(),
        looper_atoms,
        looper_bars: 4,
        seq_target_atom,
        perf_name: String::new(),
        perf_list: preset::list_performances(),
        perf_status: String::new(),
        nav_press_time: None,
        show_pad_perf: false,
        pad_perf_map: {
            let saved = &config.ui.pad_perf_map;
            std::array::from_fn(|i| saved.get(i).cloned().flatten())
        },
        pad_perf_status: String::new(),
        pad_prev_state: [0u8; 16],
        last_config_save: std::time::Instant::now(),
        global_dirty: false,
        sf2_file_list: gui::scan_sf2_files(),
        sf2_keys_selected: None,
        sf2_keys_loaded_name: String::new(),
        sf2_drums_selected: None,
        sf2_drums_loaded_name: String::new(),
        sf2_status: String::new(),
        sf2_drums_enabled: config.sf2.drums_sf2,
        sf2_keys_soundfont: None,
        sf2_drums_soundfont: None,
        sf2_block_size: config.sf2.block_size,
        pitch_seq_step_atoms: pitch_seq_step_atoms,
        pitch_seq_enabled: [false; 2],
        pitch_seq_steps: [[synth::step_seq::PitchStep::default(); 16]; 2],
        pitch_seq_length: [16; 2],
        pitch_seq_rate: [2; 2], // 1/16
        pitch_seq_scale: [0; 2], // chromatic
        pitch_seq_swing: [0.0; 2],
        preset_search: String::new(),
    };

    app.load_edited_params(0);
    app.load_edited_params(1);
    app.send_initial_presets();

    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth")
        .with_inner_size([config.ui.window_width, config.ui.window_height])
        .with_min_inner_size([500.0, 300.0])
        .with_icon(make_piano_icon());
    let options = eframe::NativeOptions {
        viewport,
        persist_window: true,
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
