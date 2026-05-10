/// Patch loading/saving with serde + JSON.

use anyhow::{Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Generic JSON save/load/list helpers
// ---------------------------------------------------------------------------

fn save_json<T: Serialize>(obj: &T, dir: Option<PathBuf>, name: &str) -> Result<PathBuf> {
    let dir = dir.context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let filename = name.to_lowercase().replace(' ', "_") + ".json";
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(obj)?;
    fs::write(&path, json)?;
    Ok(path)
}

fn load_json<T: DeserializeOwned>(path: &std::path::Path, label: &str) -> Result<T> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {label} file"))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {label} JSON"))
}

fn list_json<T: DeserializeOwned>(dir: Option<PathBuf>, get_name: fn(&T) -> String) -> Vec<(String, PathBuf)> {
    let mut items = Vec::new();
    if let Some(dir) = dir {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "json") {
                        if let Ok(contents) = fs::read_to_string(&path) {
                            if let Ok(obj) = serde_json::from_str::<T>(&contents) {
                                items.push((get_name(&obj), path));
                            }
                        }
                    }
                }
            }
        }
    }
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
}

use crate::synth::drum::{DrumPattern, DrumSlotParams};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub name: String,
    #[serde(default)]
    pub category: String,
    pub params: BTreeMap<String, f32>,
    /// Path to .wt file relative to wavetable dir (e.g. "Basic/Sine.wt").
    /// Set in converted Surge patches that use wavetable oscillator.
    #[serde(default)]
    pub wavetable_file: Option<String>,
    /// Parsed wavetable data (loaded at runtime, not serialized).
    #[serde(skip)]
    pub wavetable_data: Option<Vec<f32>>,
    /// Frames in the wavetable (loaded at runtime).
    #[serde(skip)]
    pub wavetable_frames: usize,
    /// Samples per frame (loaded at runtime).
    #[serde(skip)]
    pub wavetable_frame_size: usize,
}

/// Parse a Surge .wt wavetable file. Returns (samples, frame_count, frame_size).
pub fn parse_wt(data: &[u8]) -> Option<(Vec<f32>, usize, usize)> {
    if data.len() < 12 { return None; }
    // Magic 'vawt' (big-endian text, rest little-endian)
    if &data[0..4] != b"vawt" { return None; }
    let wave_size = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    let wave_count = u16::from_le_bytes(data[8..10].try_into().ok()?) as usize;
    let flags = u16::from_le_bytes(data[10..12].try_into().ok()?);
    let is_int16 = (flags & 0x0004) != 0;
    let full_range = (flags & 0x0008) != 0;

    let expected = if is_int16 { 2 } else { 4 } * wave_size * wave_count;
    if data.len() < 12 + expected { return None; }

    let payload = &data[12..];
    let mut samples = Vec::with_capacity(wave_size * wave_count);

    if is_int16 {
        let scale = if full_range { 32768.0_f32 } else { 16384.0_f32 };
        for i in 0..wave_size * wave_count {
            let v = i16::from_le_bytes(payload[i*2..i*2+2].try_into().ok()?) as f32 / scale;
            samples.push(v);
        }
    } else {
        for i in 0..wave_size * wave_count {
            let v = f32::from_le_bytes(payload[i*4..i*4+4].try_into().ok()?);
            samples.push(v);
        }
    }

    Some((samples, wave_count, wave_size))
}

/// Embedded factory patches: (category, id, json_content).
const FACTORY_PRESETS: &[(&str, &str, &str)] = &[
    ("General", "init", include_str!("../presets/init.json")),
    // Piano
    ("Piano", "grand_piano", include_str!("../presets/grand_piano.json")),
    ("Piano", "warm_upright", include_str!("../presets/warm_upright.json")),
    ("Piano", "bright_grand", include_str!("../presets/bright_grand.json")),
    ("Piano", "honky_tonk", include_str!("../presets/honky_tonk.json")),
    ("Piano", "electric_piano", include_str!("../presets/electric_piano.json")),
    ("Piano", "fm_piano", include_str!("../presets/fm_piano.json")),
    ("Piano", "fm_piano_v2", include_str!("../presets/fm_piano_v2.json")),
    ("Piano", "lofi_keys", include_str!("../presets/lofi_keys.json")),
    // Physical piano model (new algorithm — brightness-controlled, not velocity)
    ("Piano", "grand_piano_v2", include_str!("../presets/grand_piano_v2.json")),
    ("Piano", "warm_upright_v2", include_str!("../presets/warm_upright_v2.json")),
    ("Piano", "bright_studio_piano", include_str!("../presets/bright_studio_piano.json")),
    ("Piano", "soft_piano", include_str!("../presets/soft_piano.json")),
    // Electric Piano physical model (Rhodes/Wurlitzer/Stage73)
    ("Piano", "rhodes_piano", include_str!("../presets/rhodes_piano.json")),
    ("Piano", "wurlitzer", include_str!("../presets/wurlitzer.json")),
    ("Piano", "stage73", include_str!("../presets/stage73.json")),
    ("Piano", "rhodes_piano_v2", include_str!("../presets/rhodes_piano_v2.json")),
    ("Piano", "stage73_v2",      include_str!("../presets/stage73_v2.json")),
    ("Piano", "wurlitzer_v2",    include_str!("../presets/wurlitzer_v2.json")),
    ("Piano", "soft_piano_v2",   include_str!("../presets/soft_piano_v2.json")),
    // Electric Piano with rotary speaker
    ("Piano", "rhodes_piano_v3", include_str!("../presets/rhodes_piano_v3.json")),
    ("Piano", "stage73_v3",      include_str!("../presets/stage73_v3.json")),
    ("Piano", "wurlitzer_v3",    include_str!("../presets/wurlitzer_v3.json")),
    ("Piano", "electric_piano_v2", include_str!("../presets/electric_piano_v2.json")),
    // Piano with convolution reverb / tape effects
    ("Piano", "grand_piano_v3",  include_str!("../presets/grand_piano_v3.json")),
    ("Piano", "soft_piano_v3",   include_str!("../presets/soft_piano_v3.json")),
    ("Piano", "convolution_grand", include_str!("../presets/convolution_grand.json")),
    ("Piano", "tape_echo_keys",  include_str!("../presets/tape_echo_keys.json")),
    ("Piano", "wurlitzer_tape",  include_str!("../presets/wurlitzer_tape.json")),
    // Organ
    ("Organ", "organ_classic", include_str!("../presets/organ_classic.json")),
    ("Organ", "organ_classic_v2", include_str!("../presets/organ_classic_v2.json")),
    ("Organ", "hammond_b3", include_str!("../presets/hammond_b3.json")),
    ("Organ", "hammond_b3_v2", include_str!("../presets/hammond_b3_v2.json")),
    ("Organ", "pipe_organ", include_str!("../presets/pipe_organ.json")),
    ("Organ", "pipe_organ_v2", include_str!("../presets/pipe_organ_v2.json")),
    ("Organ", "farfisa", include_str!("../presets/farfisa.json")),
    ("Organ", "synth_organ", include_str!("../presets/organ.json")),
    // Mallet / Percussion
    ("Mallet", "twist_modal_bell", include_str!("../presets/twist_modal.json")),
    ("Mallet", "twist_string",     include_str!("../presets/twist_string.json")),
    ("Mallet", "vibraphone", include_str!("../presets/vibraphone.json")),
    ("Mallet", "glockenspiel", include_str!("../presets/glockenspiel.json")),
    ("Mallet", "fm_bell", include_str!("../presets/fm_bell.json")),
    ("Mallet", "fm_bell_v2",     include_str!("../presets/fm_bell_v2.json")),
    ("Mallet", "fm_bell_v3",     include_str!("../presets/fm_bell_v3.json")),
    ("Mallet", "fm_bell_v4",     include_str!("../presets/fm_bell_v4.json")),
    ("Mallet", "vibraphone_v2",  include_str!("../presets/vibraphone_v2.json")),
    ("Mallet", "bell_chime", include_str!("../presets/bell_chime.json")),
    ("Mallet", "music_box", include_str!("../presets/music_box.json")),
    ("Mallet", "steel_drum", include_str!("../presets/steel_drum.json")),
    ("Mallet", "steel_drum_v2",   include_str!("../presets/steel_drum_v2.json")),
    // Plucked / Strings
    ("Strings", "nylon_guitar", include_str!("../presets/nylon_guitar.json")),
    ("Strings", "nylon_guitar_v2", include_str!("../presets/nylon_guitar_v2.json")),
    ("Strings", "clean_guitar", include_str!("../presets/clean_guitar.json")),
    ("Strings", "harp", include_str!("../presets/harp.json")),
    ("Strings", "kalimba", include_str!("../presets/kalimba.json")),
    ("Strings", "clavinet", include_str!("../presets/clavinet.json")),
    ("Strings", "clavinet_v2",   include_str!("../presets/clavinet_v2.json")),
    ("Strings", "vinyl_strings", include_str!("../presets/vinyl_strings.json")),
    // Woodwind
    ("Wind", "flute", include_str!("../presets/flute.json")),
    ("Wind", "flute_v2",     include_str!("../presets/flute_v2.json")),
    ("Wind", "clarinet", include_str!("../presets/clarinet.json")),
    ("Wind", "shakuhachi", include_str!("../presets/shakuhachi.json")),
    ("Wind", "shakuhachi_v2", include_str!("../presets/shakuhachi_v2.json")),
    // Strings (Bowed)
    ("Strings (Bowed)", "violin", include_str!("../presets/violin.json")),
    ("Strings (Bowed)", "violin_v2", include_str!("../presets/violin_v2.json")),
    ("Strings (Bowed)", "viola", include_str!("../presets/viola.json")),
    ("Strings (Bowed)", "cello", include_str!("../presets/cello.json")),
    ("Strings (Bowed)", "cello_v2",  include_str!("../presets/cello_v2.json")),
    ("Strings (Bowed)", "double_bass", include_str!("../presets/double_bass.json")),
    ("Strings (Bowed)", "double_bass_v2",  include_str!("../presets/double_bass_v2.json")),
    // Brass
    ("Brass", "synth_brass", include_str!("../presets/synth_brass.json")),
    ("Brass", "trumpet", include_str!("../presets/trumpet.json")),
    ("Brass", "trumpet_v2", include_str!("../presets/trumpet_v2.json")),
    ("Brass", "french_horn", include_str!("../presets/french_horn.json")),
    ("Brass", "trombone", include_str!("../presets/trombone.json")),
    ("Brass", "trombone_v2", include_str!("../presets/trombone_v2.json")),
    ("Brass", "tuba", include_str!("../presets/tuba.json")),
    ("Brass", "tuba_v2",         include_str!("../presets/tuba_v2.json")),
    ("Brass", "french_horn_v2",  include_str!("../presets/french_horn_v2.json")),
    // Bass
    ("Bass", "sub_bass", include_str!("../presets/sub_bass.json")),
    ("Bass", "sub_bass_v2",      include_str!("../presets/sub_bass_v2.json")),
    ("Bass", "acid_bass", include_str!("../presets/acid_bass.json")),
    ("Bass", "acid_bass_v2",     include_str!("../presets/acid_bass_v2.json")),
    ("Bass", "pluck_bass", include_str!("../presets/pluck_bass.json")),
    ("Bass", "reese_bass", include_str!("../presets/reese_bass.json")),
    ("Bass", "reese_bass_v2",    include_str!("../presets/reese_bass_v2.json")),
    ("Bass", "reese_bass_v3",    include_str!("../presets/reese_bass_v3.json")),
    ("Bass", "fm_bass", include_str!("../presets/fm_bass.json")),
    ("Bass", "wobble_bass", include_str!("../presets/wobble_bass.json")),
    ("Bass", "fuzz_bass", include_str!("../presets/fuzz_bass.json")),
    ("Bass", "pressure_bass", include_str!("../presets/pressure_bass.json")),
    ("Bass", "aw_iron_tape_bass", include_str!("../presets/aw_iron_tape_bass.json")),
    // Bass Guitar
    ("Bass Guitar", "bass_finger", include_str!("../presets/bass_finger.json")),
    ("Bass Guitar", "bass_finger_v2",  include_str!("../presets/bass_finger_v2.json")),
    ("Bass Guitar", "bass_finger_v3",  include_str!("../presets/bass_finger_v3.json")),
    ("Bass Guitar", "bass_pick", include_str!("../presets/bass_pick.json")),
    ("Bass Guitar", "bass_slap", include_str!("../presets/bass_slap.json")),
    // Lead
    ("Lead", "saw_lead", include_str!("../presets/saw_lead.json")),
    ("Lead", "mono_lead", include_str!("../presets/mono_lead.json")),
    ("Lead", "detuned_lead", include_str!("../presets/detuned_lead.json")),
    ("Lead", "screaming_lead", include_str!("../presets/screaming_lead.json")),
    ("Lead", "screaming_lead_v2", include_str!("../presets/screaming_lead_v2.json")),
    ("Lead", "supersaw_lead", include_str!("../presets/supersaw_lead.json")),
    ("Lead", "trance_lead", include_str!("../presets/trance_lead.json")),
    ("Lead", "hoover_lead", include_str!("../presets/hoover_lead.json")),
    ("Lead", "hoover_lead_v2",   include_str!("../presets/hoover_lead_v2.json")),
    ("Lead", "treemonster_lead", include_str!("../presets/treemonster_lead.json")),
    ("Lead", "vintage_ladder_lead", include_str!("../presets/vintage_ladder_lead.json")),
    ("Lead", "westcoast_lead", include_str!("../presets/westcoast_lead.json")),
    ("Lead", "aw_hardvac_lead", include_str!("../presets/aw_hardvac_lead.json")),
    ("Lead", "twist_wavetable", include_str!("../presets/twist_wavetable.json")),
    ("Lead", "polivoks_lead", include_str!("../presets/polivoks_lead.json")),
    ("Lead", "mick_gordon_doom", include_str!("../presets/mick_gordon_doom.json")),
    // Pad
    ("Pad", "warm_pad", include_str!("../presets/warm_pad.json")),
    ("Pad", "warm_pad_v2",       include_str!("../presets/warm_pad_v2.json")),
    ("Pad", "ambient_pad", include_str!("../presets/ambient_pad.json")),
    ("Pad", "ambient_pad_v2",    include_str!("../presets/ambient_pad_v2.json")),
    ("Pad", "ambient_pad_v3",    include_str!("../presets/ambient_pad_v3.json")),
    ("Pad", "string_pad", include_str!("../presets/string_pad.json")),
    ("Pad", "string_pad_v2",     include_str!("../presets/string_pad_v2.json")),
    ("Pad", "string_pad_v3",     include_str!("../presets/string_pad_v3.json")),
    ("Pad", "dark_pad", include_str!("../presets/dark_pad.json")),
    ("Pad", "dark_pad_v2",       include_str!("../presets/dark_pad_v2.json")),
    ("Pad", "chorus_pad", include_str!("../presets/chorus_pad.json")),
    ("Pad", "ethereal_pad", include_str!("../presets/ethereal_pad.json")),
    ("Pad", "ethereal_pad_v2",   include_str!("../presets/ethereal_pad_v2.json")),
    ("Pad", "granular_drone",    include_str!("../presets/granular_drone.json")),
    ("Pad", "lofi_digital_pad",  include_str!("../presets/lofi_digital_pad.json")),
    ("Pad", "lofi_tape_pad",     include_str!("../presets/lofi_tape_pad.json")),
    ("Pad", "svf_morph_sweep",   include_str!("../presets/svf_morph_sweep.json")),
    ("Pad", "harmonic_shimmer_pad", include_str!("../presets/harmonic_shimmer_pad.json")),
    ("Pad", "aw_galactic_pad",   include_str!("../presets/aw_galactic_pad.json")),
    ("Pad", "aw_melt_chorus",    include_str!("../presets/aw_melt_chorus.json")),
    ("Pad", "twist_grain",       include_str!("../presets/twist_grain.json")),
    ("Pad", "twist_chords",      include_str!("../presets/twist_chords.json")),
    ("Pad", "twist_vowels",      include_str!("../presets/twist_vowels.json")),
    // FX / Stab
    ("FX", "synth_stab", include_str!("../presets/synth_stab.json")),
    ("FX", "noise_sweep", include_str!("../presets/noise_sweep.json")),
    ("FX", "riser", include_str!("../presets/riser.json")),
    ("FX", "wobble", include_str!("../presets/wobble.json")),
    ("FX", "wind", include_str!("../presets/wind.json")),
    // Drums / Percussion
    ("Drums", "aw_pressure_drums", include_str!("../presets/aw_pressure_drums.json")),
    ("Drums", "taiko", include_str!("../presets/taiko.json")),
    ("Drums", "timpani", include_str!("../presets/timpani.json")),
    ("Drums", "bass_drum", include_str!("../presets/bass_drum.json")),
    ("Drums", "bass_drum_v2",    include_str!("../presets/bass_drum_v2.json")),
    ("Drums", "gong_crash", include_str!("../presets/gong_crash.json")),
    ("Drums", "gong_crash_v2",   include_str!("../presets/gong_crash_v2.json")),
    ("Drums", "snare", include_str!("../presets/snare.json")),
    ("Drums", "metal_kick", include_str!("../presets/metal_kick.json")),
    ("Drums", "metal_snare", include_str!("../presets/metal_snare.json")),
    ("Drums", "metal_snare_crack", include_str!("../presets/metal_snare_crack.json")),
    // Cinematic
    ("Cinematic", "braam", include_str!("../presets/braam.json")),
    ("Cinematic", "whoosh", include_str!("../presets/whoosh.json")),
    ("Cinematic", "hit_impact", include_str!("../presets/hit_impact.json")),
    ("Cinematic", "boom", include_str!("../presets/boom.json")),
    ("Cinematic", "downer", include_str!("../presets/downer.json")),
    ("Cinematic", "stinger", include_str!("../presets/stinger.json")),
    // Vocal / Choir
    ("Vocal", "male_choir", include_str!("../presets/male_choir.json")),
    ("Vocal", "male_choir_v2",   include_str!("../presets/male_choir_v2.json")),
    ("Vocal", "male_choir_v3",   include_str!("../presets/male_choir_v3.json")),
    ("Vocal", "female_choir", include_str!("../presets/female_choir.json")),
    ("Vocal", "female_choir_v2", include_str!("../presets/female_choir_v2.json")),
    ("Vocal", "female_choir_v3", include_str!("../presets/female_choir_v3.json")),
    ("Vocal", "robot_choir",     include_str!("../presets/robot_choir.json")),
    ("Vocal", "vocal_shouts", include_str!("../presets/vocal_shouts.json")),
    // Phase Distortion
    ("Phase Distortion", "cz_reso_lead", include_str!("../presets/cz_reso_lead.json")),
    ("Phase Distortion", "cz_bass", include_str!("../presets/cz_bass.json")),
    // Wavefolder
    ("Wavefolder", "wavefold_lead", include_str!("../presets/wavefold_lead.json")),
    ("Wavefolder", "harsh_fold", include_str!("../presets/harsh_fold.json")),
    ("Wavefolder", "harsh_fold_v2",        include_str!("../presets/harsh_fold_v2.json")),
    ("Wavefolder", "dirty_saw", include_str!("../presets/dirty_saw.json")),
    ("Wavefolder", "crushed_lead", include_str!("../presets/crushed_lead.json")),
    ("Wavefolder", "crushed_lead_v2",      include_str!("../presets/crushed_lead_v2.json")),
    ("Wavefolder", "distorted_square", include_str!("../presets/distorted_square.json")),
    ("Wavefolder", "distorted_square_v2",  include_str!("../presets/distorted_square_v2.json")),
    // Modal
    ("Modal", "vibraphone_modal", include_str!("../presets/vibraphone_modal.json")),
    ("Modal", "church_bell_modal", include_str!("../presets/church_bell_modal.json")),
    ("Modal", "church_bell_modal_v2",  include_str!("../presets/church_bell_modal_v2.json")),
    ("Modal", "church_bell_v3",        include_str!("../presets/church_bell_v3.json")),
    ("Modal", "church_bell_v4",        include_str!("../presets/church_bell_v4.json")),
    ("Modal", "marimba_modal", include_str!("../presets/marimba_modal.json")),
    ("Modal", "marimba_v2",            include_str!("../presets/marimba_v2.json")),
    ("Modal", "glass_modal", include_str!("../presets/glass_modal.json")),
    ("Modal", "metallic_bell_comb",    include_str!("../presets/metallic_bell_comb.json")),
    // Moog
    ("Moog", "moog_bass", include_str!("../presets/moog_bass.json")),
    ("Moog", "moog_bass_v2",     include_str!("../presets/moog_bass_v2.json")),
    ("Moog", "moog_lead", include_str!("../presets/moog_lead.json")),
    ("Moog", "moog_lead_v2",          include_str!("../presets/moog_lead_v2.json")),
    ("Moog", "moog_lead_v3",          include_str!("../presets/moog_lead_v3.json")),
    ("Moog", "moog_squelch", include_str!("../presets/moog_squelch.json")),
    // OB-Xd filters
    ("OB-Xd", "obxd_pad",            include_str!("../presets/obxd_pad.json")),
    ("OB-Xd", "obxd_lead",           include_str!("../presets/obxd_lead.json")),
    ("OB-Xd", "obxd_brass",          include_str!("../presets/obxd_brass.json")),
    ("OB-Xd", "obxd_strings",        include_str!("../presets/obxd_strings.json")),
    ("OB-Xd", "obxd_warm_strings",   include_str!("../presets/obxd_warm_strings.json")),
    ("OB-Xd", "obxd_brass_punch",    include_str!("../presets/obxd_brass_punch.json")),
    ("OB-Xd", "obxd2_bright_lead",   include_str!("../presets/obxd2_bright_lead.json")),
    // Tripole 18dB
    ("Tripole", "tripole_bass", include_str!("../presets/tripole_bass.json")),
    ("Tripole", "tripole_lead",         include_str!("../presets/tripole_lead.json")),
    ("Tripole", "tripole_wobble_bass",  include_str!("../presets/tripole_wobble_bass.json")),
    // Sample & Hold filter
    ("S&H", "snh_digital", include_str!("../presets/snh_digital.json")),
    ("S&H", "snh_robot",   include_str!("../presets/snh_robot.json")),
    // Warp filters (cutoff saturation)
    ("Warp", "warp_acid",    include_str!("../presets/warp_acid.json")),
    ("Warp", "warp_pad",     include_str!("../presets/warp_pad.json")),
    ("Warp", "warpbp_wah",          include_str!("../presets/warpbp_wah.json")),
    ("Warp", "cutwarp_acid_smooth", include_str!("../presets/cutwarp_acid_smooth.json")),
    // ResWarp filters (resonance saturation)
    ("ResWarp", "reswarp_lead", include_str!("../presets/reswarp_lead.json")),
    ("ResWarp", "reswarp_bass",    include_str!("../presets/reswarp_bass.json")),
    ("ResWarp", "reswarp_melody",  include_str!("../presets/reswarp_melody.json")),
    // BP24 / Notch24 spectral
    ("Spectral", "bp24_vocal", include_str!("../presets/bp24_vocal.json")),
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
    ("Iconic", "tb303_acid_v2", include_str!("../presets/tb303_acid_v2.json")),
    ("Iconic", "tb303_acid_v3",  include_str!("../presets/tb303_acid_v3.json")),
    ("Iconic", "tb303_acid_v4",  include_str!("../presets/tb303_acid_v4.json")),
    ("Iconic", "tb303_square_v2", include_str!("../presets/tb303_square_v2.json")),
    ("Iconic", "jump_brass", include_str!("../presets/jump_brass.json")),
    ("Iconic", "blade_runner", include_str!("../presets/blade_runner.json")),
    ("Iconic", "blade_runner_v2", include_str!("../presets/blade_runner_v2.json")),
    ("Iconic", "blade_runner_v3", include_str!("../presets/blade_runner_v3.json")),
    ("Iconic", "juno_pad", include_str!("../presets/juno_pad.json")),
    ("Iconic", "juno_pad_v2",    include_str!("../presets/juno_pad_v2.json")),
    ("Iconic", "juno_pad_v3",    include_str!("../presets/juno_pad_v3.json")),
    ("Iconic", "prophet_brass", include_str!("../presets/prophet_brass.json")),
    ("Iconic", "cs80_strings", include_str!("../presets/cs80_strings.json")),
    ("Iconic", "cs80_strings_v2", include_str!("../presets/cs80_strings_v2.json")),
    ("Iconic", "cs80_strings_v3", include_str!("../presets/cs80_strings_v3.json")),
    ("Iconic", "odyssey_lead", include_str!("../presets/odyssey_lead.json")),
    ("Iconic", "the_sync", include_str!("../presets/the_sync.json")),
    ("Iconic", "fairlight_arr1", include_str!("../presets/fairlight_arr1.json")),
    ("Iconic", "synth_brass_80s", include_str!("../presets/synth_brass_80s.json")),
    ("Iconic", "dx7_epiano_v2", include_str!("../presets/dx7_epiano_v2.json")),
    ("Iconic", "dx7_epiano_v3", include_str!("../presets/dx7_epiano_v3.json")),
    ("Iconic", "house_piano_m1", include_str!("../presets/house_piano_m1.json")),
    ("Iconic", "dx7_bass", include_str!("../presets/dx7_bass.json")),
    ("Iconic", "dx7_bass_v2", include_str!("../presets/dx7_bass_v2.json")),
    ("Iconic", "dx7_tubular_bell", include_str!("../presets/dx7_tubular_bell.json")),
    ("Iconic", "dx7_tubular_bell_v2", include_str!("../presets/dx7_tubular_bell_v2.json")),
    ("Iconic", "d50_pizzagogo", include_str!("../presets/d50_pizzagogo.json")),
    ("Iconic", "minimoog_lead", include_str!("../presets/minimoog_lead.json")),
    ("Iconic", "dx7_epiano", include_str!("../presets/dx7_epiano.json")),
    ("Iconic", "trevor_horn_is_my_mother_110bpm", include_str!("../presets/trevor_horn_is_my_mother_110bpm.json")),
    ("Iconic", "trevor_horn_is_my_mother_melody", include_str!("../presets/trevor_horn_is_my_mother_melody.json")),
    // Scooter style
    ("Scooter", "scooter_hyper_lead", include_str!("../presets/scooter_hyper_lead.json")),
    ("Scooter", "scooter_hoover", include_str!("../presets/scooter_hoover.json")),
    ("Scooter", "scooter_hard_stab", include_str!("../presets/scooter_hard_stab.json")),
    ("Scooter", "scooter_hard_bass", include_str!("../presets/scooter_hard_bass.json")),
    ("Scooter", "scooter_rave_stab", include_str!("../presets/scooter_rave_stab.json")),
    ("Scooter", "scooter_supersaw_lead", include_str!("../presets/scooter_supersaw_lead.json")),
    ("Scooter", "scooter_rave_pad", include_str!("../presets/scooter_rave_pad.json")),
    ("Scooter", "scooter_arpegg", include_str!("../presets/scooter_arpegg.json")),
    // Accordion
    ("Accordion", "accordion", include_str!("../presets/accordion.json")),
    ("Accordion", "accordion_musette", include_str!("../presets/accordion_musette.json")),
    ("Accordion", "accordion_master", include_str!("../presets/accordion_master.json")),
    ("Accordion", "harmonica", include_str!("../presets/harmonica.json")),
    ("Accordion", "blues_harp", include_str!("../presets/blues_harp.json")),
    // Saxophone
    ("Saxophone", "soprano_sax", include_str!("../presets/soprano_sax.json")),
    ("Saxophone", "alto_sax", include_str!("../presets/alto_sax.json")),
    ("Saxophone", "tenor_sax", include_str!("../presets/tenor_sax.json")),
    ("Saxophone", "bari_sax", include_str!("../presets/bari_sax.json")),
    ("Saxophone", "soprano_sax_v2", include_str!("../presets/soprano_sax_v2.json")),
    ("Saxophone", "soprano_sax_v3", include_str!("../presets/soprano_sax_v3.json")),
    ("Saxophone", "alto_sax_v2",    include_str!("../presets/alto_sax_v2.json")),
    ("Saxophone", "tenor_sax_v2",   include_str!("../presets/tenor_sax_v2.json")),
    ("Saxophone", "bari_sax_v2",    include_str!("../presets/bari_sax_v2.json")),
    // Sequences (ported from Surge XT)
    // Surge XT: Bass
    // Surge XT: Brass
    // Surge XT: Chords
    // Surge XT: Experimental
    // Surge XT: Keys
    // Surge XT: Leads
    // Surge XT: Pads
    // Surge XT: Percussion
    // Surge XT: Plucks
    // Surge XT: Polysynths
    // Surge XT: Sequences
    // Surge XT: Winds
];

/// Return user patch directory (~/.config/mini_midi_synth/presets/).
fn user_preset_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("presets"))
}

/// Load all presets from a directory recursively (subdirs = categories).
/// Also loads .wt wavetable data if patch references one.
fn load_presets_from_dir(dir: &PathBuf, presets: &mut Vec<Patch>) {
    if !dir.exists() { return; }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                load_presets_from_dir(&path, presets);
            } else if path.extension().is_some_and(|e| e == "json") {
                if let Ok(contents) = fs::read_to_string(&path) {
                    if let Ok(mut p) = serde_json::from_str::<Patch>(&contents) {
                        if p.category.is_empty() {
                            if let Some(cat) = path.parent()
                                .and_then(|p| p.file_name())
                                .and_then(|n| n.to_str()) {
                                p.category = cat.to_string();
                            }
                        }
                        // Load .wt if referenced
                        if let Some(ref wt_rel) = p.wavetable_file.clone() {
                            let wt_path = wavetable_dir().join(wt_rel);
                            if let Ok(bytes) = fs::read(&wt_path) {
                                if let Some((data, frames, frame_size)) = parse_wt(&bytes) {
                                    p.wavetable_data = Some(data);
                                    p.wavetable_frames = frames;
                                    p.wavetable_frame_size = frame_size;
                                }
                            }
                        }
                        presets.push(p);
                    }
                }
            }
        }
    }
}

/// External patch directory: ~/.local/share/mini_midi_synth/presets/
/// Place converted Surge presets here.
pub fn external_preset_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mini_midi_synth")
        .join("presets")
}

/// Wavetable directory: ~/.local/share/mini_midi_synth/wavetables/
pub fn wavetable_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mini_midi_synth")
        .join("wavetables")
}

/// Load all available presets: factory + external + user.
pub fn load_all_patches() -> Vec<Patch> {
    let mut presets = Vec::new();

    // Factory presets (compiled in)
    for (category, _, json) in FACTORY_PRESETS {
        if let Ok(mut p) = serde_json::from_str::<Patch>(json) {
            p.category = category.to_string();
            presets.push(p);
        }
    }

    // External presets (~/.local/share/mini_midi_synth/presets/)
    // Converted Surge presets and other third-party presets go here.
    load_presets_from_dir(&external_preset_dir(), &mut presets);

    // User presets (~/.config/mini_midi_synth/presets/)
    if let Some(dir) = user_preset_dir() {
        load_presets_from_dir(&dir, &mut presets);
    }

    presets
}

/// Save a patch to user directory.
#[allow(dead_code)]
pub fn save_patch(preset: &Patch) -> Result<PathBuf> {
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
    save_json(kit, drum_kit_dir(), &kit.name)
}

pub fn load_drum_kit(path: &std::path::Path) -> Result<DrumKit> {
    load_json(path, "drum kit")
}

pub fn list_drum_kits() -> Vec<(String, PathBuf)> {
    list_json(drum_kit_dir(), |k: &DrumKit| k.name.clone())
}

// ---------------------------------------------------------------------------
// MIDI drum import
// ---------------------------------------------------------------------------

/// Import drum patterns from a standard MIDI file (.mid).
/// Extracts notes on channel 10 (GM drums), quantizes to 16-step grid,
/// splits into bars (up to 8 patterns).
pub fn import_midi_drums(path: &std::path::Path) -> Result<(Vec<DrumPattern>, f32)> {
    use crate::synth::drum::{DRUM_NOTE_BASE, NUM_DRUM_SLOTS};

    let data = fs::read(path).context("Failed to read MIDI file")?;
    let smf = midly::Smf::parse(&data).map_err(|e| anyhow::anyhow!("MIDI parse error: {e}"))?;

    let ppq = match smf.header.timing {
        midly::Timing::Metrical(tpb) => tpb.as_int() as u32,
        midly::Timing::Timecode(fps, sub) => {
            // Approximate: treat as if metrical
            (fps.as_int() as u32) * (sub as u32)
        }
    };
    if ppq == 0 {
        anyhow::bail!("Invalid MIDI timing (PPQ=0)");
    }

    // 16 steps per bar in 4/4 = 16th notes
    let ticks_per_step = ppq * 4 / 16; // ppq * 4 quarter notes / 16 steps

    // Extract tempo (first tempo event, default 120 BPM)
    let mut bpm: f32 = 120.0;
    for track in &smf.tracks {
        for event in track {
            if let midly::TrackEventKind::Meta(midly::MetaMessage::Tempo(t)) = event.kind {
                bpm = 60_000_000.0 / t.as_int() as f32;
                break;
            }
        }
        if (bpm - 120.0).abs() > 0.01 { break; }
    }

    // GM drum note range we support: 36-51 (slots 0-15)
    // Also map note 35 (Acoustic Bass Drum) → slot 0 (Kick)
    let map_note = |note: u8| -> Option<usize> {
        if note == 35 { return Some(0); } // Bass drum → kick
        if note >= DRUM_NOTE_BASE && note < DRUM_NOTE_BASE + NUM_DRUM_SLOTS as u8 {
            Some((note - DRUM_NOTE_BASE) as usize)
        } else {
            None // outside our range
        }
    };

    // Collect all (absolute_step, slot, velocity) hits across all tracks
    let mut hits: Vec<(u32, usize, u8)> = Vec::new();

    for track in &smf.tracks {
        let mut abs_tick: u64 = 0;
        for event in track {
            abs_tick += event.delta.as_int() as u64;
            if let midly::TrackEventKind::Midi { channel, message } = event.kind {
                // Channel 9 = GM drums (0-indexed)
                if channel.as_int() != 9 { continue; }
                let (note, vel) = match message {
                    midly::MidiMessage::NoteOn { key, vel } => {
                        (key.as_int(), vel.as_int())
                    }
                    _ => continue,
                };
                if vel == 0 { continue; } // NoteOn vel=0 = NoteOff
                if let Some(slot) = map_note(note) {
                    let step = ((abs_tick as f64 / ticks_per_step as f64).round()) as u32;
                    hits.push((step, slot, vel));
                }
            }
        }
    }

    if hits.is_empty() {
        anyhow::bail!("No drum notes found on channel 10");
    }

    // Find total steps and split into bars of 16
    let max_step = hits.iter().map(|(s, _, _)| *s).max().unwrap_or(0);
    let num_bars = ((max_step / 16) + 1).min(8) as usize; // cap at 8 patterns

    let mut patterns: Vec<DrumPattern> = vec![DrumPattern::default(); num_bars];

    for (step, slot, vel) in &hits {
        let bar = (*step / 16) as usize;
        let step_in_bar = (*step % 16) as usize;
        if bar < num_bars && step_in_bar < 16 && *slot < NUM_DRUM_SLOTS {
            // Take highest velocity if multiple hits on same step
            let existing = patterns[bar].steps[*slot][step_in_bar].velocity;
            if *vel > existing {
                patterns[bar].steps[*slot][step_in_bar].velocity = *vel;
            }
        }
    }

    Ok((patterns, bpm))
}

// ---------------------------------------------------------------------------
// Performance patches (split/part combos)
// ---------------------------------------------------------------------------

/// One part in a performance — references a preset by name + overrides.
#[derive(Clone, Serialize, Deserialize)]
pub struct PartConfig {
    pub patch_name: String,
    pub enabled: bool,
    pub volume: f32,
    pub key_low: u8,
    pub key_high: u8,
    #[serde(default)]
    pub param_overrides: BTreeMap<String, f32>,
    /// If true, this part uses SF2 soundfont instead of DSP patch.
    #[serde(default)]
    pub sf2_mode: bool,
    /// SF2 program number (0-127).
    #[serde(default)]
    pub sf2_program: u8,
}

/// A saved split/part configuration.
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
    save_json(perf, performance_dir(), &perf.name)
}

pub fn load_performance(path: &std::path::Path) -> Result<Performance> {
    load_json(path, "performance")
}

pub fn list_performances() -> Vec<(String, PathBuf)> {
    list_json(performance_dir(), |p: &Performance| p.name.clone())
}

