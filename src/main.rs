mod audio;
mod config;
mod gui;
mod midi;
mod preset;
mod synth;

use std::sync::{Arc, Mutex};

use anyhow::Result;

use cpal::HostId;
use midir::MidiInputConnection;

use crate::config::Config;

/// Suppress ALSA lib error messages (not our own stderr).
#[cfg(target_os = "linux")]
fn suppress_alsa_errors() {
    // Link against libasound and set a no-op error handler.
    // This only suppresses ALSA's internal messages, not eprintln!().
    #[link(name = "asound")]
    extern "C" {
        fn snd_lib_error_set_handler(handler: *const std::ffi::c_void) -> std::ffi::c_int;
    }
    unsafe {
        // Passing a null pointer sets a no-op handler
        snd_lib_error_set_handler(std::ptr::null());
    }
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

    let (midi_tx, midi_rx) = rtrb::RingBuffer::<synth::MidiEvent>::new(256);
    let (ctrl_tx, ctrl_rx) = rtrb::RingBuffer::<synth::ControlEvent>::new(16);

    let midi_tx_shared: midi::SharedMidiTx = Arc::new(Mutex::new(midi_tx));

    let engine = synth::SynthEngine::new(config.audio.sample_rate as f32);

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
        midi::connect(idx, midi_tx_shared.clone(), note_state.clone()).ok()
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
    let on_midi_reconnect = Box::new(move |port_idx: usize| -> Result<(), String> {
        let new_conn = midi::connect(port_idx, midi_tx_cb.clone(), note_state_cb.clone())
            .map_err(|e| e.to_string())?;
        let mut handle = midi_handle_cb.lock().map_err(|e| e.to_string())?;
        *handle = Some(new_conn);
        Ok(())
    });

    let mut app = gui::App {
        presets,
        preset_idx,
        note_state,
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
        edited_params: std::collections::BTreeMap::new(),
        params_dirty: false,
        on_midi_reconnect: Some(on_midi_reconnect),
    };

    // Initialize edited params from selected preset
    app.load_edited_params();

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([config.ui.window_width, config.ui.window_height])
            .with_min_inner_size([500.0, 300.0]),
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
