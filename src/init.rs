#[allow(unused_imports)]
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::{Arc, Mutex};

use anyhow::Result;

#[cfg(not(target_os = "android"))]
use cpal::HostId;
use midir::MidiInputConnection;

use crate::config::Config;
use crate::{audio, midi, metrics, preset, synth, trash};

#[cfg(target_os = "android")]
use crate::{AUDIO_PAUSED, JNI_MIDI_TX, JNI_NOTE_STATE, JNI_PAD_STATE};

pub static AMIDI_PORT: std::sync::OnceLock<std::sync::Arc<crate::amidi::AmidiPort>> = std::sync::OnceLock::new();

/// Suppress ALSA lib error messages (not our own stderr).
#[cfg(target_os = "linux")]
pub(crate) fn suppress_alsa_errors() {
    #[link(name = "asound")]
    extern "C" {
        fn snd_lib_error_set_handler(handler: *const std::ffi::c_void) -> std::ffi::c_int;
    }
    unsafe {
        snd_lib_error_set_handler(std::ptr::null());
    }
}

#[cfg(not(target_os = "android"))]
pub(crate) fn find_host_idx(hosts: &[(HostId, &str)], name: &str) -> usize {
    hosts
        .iter()
        .position(|(_, n)| n.eq_ignore_ascii_case(name))
        .unwrap_or(0)
}

pub(crate) fn find_midi_port(port_names: &[String], saved_name: &str) -> Option<usize> {
    port_names.iter().position(|n| n == saved_name)
}

/// Единый бандл общих хэндлов движка (ST1: 17 хэндлов × 6 мест → 3).
/// Клон = клон Arc'ов (GUI-поток). Новый хэндл: поле здесь + строка в
/// `SynthEngine::handles()` + присвоение потребителю — больше нигде.
#[derive(Clone)]
pub struct EngineHandles {
    #[cfg(feature = "gui")]
    pub drum_step_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub drum_play_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub drum_rec_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub drum_pattern_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub drum_undo_atom: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "gui")]
    pub drum_redo_atom: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "gui")]
    pub looper_atoms: Arc<synth::looper::LooperAtoms>,
    #[cfg(feature = "gui")]
    pub looper_display: Arc<std::sync::Mutex<synth::looper::LooperDisplay>>,
    pub seq_target_atom: Arc<AtomicU8>,
    pub active_part_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub sustain_atom: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "gui")]
    pub master_rec_state_atom: Arc<std::sync::atomic::AtomicU8>,
    pub master_rec_cycle_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub pitch_seq_step_atoms: Vec<Arc<AtomicU8>>,
    #[cfg(feature = "gui")]
    pub scope_buf: Arc<synth::ScopeBuffer>,
    #[cfg(feature = "gui")]
    pub midi_seq_play_atom: Arc<AtomicU8>,
    #[cfg(feature = "gui")]
    pub midi_seq_pos_atom: Arc<std::sync::atomic::AtomicU32>,
}

/// Common initialization shared by GUI and headless modes.
pub(crate) struct CommonInit {
    pub(crate) config: Config,
    pub(crate) patches: Vec<preset::Patch>,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    pub(crate) available_hosts: Vec<(HostId, &'static str)>,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    pub(crate) host_name: &'static str,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    pub(crate) host_idx: usize,
    #[cfg(all(feature = "gui", not(target_os = "android")))]
    pub(crate) supported_sr: Vec<u32>,
    #[cfg(feature = "gui")]
    pub(crate) is_jack: bool,
    pub(crate) actual_sr: u32,
    pub(crate) note_state: midi::NoteState,
    pub(crate) pad_state: midi::PadState,
    pub(crate) midi_tx_shared: midi::SharedMidiTx,
    /// Bank-2 console pad notes, mirrored by the engine (Task 4).
    pub(crate) console_rx: std::sync::mpsc::Receiver<(u8, bool)>,
    pub(crate) flash_rx: rtrb::Consumer<midi::NoteFlash>,
    pub(crate) ctrl_tx: metrics::CountingProducer<synth::ControlEvent>,
    pub(crate) feedback_rx: rtrb::Consumer<synth::ParamFeedback>,
    pub(crate) program_change_atom: Arc<AtomicU8>,
    /// Общие хэндлы движка (ST1): один бандл вместо 17 полей.
    pub(crate) handles: EngineHandles,
    #[cfg_attr(not(feature = "gui"), allow(dead_code))]
    pub(crate) midi_port_names: Vec<String>,
    pub(crate) midi_conn: Option<MidiInputConnection<()>>,
    pub(crate) midi_connected_name: Option<String>,
    pub(crate) patch_idx: usize,
    pub(crate) _audio_handle: audio::AudioBackend,
    pub(crate) audio_disconnected: Arc<AtomicBool>,
    /// Heap objects handed back by the engine and the audio callback. Drained
    /// (and thereby freed) on the GUI or headless main thread.
    pub(crate) engine_trash_rx: trash::TrashRx,
    pub(crate) cb_trash_rx: trash::TrashRx,
}

pub(crate) fn init_common() -> Result<CommonInit> {
    let config = Config::load();
    let patches = preset::load_all_patches();

    // Warm static DSP tables BEFORE the audio stream exists: both are lazily
    // built otherwise and the first build would land in the audio callback
    // (wavetable bank on the first note-on, LFO sine LUT on the first tick).
    synth::oscillator::warmup_builtin_wavetable();
    synth::lfo::warmup_sine_lut();

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
    // 512 rather than 256: a full state resend (patches, per-part settings,
    // macros, CC map, drum kit) is around 165 events in one burst, and an
    // overflow there silently loses settings. 512 events is 16 KB.
    let (ctrl_tx, ctrl_rx) = rtrb::RingBuffer::<synth::ControlEvent>::new(512);
    let (feedback_tx, feedback_rx) = rtrb::RingBuffer::<synth::ParamFeedback>::new(64);

    let midi_tx_shared: midi::SharedMidiTx = Arc::new(Mutex::new(midi_tx));

    #[cfg(target_os = "android")]
    {
        let _ = JNI_MIDI_TX.set(midi_tx_shared.clone());
        let _ = JNI_NOTE_STATE.set(note_state.clone());
        let _ = JNI_PAD_STATE.set(pad_state.clone());
        log::info!("[init] JNI MIDI bridge globals set");
    }

    let (console_snd, console_rx) = std::sync::mpsc::channel::<(u8, bool)>();
    let _ = midi::CONSOLE_TX.set(std::sync::Mutex::new(Some(console_snd)));

    // NoteFlash: затухающие следы нот на клавиатуре (см. src/gui/note_flash.rs)
    let (flash_tx, flash_rx) = rtrb::RingBuffer::<midi::NoteFlash>::new(128);
    midi::init_note_flash(flash_tx);

    let program_change_atom = Arc::new(AtomicU8::new(255));

    // Two independent trash queues: rtrb producers cannot be cloned, and both
    // the engine (displaced soundfonts) and the callback (spent control
    // events) need somewhere to put heap objects they must not free.
    let (engine_trash_tx, engine_trash_rx) = trash::channel();
    let (cb_trash_tx, cb_trash_rx) = trash::channel();

    let mut engine = synth::SynthEngine::new(config.audio.sample_rate as f32);
    engine.set_patches(patches.clone());
    engine.set_feedback_tx(feedback_tx);
    engine.set_trash_tx(engine_trash_tx);
    engine.set_program_change_atom(program_change_atom.clone());
    let h = engine.handles();

    let amidi_port = {
        let port = std::sync::Arc::new(crate::amidi::AmidiPort::new());
        let _ = AMIDI_PORT.set(port.clone());
        port
    };

    let audio_paused = Arc::new(AtomicBool::new(false));
    #[cfg(target_os = "android")]
    {
        let _ = AUDIO_PAUSED.set(audio_paused.clone());
    }

    let audio_config = audio::AudioConfig {
        host_id,
        sample_rate: config.audio.sample_rate,
        buffer_size: config.audio.buffer_size,
    };
    let (audio_backend, actual_sr) = audio::AudioBackend::new(
        audio_config,
        engine,
        midi_rx,
        ctrl_rx,
        audio::AudioShared {
            amidi_port,
            note_state: note_state.clone(),
            pad_state: pad_state.clone(),
            paused: audio_paused,
            trash: cb_trash_tx,
        },
    )?;
    let audio_disconnected = audio_backend.disconnected.clone();
    log::info!("[init] Audio: sample_rate={actual_sr}");

    // MIDI — On Android, midir AMidi polling thread causes audio callback starvation
    // (ring buffer fills up, no sound). BLE MIDI goes through Java MidiBridge → JNI.
    // USB MIDI: user can connect manually via Settings, but no auto-connect at startup.
    #[cfg(not(target_os = "android"))]
    let midi_port_names = midi::list_ports().unwrap_or_default();
    #[cfg(target_os = "android")]
    let midi_port_names: Vec<String> = Vec::new();

    log::info!("[init] MIDI ports: {midi_port_names:?}");

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
        console_rx,
        flash_rx,
        ctrl_tx: metrics::CountingProducer::new(ctrl_tx),
        feedback_rx,
        program_change_atom,
        handles: h,
        midi_port_names,
        midi_conn,
        midi_connected_name,
        patch_idx,
        _audio_handle: audio_backend,
        audio_disconnected,
        engine_trash_rx,
        cb_trash_rx,
    })
}
