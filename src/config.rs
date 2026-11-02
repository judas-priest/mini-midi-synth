//! Persistent settings: audio backend, buffer size, sample rate, MIDI port.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[cfg(target_os = "android")]
static ANDROID_DATA_DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_android_data_dir(path: std::path::PathBuf) {
    let _ = ANDROID_DATA_DIR.set(path);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub audio: AudioSettings,
    pub midi: MidiSettings,
    pub ui: UiSettings,
    #[serde(default)]
    pub cc_map: Option<crate::cc_map::CcMapRaw>,
    #[serde(default)]
    pub sf2: Sf2Settings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Sf2Settings {
    /// Path to the loaded SF2 file (legacy, used as fallback for keys_file_path).
    #[serde(default)]
    pub file_path: Option<String>,
    /// Path to the SF2 file for keys (parts A/B).
    #[serde(default)]
    pub keys_file_path: Option<String>,
    /// Path to the SF2 file for drums.
    #[serde(default)]
    pub drums_file_path: Option<String>,
    /// Layer A uses SF2 mode.
    #[serde(default)]
    pub layer_a_sf2: bool,
    /// GM program number for part A.
    #[serde(default)]
    pub layer_a_program: u8,
    /// Layer B uses SF2 mode.
    #[serde(default)]
    pub layer_b_sf2: bool,
    /// GM program number for part B.
    #[serde(default)]
    pub layer_b_program: u8,
    /// Drums use SF2 mode.
    #[serde(default)]
    pub drums_sf2: bool,
    /// Pad-note remap tables per drums kit (key = kit file name or "internal"), len 16.
    #[serde(default)]
    pub drums_remaps: std::collections::HashMap<String, Vec<u8>>,
    /// Per-pad velocity overrides per drums kit (key = kit file name or
    /// "internal"), len 16, 0 = off.
    #[serde(default)]
    pub drums_vels: std::collections::HashMap<String, Vec<u8>>,
    /// SF2 internal block size (samples). Higher = less CPU, more latency.
    #[serde(default = "default_sf2_block_size")]
    pub block_size: usize,
    /// SF2 sample start offset in ms — skip initial attack to reduce perceived latency.
    #[serde(default)]
    pub sample_offset_ms: f32,
}

fn default_sf2_block_size() -> usize { 8 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    pub backend: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    /// DSP-bus fine trim in dB (parts + internal drums; not SF2). 0.0 = unity.
    /// Matching is carried by the measured SF2_CAL_GAIN, so both defaults are 0.
    #[serde(default)]
    pub dsp_trim_db: f32,
    /// SF2 fine trim in dB on top of the measured SF2_CAL_GAIN.
    #[serde(default)]
    pub sf2_trim_db: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiSettings {
    pub port_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct PartUiState {
    #[serde(default)]
    pub mute: bool,
    /// Note mode: 0=Poly, 1=Bass (the old root_only), 2=Root (chord root).
    #[serde(default)]
    pub note_mode: u8,
    /// Legacy flag (pre-note_mode configs). Never written anymore
    /// (skip_serializing); kept so old files parse and can be mapped:
    /// root_only=true + no explicit mode → Bass (see migrate_legacy_root_only).
    #[serde(default, skip_serializing)]
    pub root_only: bool,
    /// Sustain pedal affects this part — default true (behaviour as before).
    #[serde(default = "default_true")]
    pub sustain_enabled: bool,
}

impl PartUiState {
    /// Read-side legacy migration: root_only=true with no explicit note_mode
    /// means the old bass split → NOTE_MODE_BASS. No-op otherwise.
    pub fn migrate_legacy_root_only(&mut self) {
        if self.root_only && self.note_mode == crate::synth::NOTE_MODE_POLY {
            self.note_mode = crate::synth::NOTE_MODE_BASS;
        }
    }
}

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSettings {
    pub window_width: f32,
    pub window_height: f32,
    #[serde(default)]
    pub maximized: bool,
    pub last_preset: Option<String>,
    #[serde(default)]
    pub collapsed_categories: Vec<String>,
    #[serde(default = "default_master_volume")]
    pub master_volume: f32,
    #[serde(default = "default_master_tone")]
    pub master_tone: f32,
    #[serde(default = "default_fader_reverb")]
    pub fader_reverb: f32,
    #[serde(default = "default_fader_delay")]
    pub fader_delay: f32,
    #[serde(default = "default_pitch_bend_range")]
    pub pitch_bend_range: u8,
    #[serde(default = "default_drum_volume")]
    pub drum_volume: f32,
    /// Drum velocity off: all hits at 127 (for velocity-less controllers)
    #[serde(default)]
    pub drum_fixed_velocity: bool,
    /// Metronome click synced to the drum step sequencer.
    #[serde(default)]
    pub drum_metronome: bool,
    /// Static pedal-up release in seconds (None = off, patch release)
    #[serde(default)]
    pub sustain_release: Option<f32>,
    /// SF2 release-time scale, 1.0 = file defaults (GM2 CC72 semantics)
    #[serde(default = "default_sf2_release_scale")]
    pub sf2_release_scale: f32,
    #[serde(default = "default_layer_a_volume")]
    pub layer_a_volume: f32,
    #[serde(default = "default_layer_b_volume")]
    pub layer_b_volume: f32,
    /// Pad performance map: 16 slots (notes 36-51) → performance name.
    #[serde(default)]
    pub pad_perf_map: Vec<Option<String>>,
    /// Keyboard split note persisted across restarts.
    #[serde(default)]
    pub split_point: Option<u8>,
    /// Last loaded performance — restored at startup.
    #[serde(default)]
    pub last_performance: Option<String>,
    /// Live per-part toggles that must survive restarts (mute / root-only).
    #[serde(default)]
    pub parts_state: Vec<PartUiState>,
    /// Bank-2 LED console companion (unix socket). Off = never connect.
    #[serde(default)]
    pub led_companion: bool,
    /// Companion spawn command path (python3 hardcoded); `~/` expands to HOME.
    #[serde(default = "default_led_companion_cmd")]
    pub led_companion_cmd: String,
    /// Keybindings: egui key name → action string.
    #[serde(default)]
    pub keybinds: std::collections::HashMap<String, String>,
    /// (#44) Top-bar telemetry (host/SR/buf/dsp load, drops/errs). Hidden by default.
    #[serde(default)]
    pub show_telemetry: bool,
    /// (#30) Drum sequencer + looper unit docked to the window bottom (true,
    /// as before) or drawn as a whole on the Drums tab (false).
    #[serde(default = "default_true")]
    pub dock_bottom_unit: bool,
    /// (#34) Last top-left corner of the Instrument Browser window; None =
    /// egui default. Written every frame while the window is open.
    #[serde(default)]
    pub inst_browser_pos: Option<[f32; 2]>,
}

fn default_led_companion_cmd() -> String { "~/Projects/smk37_config/pad_console.py".to_string() }

/// Expand a leading `~/` to the user home directory; everything else verbatim.
pub fn expand_home(p: &str) -> std::path::PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(p)
}

fn default_sf2_release_scale() -> f32 { 1.0 }

fn default_master_volume() -> f32 { 0.8 }
fn default_master_tone() -> f32 { 20000.0 }
fn default_fader_reverb() -> f32 { 0.0 }
fn default_fader_delay() -> f32 { 0.0 }
fn default_pitch_bend_range() -> u8 { 2 }
fn default_drum_volume() -> f32 { 0.8 }
fn default_layer_a_volume() -> f32 { 0.8 }
fn default_layer_b_volume() -> f32 { 0.5 }

impl Default for Config {
    fn default() -> Self {
        Self {
            audio: AudioSettings {
                backend: "JACK".to_string(),
                sample_rate: 48000,
                buffer_size: 256,
                // 0.0 = engines already matched via the measured SF2_CAL_GAIN
                // (DSP/SF2 RMS ratio 11.71 = 21.4 dB, FluidR3_GM piano C4 vs
                // DSP Init C4, 2026-11-01). Old configs without these fields
                // get the same defaults via serde → also matched.
                dsp_trim_db: 0.0,
                sf2_trim_db: 0.0,
            },
            midi: MidiSettings { port_name: None },
            cc_map: None,
            sf2: Sf2Settings::default(),
            ui: UiSettings {
                window_width: 720.0,
                window_height: 400.0,
                maximized: false,
                last_preset: None,
                collapsed_categories: Vec::new(),
                master_volume: 0.8,
                master_tone: 20000.0,
                fader_reverb: 0.0,
                fader_delay: 0.0,
                pitch_bend_range: 2,
                drum_volume: 0.8,
                drum_fixed_velocity: false,
                drum_metronome: false,
                sustain_release: None,
                sf2_release_scale: 1.0,
                layer_a_volume: 0.8,
                layer_b_volume: 0.5,
                pad_perf_map: Vec::new(),
                split_point: None,
                last_performance: None,
                led_companion: false,
                led_companion_cmd: default_led_companion_cmd(),
            parts_state: Vec::new(),
                keybinds: std::collections::HashMap::new(),
                show_telemetry: false,
                dock_bottom_unit: true,
                inst_browser_pos: None,
            },
        }
    }
}

/// Base config directory (~/.config/mini_midi_synth or Android internal).
pub fn app_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "android")]
    {
        ANDROID_DATA_DIR.get().cloned()
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::config_dir().map(|d| d.join("mini_midi_synth"))
    }
}

/// Base data directory (~/.local/share/mini_midi_synth or Android internal).
pub fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        ANDROID_DATA_DIR.get().cloned()
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(target_os = "android"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mini_midi_synth")
    }
}

fn config_path() -> Option<PathBuf> {
    app_config_dir().map(|d| d.join("config.json"))
}

/// Writes `contents` to `path` atomically: a crash mid-write leaves either the
/// old file or the new one, never a truncated one. Android kills backgrounded
/// processes without warning, which makes this more than theoretical.
pub fn write_atomic(path: &std::path::Path, contents: &str) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, contents)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

impl Config {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };

        match serde_json::from_str::<Config>(&contents) {
            Ok(mut config) => {
                // Legacy root_only → note_mode read-side map (TC3).
                for st in &mut config.ui.parts_state { st.migrate_legacy_root_only(); }
                config
            }
            Err(e) => {
                // Keep the unparseable file. Autosave would overwrite it two
                // seconds later and the user's settings would be gone for good.
                let backup = path.with_extension("json.bad");
                let _ = fs::write(&backup, &contents);
                log::warn!(
                    "[config] {} is not valid JSON ({e}); starting from defaults, original kept at {}",
                    path.display(),
                    backup.display()
                );
                eprintln!(
                    "[config] Could not parse {}: {e}\n[config] Original saved as {}",
                    path.display(),
                    backup.display()
                );
                Self::default()
            }
        }
    }

    pub fn save(&self) -> Result<()> {
        let Some(path) = config_path() else {
            anyhow::bail!("Could not determine config directory");
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        write_atomic(&path, &json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_with_looper_sync_bpm_key_parses() {
        // TC17: pre-S5 config files carry the removed key; serde must ignore it.
        let json = r#"{
            "audio": { "backend": "JACK", "sample_rate": 48000, "buffer_size": 256 },
            "midi": { "port_name": null },
            "ui": { "window_width": 1280.0, "window_height": 800.0, "last_preset": null, "looper_sync_bpm": false }
        }"#;
        let cfg: Config = serde_json::from_str(json).expect("legacy config must parse");
        assert_eq!(cfg.audio.sample_rate, 48000);
    }

    #[test]
    fn legacy_root_only_migrates_to_bass() {
        // TC3: старый конфиг с root_only: true (без note_mode) → Bass;
        // root_only: false → Poly. Новый конфиг пишет только note_mode.
        let legacy_on: PartUiState =
            serde_json::from_str(r#"{"root_only": true}"#).expect("legacy part state must parse");
        let mut st = legacy_on;
        st.migrate_legacy_root_only();
        assert_eq!(st.note_mode, crate::synth::NOTE_MODE_BASS, "root_only:true must map to Bass");

        let mut off: PartUiState =
            serde_json::from_str(r#"{"root_only": false}"#).expect("legacy part state must parse");
        off.migrate_legacy_root_only();
        assert_eq!(off.note_mode, crate::synth::NOTE_MODE_POLY, "root_only:false must stay Poly");

        // Round-trip: новый файл не содержит legacy-поля.
        let saved = serde_json::to_string(&st).expect("serialize");
        assert!(!saved.contains("root_only"), "legacy field must not be written: {saved}");
        assert!(saved.contains(r#""note_mode":1"#), "Bass mode must persist: {saved}");
    }
}
