/// Preset loading/saving with serde + JSON.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::synth::drum::{DrumPattern, DrumSlotParams};

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
    // Physical piano model (new algorithm — brightness-controlled, not velocity)
    ("Piano", "grand_piano_v2", include_str!("../presets/grand_piano_v2.json")),
    ("Piano", "warm_upright_v2", include_str!("../presets/warm_upright_v2.json")),
    ("Piano", "bright_studio_piano", include_str!("../presets/bright_studio_piano.json")),
    ("Piano", "soft_piano", include_str!("../presets/soft_piano.json")),
    // Organ
    ("Organ", "organ_classic", include_str!("../presets/organ_classic.json")),
    ("Organ", "hammond_b3", include_str!("../presets/hammond_b3.json")),
    ("Organ", "pipe_organ", include_str!("../presets/pipe_organ.json")),
    ("Organ", "farfisa", include_str!("../presets/farfisa.json")),
    ("Organ", "synth_organ", include_str!("../presets/organ.json")),
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
    // Strings (Bowed)
    ("Strings (Bowed)", "violin", include_str!("../presets/violin.json")),
    ("Strings (Bowed)", "viola", include_str!("../presets/viola.json")),
    ("Strings (Bowed)", "cello", include_str!("../presets/cello.json")),
    ("Strings (Bowed)", "double_bass", include_str!("../presets/double_bass.json")),
    // Brass
    ("Brass", "synth_brass", include_str!("../presets/synth_brass.json")),
    ("Brass", "trumpet", include_str!("../presets/trumpet.json")),
    ("Brass", "french_horn", include_str!("../presets/french_horn.json")),
    ("Brass", "trombone", include_str!("../presets/trombone.json")),
    ("Brass", "tuba", include_str!("../presets/tuba.json")),
    // Bass
    ("Bass", "sub_bass", include_str!("../presets/sub_bass.json")),
    ("Bass", "acid_bass", include_str!("../presets/acid_bass.json")),
    ("Bass", "pluck_bass", include_str!("../presets/pluck_bass.json")),
    ("Bass", "reese_bass", include_str!("../presets/reese_bass.json")),
    ("Bass", "fm_bass", include_str!("../presets/fm_bass.json")),
    ("Bass", "wobble_bass", include_str!("../presets/wobble_bass.json")),
    // Bass Guitar
    ("Bass Guitar", "bass_finger", include_str!("../presets/bass_finger.json")),
    ("Bass Guitar", "bass_pick", include_str!("../presets/bass_pick.json")),
    ("Bass Guitar", "bass_slap", include_str!("../presets/bass_slap.json")),
    // Lead
    ("Lead", "saw_lead", include_str!("../presets/saw_lead.json")),
    ("Lead", "mono_lead", include_str!("../presets/mono_lead.json")),
    ("Lead", "detuned_lead", include_str!("../presets/detuned_lead.json")),
    ("Lead", "screaming_lead", include_str!("../presets/screaming_lead.json")),
    ("Lead", "supersaw_lead", include_str!("../presets/supersaw_lead.json")),
    ("Lead", "trance_lead", include_str!("../presets/trance_lead.json")),
    ("Lead", "hoover_lead", include_str!("../presets/hoover_lead.json")),
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
    ("Drums", "snare", include_str!("../presets/snare.json")),
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
    // Phase Distortion
    ("Phase Distortion", "cz_reso_lead", include_str!("../presets/cz_reso_lead.json")),
    ("Phase Distortion", "cz_bass", include_str!("../presets/cz_bass.json")),
    // Wavefolder
    ("Wavefolder", "wavefold_lead", include_str!("../presets/wavefold_lead.json")),
    ("Wavefolder", "harsh_fold", include_str!("../presets/harsh_fold.json")),
    // Modal
    ("Modal", "vibraphone_modal", include_str!("../presets/vibraphone_modal.json")),
    ("Modal", "church_bell_modal", include_str!("../presets/church_bell_modal.json")),
    ("Modal", "marimba_modal", include_str!("../presets/marimba_modal.json")),
    ("Modal", "glass_modal", include_str!("../presets/glass_modal.json")),
    // Moog
    ("Moog", "moog_bass", include_str!("../presets/moog_bass.json")),
    ("Moog", "moog_lead", include_str!("../presets/moog_lead.json")),
    ("Moog", "moog_squelch", include_str!("../presets/moog_squelch.json")),
    // Hard Sync
    ("Hard Sync", "sync_lead", include_str!("../presets/sync_lead.json")),
    ("Hard Sync", "sync_brass", include_str!("../presets/sync_brass.json")),
    // Supersaw
    ("Supersaw", "jp_supersaw", include_str!("../presets/jp_supersaw.json")),
    ("Supersaw", "supersaw_pad", include_str!("../presets/supersaw_pad.json")),
    ("Supersaw", "supersaw_trance", include_str!("../presets/supersaw_trance.json")),
    // Iconic
    ("Iconic", "tb303_acid", include_str!("../presets/tb303_acid.json")),
    ("Iconic", "tb303_square", include_str!("../presets/tb303_square.json")),
    ("Iconic", "jump_brass", include_str!("../presets/jump_brass.json")),
    ("Iconic", "blade_runner", include_str!("../presets/blade_runner.json")),
    ("Iconic", "juno_pad", include_str!("../presets/juno_pad.json")),
    ("Iconic", "prophet_brass", include_str!("../presets/prophet_brass.json")),
    ("Iconic", "cs80_strings", include_str!("../presets/cs80_strings.json")),
    ("Iconic", "odyssey_lead", include_str!("../presets/odyssey_lead.json")),
    ("Iconic", "minimoog_lead", include_str!("../presets/minimoog_lead.json")),
    ("Iconic", "dx7_epiano", include_str!("../presets/dx7_epiano.json")),
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

// ---------------------------------------------------------------------------
// Drum kit presets (patterns + params)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct DrumKit {
    pub name: String,
    pub patterns: Vec<DrumPattern>,
    pub params: Vec<DrumSlotParams>,
    pub bpm: f32,
    pub swing: f32,
    pub volume: f32,
}

fn drum_kit_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("drum_kits"))
}

pub fn save_drum_kit(kit: &DrumKit) -> Result<PathBuf> {
    let dir = drum_kit_dir().context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let filename = kit.name.to_lowercase().replace(' ', "_") + ".json";
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(kit)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_drum_kit(path: &std::path::Path) -> Result<DrumKit> {
    let contents = fs::read_to_string(path).context("Failed to read drum kit file")?;
    let kit: DrumKit = serde_json::from_str(&contents).context("Failed to parse drum kit JSON")?;
    Ok(kit)
}

pub fn list_drum_kits() -> Vec<(String, PathBuf)> {
    let mut kits = Vec::new();
    if let Some(dir) = drum_kit_dir() {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "json") {
                        if let Ok(contents) = fs::read_to_string(&path) {
                            if let Ok(kit) = serde_json::from_str::<DrumKit>(&contents) {
                                kits.push((kit.name, path));
                            }
                        }
                    }
                }
            }
        }
    }
    kits.sort_by(|a, b| a.0.cmp(&b.0));
    kits
}

// ---------------------------------------------------------------------------
// Performance presets (split/layer combos)
// ---------------------------------------------------------------------------

/// One part in a performance — references a preset by name + overrides.
#[derive(Clone, Serialize, Deserialize)]
pub struct PartConfig {
    pub preset_name: String,
    pub enabled: bool,
    pub volume: f32,
    pub key_low: u8,
    pub key_high: u8,
    #[serde(default)]
    pub param_overrides: BTreeMap<String, f32>,
}

/// A saved split/layer configuration.
#[derive(Clone, Serialize, Deserialize)]
pub struct Performance {
    pub name: String,
    #[serde(default)]
    pub category: String,
    pub parts: Vec<PartConfig>,
}

fn performance_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("performances"))
}

pub fn save_performance(perf: &Performance) -> Result<PathBuf> {
    let dir = performance_dir().context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let filename = perf.name.to_lowercase().replace(' ', "_") + ".json";
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(perf)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_performance(path: &std::path::Path) -> Result<Performance> {
    let contents = fs::read_to_string(path).context("Failed to read performance file")?;
    let perf: Performance = serde_json::from_str(&contents).context("Failed to parse performance JSON")?;
    Ok(perf)
}

pub fn list_performances() -> Vec<(String, PathBuf)> {
    let mut perfs = Vec::new();
    if let Some(dir) = performance_dir() {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "json") {
                        if let Ok(contents) = fs::read_to_string(&path) {
                            if let Ok(p) = serde_json::from_str::<Performance>(&contents) {
                                perfs.push((p.name, path));
                            }
                        }
                    }
                }
            }
        }
    }
    perfs.sort_by(|a, b| a.0.cmp(&b.0));
    perfs
}

// ---------------------------------------------------------------------------
// Set lists (ordered performance sequences)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct SetList {
    pub name: String,
    pub entries: Vec<String>, // performance names, in order
}

fn setlist_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("setlists"))
}

pub fn save_setlist(setlist: &SetList) -> Result<PathBuf> {
    let dir = setlist_dir().context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let filename = setlist.name.to_lowercase().replace(' ', "_") + ".json";
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(setlist)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_setlist(path: &std::path::Path) -> Result<SetList> {
    let contents = fs::read_to_string(path).context("Failed to read setlist file")?;
    let sl: SetList = serde_json::from_str(&contents).context("Failed to parse setlist JSON")?;
    Ok(sl)
}

pub fn list_setlists() -> Vec<(String, PathBuf)> {
    let mut lists = Vec::new();
    if let Some(dir) = setlist_dir() {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "json") {
                        if let Ok(contents) = fs::read_to_string(&path) {
                            if let Ok(sl) = serde_json::from_str::<SetList>(&contents) {
                                lists.push((sl.name, path));
                            }
                        }
                    }
                }
            }
        }
    }
    lists.sort_by(|a, b| a.0.cmp(&b.0));
    lists
}
