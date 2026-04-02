/// Preset loading/saving with serde + JSON.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    pub params: BTreeMap<String, f32>,
}

/// Embedded factory presets.
const FACTORY_PRESETS: &[(&str, &str)] = &[
    ("init", include_str!("../presets/init.json")),
    ("saw_lead", include_str!("../presets/saw_lead.json")),
    ("warm_pad", include_str!("../presets/warm_pad.json")),
    ("fm_bell", include_str!("../presets/fm_bell.json")),
    // Bass
    ("sub_bass", include_str!("../presets/sub_bass.json")),
    ("acid_bass", include_str!("../presets/acid_bass.json")),
    ("pluck_bass", include_str!("../presets/pluck_bass.json")),
    // Lead
    ("mono_lead", include_str!("../presets/mono_lead.json")),
    ("detuned_lead", include_str!("../presets/detuned_lead.json")),
    ("screaming_lead", include_str!("../presets/screaming_lead.json")),
    // Pad
    ("ambient_pad", include_str!("../presets/ambient_pad.json")),
    ("string_pad", include_str!("../presets/string_pad.json")),
    ("dark_pad", include_str!("../presets/dark_pad.json")),
    // Keys
    ("electric_piano", include_str!("../presets/electric_piano.json")),
    ("organ", include_str!("../presets/organ.json")),
    ("bell_chime", include_str!("../presets/bell_chime.json")),
    // FX
    ("noise_sweep", include_str!("../presets/noise_sweep.json")),
    ("riser", include_str!("../presets/riser.json")),
    ("wobble", include_str!("../presets/wobble.json")),
];

/// Return user preset directory (~/.config/mini_midi_synth/presets/).
fn user_preset_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("presets"))
}

/// Load all available presets: factory + user.
pub fn load_all_presets() -> Vec<Preset> {
    let mut presets = Vec::new();

    // Factory presets
    for (_, json) in FACTORY_PRESETS {
        if let Ok(p) = serde_json::from_str::<Preset>(json) {
            presets.push(p);
        }
    }

    // User presets
    if let Some(dir) = user_preset_dir() {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "json") {
                        if let Ok(contents) = fs::read_to_string(&path) {
                            if let Ok(p) = serde_json::from_str::<Preset>(&contents) {
                                presets.push(p);
                            }
                        }
                    }
                }
            }
        }
    }

    presets
}

/// Save a preset to user directory.
#[allow(dead_code)]
pub fn save_preset(preset: &Preset) -> Result<PathBuf> {
    let dir = user_preset_dir().context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let filename = preset.name.to_lowercase().replace(' ', "_") + ".json";
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(preset)?;
    fs::write(&path, json)?;
    Ok(path)
}
