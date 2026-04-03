/// Persistent settings: audio backend, buffer size, sample rate, MIDI port.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    /// Path to the SF2 file for keys (layers A/B).
    #[serde(default)]
    pub keys_file_path: Option<String>,
    /// Path to the SF2 file for drums.
    #[serde(default)]
    pub drums_file_path: Option<String>,
    /// Layer A uses SF2 mode.
    #[serde(default)]
    pub layer_a_sf2: bool,
    /// GM program number for layer A.
    #[serde(default)]
    pub layer_a_program: u8,
    /// Layer B uses SF2 mode.
    #[serde(default)]
    pub layer_b_sf2: bool,
    /// GM program number for layer B.
    #[serde(default)]
    pub layer_b_program: u8,
    /// Drums use SF2 mode.
    #[serde(default)]
    pub drums_sf2: bool,
    /// SF2 internal block size (samples). Higher = less CPU, more latency.
    #[serde(default = "default_sf2_block_size")]
    pub block_size: usize,
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
}

fn default_master_volume() -> f32 { 0.8 }
fn default_master_tone() -> f32 { 20000.0 }

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
            },
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("config.json"))
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
