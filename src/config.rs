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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiSettings {
    pub port_name: Option<String>,
}

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
    #[serde(default = "default_layer_a_volume")]
    pub layer_a_volume: f32,
    #[serde(default = "default_layer_b_volume")]
    pub layer_b_volume: f32,
    /// Pad performance map: 16 slots (notes 36-51) → performance name.
    #[serde(default)]
    pub pad_perf_map: Vec<Option<String>>,
    /// Sync looper BPM with drum sequencer BPM.
    #[serde(default = "default_looper_sync_bpm")]
    pub looper_sync_bpm: bool,
    /// Keybindings: egui key name → action string.
    #[serde(default)]
    pub keybinds: std::collections::HashMap<String, String>,
}

fn default_looper_sync_bpm() -> bool { true }

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
                layer_a_volume: 0.8,
                layer_b_volume: 0.5,
                pad_perf_map: Vec::new(),
                looper_sync_bpm: true,
                keybinds: std::collections::HashMap::new(),
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

impl Config {
    pub fn load() -> Self {
        let Some(path) = config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    #[allow(dead_code)]
    pub fn save(&self) -> Result<()> {
        let Some(path) = config_path() else {
            anyhow::bail!("Could not determine config directory");
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        Ok(())
    }
}
