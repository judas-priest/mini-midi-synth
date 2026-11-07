use anyhow::Result;
use midir::MidiInputConnection;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use crate::init::{find_midi_port, init_common};
use crate::{cc_map, key_action, midi, synth};

#[cfg(target_os = "linux")]
use crate::input;

// ---------------------------------------------------------------------------
// Headless / CLI mode
// ---------------------------------------------------------------------------
pub(crate) fn run_headless() -> Result<()> {
    static SHUTDOWN: AtomicBool = AtomicBool::new(false);

    // Signal handler for graceful shutdown (sigaction is MT-safe, unlike signal)
    #[cfg(target_os = "linux")]
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

    let mut c = init_common()?;

    // --- Send full state to engine (same as gui::App::send_initial_presets) ---

    // Load patch params for all parts
    let layer_configs: Vec<(usize, bool, f32, u8, u8, bool, u8)> = vec![
        (c.patch_idx, true,  c.config.ui.layer_a_volume, 0, 127, c.config.sf2.layer_a_sf2, c.config.sf2.layer_a_program),
        (0,            false, c.config.ui.layer_b_volume, 60, 127, c.config.sf2.layer_b_sf2, c.config.sf2.layer_b_program),
    ];

    for i in 0..synth::MAX_PARTS {
        let (pidx, enabled, volume, min_note, max_note, sf2_mode, sf2_program) =
            if i < layer_configs.len() {
                layer_configs[i]
            } else {
                (0, false, 0.8, 0u8, 127u8, false, 0u8)
            };

        // Send preset with full params
        if let Some(patch) = c.patches.get(pidx) {
            let _ = c.ctrl_tx.push(synth::ControlEvent::load_patch_from(i, patch));
        }

        let _ = c.ctrl_tx.push(synth::ControlEvent::SetPartEnabled { part: i, enabled });
        let _ = c.ctrl_tx.push(synth::ControlEvent::SetPartVolume { part: i, volume });
        let _ = c.ctrl_tx.push(synth::ControlEvent::SetPartRange { part: i, min_note, max_note });

        if sf2_mode {
            let _ = c.ctrl_tx.push(synth::ControlEvent::SetPartSf2Mode { part: i, enabled: true });
            let _ = c.ctrl_tx.push(synth::ControlEvent::SetPartSf2Program { part: i, program: sf2_program, bank: 0 });
        }
    }

    // Global params
    let global_params = [
        ("master_volume", c.config.ui.master_volume),
        ("master_tone", c.config.ui.master_tone),
        ("reverb_mix", c.config.ui.fader_reverb),
        ("delay_mix", c.config.ui.fader_delay),
        ("pitch_bend_range", c.config.ui.pitch_bend_range as f32),
    ];
    for (key, val) in &global_params {
        if let Some(static_key) = cc_map::resolve_key(key) {
            let _ = c.ctrl_tx.push(synth::ControlEvent::SetGlobalParam { key: static_key, value: *val });
        }
    }
    let _ = c.ctrl_tx.push(synth::ControlEvent::DrumSetVolume { volume: c.config.ui.drum_volume });
    let _ = c.ctrl_tx.push(synth::ControlEvent::SetDspTrim { db: c.config.audio.dsp_trim_db });
    let _ = c.ctrl_tx.push(synth::ControlEvent::SetSf2Trim { db: c.config.audio.sf2_trim_db });

    // --- SF2 loading ---
    let keys_path = c.config.sf2.keys_file_path.clone()
        .or_else(|| c.config.sf2.file_path.clone());
    if let Some(sf2_path) = keys_path {
        match parse_sf2_file(&sf2_path) {
            Ok(sf) => {
                let _ = c.ctrl_tx.push(synth::ControlEvent::SetSf2BlockSize { size: c.config.sf2.block_size });
                let _ = c.ctrl_tx.push(synth::ControlEvent::LoadKeysSoundFont { soundfont: sf });
                eprintln!("[cli] Loaded keys SF2: {sf2_path}");
            }
            Err(e) => eprintln!("[cli] Failed to load keys SF2 '{sf2_path}': {e}"),
        }
    }
    if let Some(sf2_path) = c.config.sf2.drums_file_path.clone() {
        match parse_sf2_file(&sf2_path) {
            Ok(sf) => {
                let _ = c.ctrl_tx.push(synth::ControlEvent::SetSf2BlockSize { size: c.config.sf2.block_size });
                let _ = c.ctrl_tx.push(synth::ControlEvent::LoadDrumsSoundFont { soundfont: sf });
                if c.config.sf2.drums_sf2 {
                    let _ = c.ctrl_tx.push(synth::ControlEvent::SetDrumsSf2Mode { enabled: true });
                }
                eprintln!("[cli] Loaded drums SF2: {sf2_path}");
            }
            Err(e) => eprintln!("[cli] Failed to load drums SF2 '{sf2_path}': {e}"),
        }
    }

    // SF2 sample offset
    if c.config.sf2.sample_offset_ms > 0.0 {
        let _ = c.ctrl_tx.push(synth::ControlEvent::SetSf2SampleOffset { ms: c.config.sf2.sample_offset_ms });
    }

    // --- Print status ---
    eprintln!("[cli] mini_midi_synth running (headless)");
    eprintln!("[cli] Audio: {}Hz, buffer {}", c.actual_sr, c.config.audio.buffer_size);
    if let Some(ref name) = c.midi_connected_name {
        eprintln!("[cli] MIDI: {name}");
    } else {
        eprintln!("[cli] MIDI: not connected");
    }
    if let Some(patch) = c.patches.get(c.patch_idx) {
        eprintln!("[cli] Preset: {}", patch.name);
    }
    eprintln!("[cli] Press Ctrl+C to exit.");

    // --- Keyboard input thread (evdev) ---
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "linux")]
    let kb_rx = input::spawn_keyboard_thread(
        c.config.ui.keybinds.as_ref().unwrap_or(&std::collections::HashMap::new()),
        shutdown_flag.clone(),
    );
    #[cfg(not(target_os = "linux"))]
    let kb_rx: Option<(std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<key_action::KeyAction>)> = None;
    let seq_target_local: Arc<AtomicU8> = c.handles.seq_target_atom.clone();
    let mut drum_playing = false;

    // --- Main loop: MIDI reconnect + Program Change + keyboard actions ---
    let mut midi_handle: Option<MidiInputConnection<()>> = c.midi_conn.take();
    let mut last_midi_check = std::time::Instant::now();
    let mut current_midi_port: Option<String> = c.midi_connected_name.clone();

    loop {
        if SHUTDOWN.load(Ordering::SeqCst) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));

        // Drain feedback (ignore in CLI)
        while c.feedback_rx.pop().is_ok() {}

        // Free whatever the audio thread handed over.
        c.engine_trash_rx.drain();
        c.cb_trash_rx.drain();

        // Headless has no reconnect path (the GUI rebuilds the engine on the
        // fly); say so instead of going silently mute.
        if c.audio_disconnected.swap(false, Ordering::SeqCst) {
            eprintln!("[cli] Audio device disconnected — restart to recover");
        }

        // Process keyboard actions
        if let Some((_, ref rx)) = kb_rx {
            use key_action::KeyAction;
            while let Ok(action) = rx.try_recv() {
                match &action {
                    KeyAction::SwitchPart(p) => {
                        c.handles.active_part_atom.store(*p, Ordering::Relaxed);
                        seq_target_local.store(1, Ordering::Relaxed);
                        eprintln!("[input] Part A{}", p + 1);
                    }
                    KeyAction::LooperRecord => {
                        let _ = c.ctrl_tx.push(synth::ControlEvent::LooperRecord);
                    }
                    KeyAction::LooperTogglePlay => {
                        let _ = c.ctrl_tx.push(synth::ControlEvent::LooperTogglePlay);
                    }
                    KeyAction::LooperUndo => {
                        let _ = c.ctrl_tx.push(synth::ControlEvent::LooperUndo);
                    }
                    KeyAction::LooperClear => {
                        let _ = c.ctrl_tx.push(synth::ControlEvent::LooperClear);
                    }
                    KeyAction::DrumTogglePlay => {
                        drum_playing = !drum_playing;
                        let _ = c.ctrl_tx.push(synth::ControlEvent::DrumSeqPlay { playing: drum_playing });
                        eprintln!("[input] Drums: {}", if drum_playing { "Play" } else { "Stop" });
                    }
                    KeyAction::ToggleDrumsSynth => {
                        let next = if seq_target_local.load(Ordering::Relaxed) == 0 { 1 } else { 0 };
                        seq_target_local.store(next, Ordering::Relaxed);
                        eprintln!("[input] Mode: {}", if next == 0 { "Drums" } else { "Synth" });
                    }
                    KeyAction::PrevPreset | KeyAction::NextPreset => {
                        // Preset navigation not supported in headless mode
                    }
                    KeyAction::TogglePadPerf => {
                        // Pad performance panel is GUI-only
                    }
                }
            }
        }

        // MIDI auto-reconnect every 3 seconds
        if last_midi_check.elapsed() >= std::time::Duration::from_secs(3) {
            last_midi_check = std::time::Instant::now();
            if midi_handle.is_none() {
                let ports = midi::list_ports().unwrap_or_default();
                let target = c.config.midi.port_name.as_deref()
                    .and_then(|name| find_midi_port(&ports, name))
                    .or(if ports.is_empty() { None } else { Some(0) });
                if let Some(idx) = target {
                    if let Ok(conn) = midi::connect(idx, c.midi_tx_shared.clone(), c.note_state.clone(), c.pad_state.clone()) {
                        let name = ports.get(idx).cloned().unwrap_or_default();
                        eprintln!("[cli] MIDI connected: {name}");
                        current_midi_port = Some(name);
                        midi_handle = Some(conn);
                    }
                }
            }
        }

        // Program Change polling
        let pc = c.program_change_atom.swap(255, Ordering::SeqCst);
        if pc != 255 {
            let idx = pc as usize;
            if idx < c.patches.len() {
                if let Some(patch) = c.patches.get(idx) {
                    let _ = c.ctrl_tx.push(synth::ControlEvent::load_patch_from(0, patch));
                    eprintln!("[cli] Program Change → {}: {}", idx, patch.name);
                }
            }
        }
    }

    // Graceful shutdown
    shutdown_flag.store(true, Ordering::SeqCst);
    eprintln!("\n[cli] Shutting down...");
    let _ = c.ctrl_tx.push(synth::ControlEvent::AllNotesOff);
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Wait for keyboard thread
    if let Some((handle, _)) = kb_rx {
        let _ = handle.join();
    }

    // Drop MIDI before exit
    drop(midi_handle);
    let _ = current_midi_port;

    Ok(())
}

pub(crate) fn parse_sf2_file(path: &str) -> Result<std::sync::Arc<rustysynth::SoundFont>> {
    use std::io::BufReader;
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let sf = rustysynth::SoundFont::new(&mut reader)
        .map_err(|e| anyhow::anyhow!("SF2 parse error: {e}"))?;
    Ok(std::sync::Arc::new(sf))
}
