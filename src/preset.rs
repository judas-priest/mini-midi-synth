/// Preset loading/saving with serde + JSON.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub name: String,
    #[serde(default)]
    pub category: String,
    pub params: BTreeMap<String, f32>,
}

/// Embedded factory presets: (category, id, json_content).
const FACTORY_PRESETS: &[(&str, &str, &str)] = &[
    ("General", "init", include_str!("../presets/init.json")),
    // Piano
    ("Piano", "grand_piano", include_str!("../presets/grand_piano.json")),
    ("Piano", "warm_upright", include_str!("../presets/warm_upright.json")),
    ("Piano", "bright_grand", include_str!("../presets/bright_grand.json")),
    ("Piano", "honky_tonk", include_str!("../presets/honky_tonk.json")),
    ("Piano", "electric_piano", include_str!("../presets/electric_piano.json")),
    ("Piano", "fm_piano", include_str!("../presets/fm_piano.json")),
    ("Piano", "lofi_keys", include_str!("../presets/lofi_keys.json")),
    // Organ
    ("Organ", "organ_classic", include_str!("../presets/organ_classic.json")),
    ("Organ", "hammond_b3", include_str!("../presets/hammond_b3.json")),
    ("Organ", "pipe_organ", include_str!("../presets/pipe_organ.json")),
    ("Organ", "farfisa", include_str!("../presets/farfisa.json")),
    ("Organ", "square_organ", include_str!("../presets/organ.json")),
    // Mallet / Percussion
    ("Mallet", "vibraphone", include_str!("../presets/vibraphone.json")),
    ("Mallet", "glockenspiel", include_str!("../presets/glockenspiel.json")),
    ("Mallet", "fm_bell", include_str!("../presets/fm_bell.json")),
    ("Mallet", "bell_chime", include_str!("../presets/bell_chime.json")),
    ("Mallet", "music_box", include_str!("../presets/music_box.json")),
    ("Mallet", "steel_drum", include_str!("../presets/steel_drum.json")),
    // Plucked / Strings
    ("Strings", "nylon_guitar", include_str!("../presets/nylon_guitar.json")),
    ("Strings", "clean_guitar", include_str!("../presets/clean_guitar.json")),
    ("Strings", "harp", include_str!("../presets/harp.json")),
    ("Strings", "kalimba", include_str!("../presets/kalimba.json")),
    ("Strings", "clavinet", include_str!("../presets/clavinet.json")),
    // Woodwind
    ("Wind", "flute", include_str!("../presets/flute.json")),
    ("Wind", "clarinet", include_str!("../presets/clarinet.json")),
    // Brass
    ("Brass", "synth_brass", include_str!("../presets/synth_brass.json")),
    // Bass
    ("Bass", "sub_bass", include_str!("../presets/sub_bass.json")),
    ("Bass", "acid_bass", include_str!("../presets/acid_bass.json")),
    ("Bass", "pluck_bass", include_str!("../presets/pluck_bass.json")),
    // Lead
    ("Lead", "saw_lead", include_str!("../presets/saw_lead.json")),
    ("Lead", "mono_lead", include_str!("../presets/mono_lead.json")),
    ("Lead", "detuned_lead", include_str!("../presets/detuned_lead.json")),
    ("Lead", "screaming_lead", include_str!("../presets/screaming_lead.json")),
    // Pad
    ("Pad", "warm_pad", include_str!("../presets/warm_pad.json")),
    ("Pad", "ambient_pad", include_str!("../presets/ambient_pad.json")),
    ("Pad", "string_pad", include_str!("../presets/string_pad.json")),
    ("Pad", "dark_pad", include_str!("../presets/dark_pad.json")),
    ("Pad", "chorus_pad", include_str!("../presets/chorus_pad.json")),
    ("Pad", "ethereal_pad", include_str!("../presets/ethereal_pad.json")),
    // FX / Stab
    ("FX", "synth_stab", include_str!("../presets/synth_stab.json")),
    ("FX", "noise_sweep", include_str!("../presets/noise_sweep.json")),
    ("FX", "riser", include_str!("../presets/riser.json")),
    ("FX", "wobble", include_str!("../presets/wobble.json")),
    ("FX", "wind", include_str!("../presets/wind.json")),
    // Drums / Percussion
    ("Drums", "taiko", include_str!("../presets/taiko.json")),
    ("Drums", "timpani", include_str!("../presets/timpani.json")),
    ("Drums", "bass_drum", include_str!("../presets/bass_drum.json")),
    ("Drums", "gong_crash", include_str!("../presets/gong_crash.json")),
    // Cinematic
    ("Cinematic", "braam", include_str!("../presets/braam.json")),
    ("Cinematic", "whoosh", include_str!("../presets/whoosh.json")),
    ("Cinematic", "hit_impact", include_str!("../presets/hit_impact.json")),
    ("Cinematic", "boom", include_str!("../presets/boom.json")),
    ("Cinematic", "downer", include_str!("../presets/downer.json")),
    ("Cinematic", "stinger", include_str!("../presets/stinger.json")),
    // Vocal / Choir
    ("Vocal", "male_choir", include_str!("../presets/male_choir.json")),
    ("Vocal", "female_choir", include_str!("../presets/female_choir.json")),
    ("Vocal", "vocal_shouts", include_str!("../presets/vocal_shouts.json")),
];

/// Return user preset directory (~/.config/mini_midi_synth/presets/).
fn user_preset_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("presets"))
}

/// Load all available presets: factory + user.
pub fn load_all_presets() -> Vec<Preset> {
    let mut presets = Vec::new();

    // Factory presets
    for (category, _, json) in FACTORY_PRESETS {
        if let Ok(mut p) = serde_json::from_str::<Preset>(json) {
            p.category = category.to_string();
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
                            if let Ok(mut p) = serde_json::from_str::<Preset>(&contents) {
                                if p.category.is_empty() {
                                    p.category = "User".to_string();
                                }
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
