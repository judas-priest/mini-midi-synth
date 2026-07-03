#[cfg_attr(not(feature = "gui"), allow(dead_code))]
mod audio;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
mod cc_map;
mod config;
#[cfg(feature = "gui")]
mod gui;
#[cfg(target_os = "linux")]
mod input;
mod key_action;
mod midi;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
mod preset;
#[cfg_attr(not(feature = "gui"), allow(dead_code))]
mod synth;

#[allow(unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

#[cfg(target_os = "android")]
use std::sync::OnceLock;

#[cfg(target_os = "android")]
static JNI_MIDI_TX: OnceLock<midi::SharedMidiTx> = OnceLock::new();
#[cfg(target_os = "android")]
static JNI_NOTE_STATE: OnceLock<midi::NoteState> = OnceLock::new();
#[cfg(target_os = "android")]
static JNI_PAD_STATE: OnceLock<midi::PadState> = OnceLock::new();

use anyhow::Result;

#[cfg(not(target_os = "android"))]
use cpal::HostId;
use midir::MidiInputConnection;

use crate::config::Config;

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

#[cfg(not(target_os = "android"))]
fn find_host_idx(hosts: &[(HostId, &str)], name: &str) -> usize {
    hosts
        .iter()
        .position(|(_, n)| n.eq_ignore_ascii_case(name))
        .unwrap_or(0)
}

fn find_midi_port(port_names: &[String], saved_name: &str) -> Option<usize> {
    port_names.iter().position(|n| n == saved_name)
}

/// Common initialization shared by GUI and headless modes.
struct CommonInit {
    config: Config,
    patches: Vec<preset::Patch>,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    available_hosts: Vec<(HostId, &'static str)>,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    host_name: &'static str,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    host_idx: usize,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    supported_sr: Vec<u32>,
    #[cfg(feature = "gui")]
    is_jack: bool,
    actual_sr: u32,
    note_state: midi::NoteState,
    pad_state: midi::PadState,
    midi_tx_shared: midi::SharedMidiTx,
    ctrl_tx: rtrb::Producer<synth::ControlEvent>,
    feedback_rx: rtrb::Consumer<synth::ParamFeedback>,
    program_change_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    drum_step_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    drum_play_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    drum_rec_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    looper_atoms: Arc<synth::looper::LooperAtoms>,
    #[cfg(feature = "gui")]
    looper_display: Arc<std::sync::Mutex<synth::looper::LooperDisplay>>,
    seq_target_atom: Arc<AtomicU8>,
    active_part_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pitch_seq_step_atoms: Vec<Arc<AtomicU8>>,
    #[cfg(feature = "gui")]
    scope_buf: Arc<synth::ScopeBuffer>,
    #[cfg(feature = "gui")]
    midi_seq_play_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    midi_seq_pos_atom: Arc<std::sync::atomic::AtomicU32>,
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    midi_port_names: Vec<String>,
    midi_conn: Option<MidiInputConnection<()>>,
    midi_connected_name: Option<String>,
    patch_idx: usize,
    _audio_handle: audio::AudioBackend,
}

fn init_common() -> Result<CommonInit> {
    let config = Config::load();
    let patches = preset::load_all_patches();

    #[cfg(not(target_os = "android"))]
    let available_hosts = audio::available_hosts();

    #[cfg(not(target_os = "android"))]
    let host_idx = find_host_idx(&available_hosts, &config.audio.backend);
    #[cfg(not(target_os = "android"))]
    let host_id = available_hosts[host_idx].0;
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    let host_name = available_hosts[host_idx].1;
    #[cfg(feature = "gui")]
    let is_jack = {
        #[cfg(not(target_os = "android"))]
        { host_name == "JACK" }
        #[cfg(target_os = "android")]
        { false }
    };

    #[cfg(all(feature = "gui", not(target_os = "android")))]
    let supported_sr = audio::supported_sample_rates(host_id);

    #[cfg(target_os = "android")]
    let host_id = {
        // On Android, use the default host (AAudio)
        cpal::default_host().id()
    };

    let note_state = midi::new_note_state();
    let pad_state = midi::new_pad_state();

    let (midi_tx, midi_rx) = rtrb::RingBuffer::<synth::MidiEvent>::new(256);
    let (ctrl_tx, ctrl_rx) = rtrb::RingBuffer::<synth::ControlEvent>::new(256);
    let (feedback_tx, feedback_rx) = rtrb::RingBuffer::<synth::ParamFeedback>::new(64);

    let midi_tx_shared: midi::SharedMidiTx = Arc::new(Mutex::new(midi_tx));

    #[cfg(target_os = "android")]
    {
        let _ = JNI_MIDI_TX.set(midi_tx_shared.clone());
        let _ = JNI_NOTE_STATE.set(note_state.clone());
        let _ = JNI_PAD_STATE.set(pad_state.clone());
        log::info!("[init] JNI MIDI bridge globals set");
    }

    let program_change_atom = Arc::new(AtomicU8::new(255));

    let mut engine = synth::SynthEngine::new(config.audio.sample_rate as f32);
    engine.set_patches(patches.clone());
    engine.set_feedback_tx(feedback_tx);
    engine.set_program_change_atom(program_change_atom.clone());
    #[cfg(feature = "gui")]
    let drum_step_atom = engine.drum_engine.step_atom();
    #[cfg(feature = "gui")]
    let drum_play_atom = engine.drum_engine.play_atom();
    #[cfg(feature = "gui")]
    let drum_rec_atom = engine.drum_engine.rec_atom();
    #[cfg(feature = "gui")]
    let looper_atoms = engine.looper.atoms();
    #[cfg(feature = "gui")]
    let looper_display = engine.looper.display();
    let seq_target_atom = engine.seq_target_atom();
    let active_part_atom = engine.active_part_atom();
    #[cfg(feature = "gui")]
    let pitch_seq_step_atoms = engine.pitch_seq_step_atoms();
    #[cfg(feature = "gui")]
    let scope_buf = engine.scope_buffer();
    #[cfg(feature = "gui")]
    let midi_seq_play_atom = engine.midi_player_play_atom();
    #[cfg(feature = "gui")]
    let midi_seq_pos_atom = engine.midi_player_pos_atom();

    let audio_config = audio::AudioConfig {
        host_id,
        sample_rate: config.audio.sample_rate,
        buffer_size: config.audio.buffer_size,
    };
    let (audio_backend, actual_sr) =
        audio::AudioBackend::new(audio_config, engine, midi_rx, ctrl_rx)?;
    log::info!("[init] Audio: sample_rate={actual_sr}");
    eprintln!("[init] Audio: sample_rate={actual_sr}");

    // MIDI — on Android, BLE MIDI goes through Java MidiBridge → JNI, not midir.
    // midir (AMidi) doesn't receive BLE MIDI data and its polling thread wastes CPU.
    #[cfg(not(target_os = "android"))]
    let midi_port_names = midi::list_ports().unwrap_or_default();
    #[cfg(target_os = "android")]
    let midi_port_names: Vec<String> = Vec::new();

    eprintln!("[init] MIDI ports: {midi_port_names:?}");

    #[cfg(not(target_os = "android"))]
    let midi_port_idx = config
        .midi
        .port_name
        .as_deref()
        .and_then(|name| find_midi_port(&midi_port_names, name))
        .or(if midi_port_names.is_empty() { None } else { Some(0) });

    #[cfg(not(target_os = "android"))]
    let midi_conn: Option<MidiInputConnection<()>> = midi_port_idx.and_then(|idx| {
        midi::connect(idx, midi_tx_shared.clone(), note_state.clone(), pad_state.clone()).ok()
    });
    #[cfg(target_os = "android")]
    let midi_conn: Option<MidiInputConnection<()>> = None;

    #[cfg(not(target_os = "android"))]
    let midi_connected_name = midi_port_idx
        .and_then(|idx| midi_port_names.get(idx))
        .filter(|_| midi_conn.is_some())
        .cloned();
    #[cfg(target_os = "android")]
    let midi_connected_name: Option<String> = None;

    let patch_idx = config
        .ui
        .last_preset
        .as_deref()
        .and_then(|name| patches.iter().position(|p| p.name == name))
        .unwrap_or(0);

    Ok(CommonInit {
        config,
        patches,
        #[cfg(all(feature = "gui", not(target_os = "android")))]
        available_hosts,
        #[cfg(all(feature = "gui", not(target_os = "android")))]
        host_name,
        #[cfg(all(feature = "gui", not(target_os = "android")))]
        host_idx,
        #[cfg(all(feature = "gui", not(target_os = "android")))]
        supported_sr,
        #[cfg(feature = "gui")]
        is_jack,
        actual_sr,
        note_state,
        pad_state,
        midi_tx_shared,
        ctrl_tx,
        feedback_rx,
        program_change_atom,
        #[cfg(feature = "gui")]
        drum_step_atom,
        #[cfg(feature = "gui")]
        drum_play_atom,
        #[cfg(feature = "gui")]
        drum_rec_atom,
        #[cfg(feature = "gui")]
        looper_atoms,
        #[cfg(feature = "gui")]
        looper_display,
        seq_target_atom,
        active_part_atom,
        #[cfg(feature = "gui")]
        pitch_seq_step_atoms,
        #[cfg(feature = "gui")]
        scope_buf,
        #[cfg(feature = "gui")]
        midi_seq_play_atom,
        #[cfg(feature = "gui")]
        midi_seq_pos_atom,
        midi_port_names,
        midi_conn,
        midi_connected_name,
        patch_idx,
        _audio_handle: audio_backend,
    })
}

// On Android the real entry point is `android_main` (see bottom of file).
// This dummy keeps the bin target compilable for `cargo ndk build`.
#[cfg(target_os = "android")]
fn main() {}

#[cfg(not(target_os = "android"))]
fn main() -> Result<()> {
    #[cfg(target_os = "linux")]
    suppress_alsa_errors();

    #[cfg(feature = "gui")]
    {
        let headless = std::env::args().any(|a| a == "--headless" || a == "-H");
        if headless {
            run_headless()
        } else {
            run_gui()
        }
    }

    #[cfg(not(feature = "gui"))]
    run_headless()
}

// ---------------------------------------------------------------------------
// Headless / CLI mode
// ---------------------------------------------------------------------------
fn run_headless() -> Result<()> {
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
        &c.config.ui.keybinds,
        shutdown_flag.clone(),
    );
    #[cfg(not(target_os = "linux"))]
    let kb_rx: Option<(std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<key_action::KeyAction>)> = None;
    let seq_target_local: Arc<AtomicU8> = c.seq_target_atom.clone();
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

        // Process keyboard actions
        if let Some((_, ref rx)) = kb_rx {
            use key_action::KeyAction;
            while let Ok(action) = rx.try_recv() {
                match &action {
                    KeyAction::SwitchPart(p) => {
                        c.active_part_atom.store(*p, Ordering::Relaxed);
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
                        let current = seq_target_local.load(Ordering::Relaxed);
                        let new = if current == 0 { 1 } else { 0 };
                        seq_target_local.store(new, Ordering::Relaxed);
                        eprintln!("[input] Mode: {}", if new == 0 { "Drums" } else { "Synth" });
                    }
                    KeyAction::PrevPreset | KeyAction::NextPreset => {
                        // Preset navigation not supported in headless mode
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

fn parse_sf2_file(path: &str) -> Result<std::sync::Arc<rustysynth::SoundFont>> {
    use std::io::BufReader;
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let sf = rustysynth::SoundFont::new(&mut reader)
        .map_err(|e| anyhow::anyhow!("SF2 parse error: {e}"))?;
    Ok(std::sync::Arc::new(sf))
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
fn run_gui(
    #[cfg(target_os = "android")]
    android_app: winit::platform::android::activity::AndroidApp,
) -> Result<()> {
    use crate::cc_map::CcMap;
    use crate::synth::drum::NUM_DRUM_SLOTS;

    let c = init_common()?;

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
        _frame_count: 0,
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
        _program_change_atom: c.program_change_atom,
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
        drum_volume: c.config.ui.drum_volume,
        drum_bpm: 120.0,
        drum_swing: 0.0,
        drum_playing: false,
        drum_recording: false,
        drum_current_pattern: 0,
        drum_step_atom: c.drum_step_atom,
        drum_play_atom: c.drum_play_atom,
        drum_rec_atom: c.drum_rec_atom,
        drum_kit_name: String::new(),
        drum_kit_list: preset::list_drum_kits(),
        drum_midi_import_path: String::new(),
        looper_atoms: c.looper_atoms,
        looper_display: c.looper_display,
        looper_bars: 4,
        looper_quantize: 0,
        looper_sync_bpm: c.config.ui.looper_sync_bpm,
        looper_bpm: 120.0,
        looper_clear_confirm: None,
        looper_layer_mute: [false; 256],
        looper_solo_layer: None,
        seq_target_atom: c.seq_target_atom,
        active_part_atom: c.active_part_atom,
        keybinds: gui::keybinds::Keybinds::from_config(&c.config.ui.keybinds),
        show_keybinds_window: false,
        keybind_capturing: None,
        perf_name: String::new(),
        perf_list: preset::list_performances(),
        nav_press_time: None,
        show_pad_perf: false,
        pad_perf_map: {
            let saved = &c.config.ui.pad_perf_map;
            std::array::from_fn(|i| saved.get(i).cloned().flatten())
        },
        pad_prev_state: [0u8; 16],
        last_config_save: std::time::Instant::now(),
        global_dirty: false,
        sf2_file_list: gui::scan_sf2_files(),
        sf2_keys_selected: None,
        sf2_keys_loaded_name: String::new(),
        sf2_drums_selected: None,
        sf2_drums_loaded_name: String::new(),
        sf2_drums_enabled: c.config.sf2.drums_sf2,
        sf2_keys_soundfont: None,
        sf2_drums_soundfont: None,
        sf2_block_size: c.config.sf2.block_size,
        sf2_sample_offset_ms: c.config.sf2.sample_offset_ms,
        pitch_seq_step_atoms: c.pitch_seq_step_atoms,
        pitch_seq_enabled: [false; 2],
        pitch_seq_steps: [[synth::step_seq::PitchStep::default(); 16]; 2],
        pitch_seq_length: [16; 2],
        pitch_seq_rate: [2; 2],
        pitch_seq_scale: [0; 2],
        pitch_seq_swing: [0.0; 2],
        preset_search: String::new(),
        scope_buf: c.scope_buf,
        split_point: 60,
        show_fx_chain: false,
        show_midi_seq: false,
        midi_seq_path: String::new(),
        midi_seq_tracks: Vec::new(),
        midi_seq_playing: false,
        midi_seq_looping: true,
        midi_seq_bpm: None,
        midi_seq_play_atom: c.midi_seq_play_atom,
        midi_seq_pos_atom: c.midi_seq_pos_atom,
        midi_seq_file_pick: None,
        undo_stack: std::collections::VecDeque::new(),
        redo_stack: Vec::new(),
        toasts: Vec::new(),
    };

    app.load_edited_params(0);
    app.load_edited_params(1);
    app.send_initial_presets();

    #[cfg(not(target_os = "android"))]
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_app_id("mini_midi_synth")
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

// ---------------------------------------------------------------------------
// Android entry point
// ---------------------------------------------------------------------------
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
        log::error!("PANIC: {info}");
    }));

    log::info!("android_main started");

    // Set data dir from Android internal storage
    if let Some(path) = app.internal_data_path() {
        std::env::set_var("MINI_SYNTH_DATA_DIR", path.to_string_lossy().as_ref());
        log::info!("data dir: {}", path.display());
    }
    if let Err(e) = run_gui(app) {
        log::error!("run_gui failed: {e}");
    }
}

/// JNI entry point: called from Java MidiBridge.onMidiData(byte[])
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_minimidisynth_MidiBridge_onMidiData<'local>(
    mut env: jni::JNIEnv<'local>,
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
        let consumed = midi::parse_and_push(&bytes[offset..], tx, ns, ps);
        if consumed == 0 { break }
        offset += consumed;
    }
}
