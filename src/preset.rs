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
    ("Sequences", "surge_seq_acid_seq_1",  include_str!("../presets/surge_seq_acid_seq_1.json")),
    ("Sequences", "surge_seq_acid_seq_2",  include_str!("../presets/surge_seq_acid_seq_2.json")),
    ("Sequences", "surge_seq_acid_seq_3",  include_str!("../presets/surge_seq_acid_seq_3.json")),
    ("Sequences", "surge_seq_calm",        include_str!("../presets/surge_seq_calm.json")),
    ("Sequences", "surge_seq_game_on_1",   include_str!("../presets/surge_seq_game_on_1.json")),
    ("Sequences", "surge_seq_game_on_2",   include_str!("../presets/surge_seq_game_on_2.json")),
    ("Sequences", "surge_seq_game_on_3",   include_str!("../presets/surge_seq_game_on_3.json")),
    ("Sequences", "surge_seq_madness",     include_str!("../presets/surge_seq_madness.json")),
    ("Sequences", "surge_seq_noisebass",   include_str!("../presets/surge_seq_noisebass.json")),
    ("Sequences", "surge_seq_octave_arp",  include_str!("../presets/surge_seq_octave_arp.json")),
    ("Sequences", "surge_seq_phase_2",     include_str!("../presets/surge_seq_phase_2.json")),
    ("Sequences", "surge_seq_sync_arp",    include_str!("../presets/surge_seq_sync_arp.json")),
    ("Sequences", "surge_seq_barrelish", include_str!("../presets/surge_seq_barrelish.json")),
    ("Sequences", "surge_seq_bell_seq", include_str!("../presets/surge_seq_bell_seq.json")),
    ("Sequences", "surge_seq_bit_seq", include_str!("../presets/surge_seq_bit_seq.json")),
    ("Sequences", "surge_seq_burial_ground", include_str!("../presets/surge_seq_burial_ground.json")),
    ("Sequences", "surge_seq_damn_aliens", include_str!("../presets/surge_seq_damn_aliens.json")),
    ("Sequences", "surge_seq_distorted_glassyish_seq", include_str!("../presets/surge_seq_distorted_glassyish_seq.json")),
    ("Sequences", "surge_seq_fb_seq", include_str!("../presets/surge_seq_fb_seq.json")),
    ("Sequences", "surge_seq_fifthseq", include_str!("../presets/surge_seq_fifthseq.json")),
    ("Sequences", "surge_seq_filter_buildup", include_str!("../presets/surge_seq_filter_buildup.json")),
    ("Sequences", "surge_seq_fm_motion_sweep", include_str!("../presets/surge_seq_fm_motion_sweep.json")),
    ("Sequences", "surge_seq_fm_seq", include_str!("../presets/surge_seq_fm_seq.json")),
    ("Sequences", "surge_seq_foldseq", include_str!("../presets/surge_seq_foldseq.json")),
    ("Sequences", "surge_seq_gatechord", include_str!("../presets/surge_seq_gatechord.json")),
    ("Sequences", "surge_seq_hovercar_championship", include_str!("../presets/surge_seq_hovercar_championship.json")),
    ("Sequences", "surge_seq_i_want_to_get_well", include_str!("../presets/surge_seq_i_want_to_get_well.json")),
    ("Sequences", "surge_seq_multiseq_01", include_str!("../presets/surge_seq_multiseq_01.json")),
    ("Sequences", "surge_seq_noise_seq_01", include_str!("../presets/surge_seq_noise_seq_01.json")),
    ("Sequences", "surge_seq_one_key_wonder", include_str!("../presets/surge_seq_one_key_wonder.json")),
    ("Sequences", "surge_seq_phase_3", include_str!("../presets/surge_seq_phase_3.json")),
    ("Sequences", "surge_seq_retrig_me", include_str!("../presets/surge_seq_retrig_me.json")),
    ("Sequences", "surge_seq_sin_sequencer_1", include_str!("../presets/surge_seq_sin_sequencer_1.json")),
    ("Sequences", "surge_seq_sq_1234", include_str!("../presets/surge_seq_sq_1234.json")),
    ("Sequences", "surge_seq_stepphaser", include_str!("../presets/surge_seq_stepphaser.json")),
    ("Sequences", "surge_seq_sync_acident", include_str!("../presets/surge_seq_sync_acident.json")),
    ("Sequences", "surge_seq_tyskland", include_str!("../presets/surge_seq_tyskland.json")),
    ("Sequences", "surge_seq_wabbit_mw", include_str!("../presets/surge_seq_wabbit_mw.json")),
    ("Sequences", "surge_seq_waveseq_01", include_str!("../presets/surge_seq_waveseq_01.json")),
    ("Sequences", "surge_seq_when_good_combs_go_bad", include_str!("../presets/surge_seq_when_good_combs_go_bad.json")),
    // Surge XT: Bass
    ("Bass", "surge_basses_attacky", include_str!("../presets/surge_basses_attacky.json")),
    ("Bass", "surge_basses_bass_10", include_str!("../presets/surge_basses_bass_10.json")),
    ("Bass", "surge_basses_bass_four", include_str!("../presets/surge_basses_bass_four.json")),
    ("Bass", "surge_basses_bass_one", include_str!("../presets/surge_basses_bass_one.json")),
    ("Bass", "surge_basses_bass_three", include_str!("../presets/surge_basses_bass_three.json")),
    ("Bass", "surge_basses_bass_two", include_str!("../presets/surge_basses_bass_two.json")),
    ("Bass", "surge_basses_bassline_wide", include_str!("../presets/surge_basses_bassline_wide.json")),
    ("Bass", "surge_basses_behemoth", include_str!("../presets/surge_basses_behemoth.json")),
    ("Bass", "surge_basses_crush_bass", include_str!("../presets/surge_basses_crush_bass.json")),
    ("Bass", "surge_basses_deep_end", include_str!("../presets/surge_basses_deep_end.json")),
    ("Bass", "surge_basses_digibass", include_str!("../presets/surge_basses_digibass.json")),
    ("Bass", "surge_basses_distbass", include_str!("../presets/surge_basses_distbass.json")),
    ("Bass", "surge_basses_distbass_2", include_str!("../presets/surge_basses_distbass_2.json")),
    ("Bass", "surge_basses_distorted_bass_01", include_str!("../presets/surge_basses_distorted_bass_01.json")),
    ("Bass", "surge_basses_distorted_fm", include_str!("../presets/surge_basses_distorted_fm.json")),
    ("Bass", "surge_basses_distorted_mw", include_str!("../presets/surge_basses_distorted_mw.json")),
    ("Bass", "surge_basses_doomsday", include_str!("../presets/surge_basses_doomsday.json")),
    ("Bass", "surge_basses_e_bass", include_str!("../presets/surge_basses_e_bass.json")),
    ("Bass", "surge_basses_eighties_drone", include_str!("../presets/surge_basses_eighties_drone.json")),
    ("Bass", "surge_basses_evilous", include_str!("../presets/surge_basses_evilous.json")),
    ("Bass", "surge_basses_fingered", include_str!("../presets/surge_basses_fingered.json")),
    ("Bass", "surge_basses_fm_bass_1", include_str!("../presets/surge_basses_fm_bass_1.json")),
    ("Bass", "surge_basses_fm_bass_2", include_str!("../presets/surge_basses_fm_bass_2.json")),
    ("Bass", "surge_basses_fm_bass_3", include_str!("../presets/surge_basses_fm_bass_3.json")),
    ("Bass", "surge_basses_fm_bass_4", include_str!("../presets/surge_basses_fm_bass_4.json")),
    ("Bass", "surge_basses_fm_bass_5", include_str!("../presets/surge_basses_fm_bass_5.json")),
    ("Bass", "surge_basses_fm_bass_6", include_str!("../presets/surge_basses_fm_bass_6.json")),
    ("Bass", "surge_basses_fm_combo", include_str!("../presets/surge_basses_fm_combo.json")),
    ("Bass", "surge_basses_fm_slap_1", include_str!("../presets/surge_basses_fm_slap_1.json")),
    ("Bass", "surge_basses_helmeto", include_str!("../presets/surge_basses_helmeto.json")),
    ("Bass", "surge_basses_lord_sawtooth", include_str!("../presets/surge_basses_lord_sawtooth.json")),
    ("Bass", "surge_basses_mellow", include_str!("../presets/surge_basses_mellow.json")),
    ("Bass", "surge_basses_metalsquare", include_str!("../presets/surge_basses_metalsquare.json")),
    ("Bass", "surge_basses_mmm_pointy", include_str!("../presets/surge_basses_mmm_pointy.json")),
    ("Bass", "surge_basses_pianobass", include_str!("../presets/surge_basses_pianobass.json")),
    ("Bass", "surge_basses_plain", include_str!("../presets/surge_basses_plain.json")),
    ("Bass", "surge_basses_ring_mayhem", include_str!("../presets/surge_basses_ring_mayhem.json")),
    ("Bass", "surge_basses_rubber_bass", include_str!("../presets/surge_basses_rubber_bass.json")),
    ("Bass", "surge_basses_rumble", include_str!("../presets/surge_basses_rumble.json")),
    ("Bass", "surge_basses_saw_lofi", include_str!("../presets/surge_basses_saw_lofi.json")),
    ("Bass", "surge_basses_schnell", include_str!("../presets/surge_basses_schnell.json")),
    ("Bass", "surge_basses_slow", include_str!("../presets/surge_basses_slow.json")),
    ("Bass", "surge_basses_smootie", include_str!("../presets/surge_basses_smootie.json")),
    ("Bass", "surge_basses_square_bass", include_str!("../presets/surge_basses_square_bass.json")),
    ("Bass", "surge_basses_squared", include_str!("../presets/surge_basses_squared.json")),
    ("Bass", "surge_basses_stable", include_str!("../presets/surge_basses_stable.json")),
    ("Bass", "surge_basses_static_1", include_str!("../presets/surge_basses_static_1.json")),
    ("Bass", "surge_basses_static_2", include_str!("../presets/surge_basses_static_2.json")),
    ("Bass", "surge_basses_stone", include_str!("../presets/surge_basses_stone.json")),
    ("Bass", "surge_basses_sub_1", include_str!("../presets/surge_basses_sub_1.json")),
    ("Bass", "surge_basses_sub_2", include_str!("../presets/surge_basses_sub_2.json")),
    ("Bass", "surge_basses_sub_3", include_str!("../presets/surge_basses_sub_3.json")),
    ("Bass", "surge_basses_sub_4", include_str!("../presets/surge_basses_sub_4.json")),
    ("Bass", "surge_basses_sub_square_1", include_str!("../presets/surge_basses_sub_square_1.json")),
    ("Bass", "surge_basses_tacky_1", include_str!("../presets/surge_basses_tacky_1.json")),
    ("Bass", "surge_basses_tacky_2", include_str!("../presets/surge_basses_tacky_2.json")),
    ("Bass", "surge_basses_theme", include_str!("../presets/surge_basses_theme.json")),
    ("Bass", "surge_basses_width", include_str!("../presets/surge_basses_width.json")),
    ("Bass", "surge_basses_wtbass", include_str!("../presets/surge_basses_wtbass.json")),
    // Surge XT: Brass
    ("Brass", "surge_brass_buggy_brass", include_str!("../presets/surge_brass_buggy_brass.json")),
    ("Brass", "surge_brass_crisp_noise_brass", include_str!("../presets/surge_brass_crisp_noise_brass.json")),
    ("Brass", "surge_brass_plasticbrass", include_str!("../presets/surge_brass_plasticbrass.json")),
    ("Brass", "surge_brass_sawteeth", include_str!("../presets/surge_brass_sawteeth.json")),
    ("Brass", "surge_brass_synth_brass_1", include_str!("../presets/surge_brass_synth_brass_1.json")),
    ("Brass", "surge_brass_synth_brass_2", include_str!("../presets/surge_brass_synth_brass_2.json")),
    ("Brass", "surge_brass_synth_brass_3", include_str!("../presets/surge_brass_synth_brass_3.json")),
    // Surge XT: Chords
    ("Chords", "surge_chords_inharmonic_stab", include_str!("../presets/surge_chords_inharmonic_stab.json")),
    ("Chords", "surge_chords_maj_min_saw", include_str!("../presets/surge_chords_maj_min_saw.json")),
    ("Chords", "surge_chords_maj_min_stab", include_str!("../presets/surge_chords_maj_min_stab.json")),
    ("Chords", "surge_chords_major_7", include_str!("../presets/surge_chords_major_7.json")),
    ("Chords", "surge_chords_major_7_mk2", include_str!("../presets/surge_chords_major_7_mk2.json")),
    ("Chords", "surge_chords_minor_7", include_str!("../presets/surge_chords_minor_7.json")),
    ("Chords", "surge_chords_minor_chord_retro_stab", include_str!("../presets/surge_chords_minor_chord_retro_stab.json")),
    ("Chords", "surge_chords_tek_stab", include_str!("../presets/surge_chords_tek_stab.json")),
    // Surge XT: Experimental
    ("Experimental", "surge_fx_aggero", include_str!("../presets/surge_fx_aggero.json")),
    ("Experimental", "surge_fx_alarm", include_str!("../presets/surge_fx_alarm.json")),
    ("Experimental", "surge_fx_aliens", include_str!("../presets/surge_fx_aliens.json")),
    ("Experimental", "surge_fx_bork", include_str!("../presets/surge_fx_bork.json")),
    ("Experimental", "surge_fx_busy", include_str!("../presets/surge_fx_busy.json")),
    ("Experimental", "surge_fx_chaotry", include_str!("../presets/surge_fx_chaotry.json")),
    ("Experimental", "surge_fx_crackling", include_str!("../presets/surge_fx_crackling.json")),
    ("Experimental", "surge_fx_damage_dealer", include_str!("../presets/surge_fx_damage_dealer.json")),
    ("Experimental", "surge_fx_die_ie_ie", include_str!("../presets/surge_fx_die_ie_ie.json")),
    ("Experimental", "surge_fx_dishonest", include_str!("../presets/surge_fx_dishonest.json")),
    ("Experimental", "surge_fx_dtmf", include_str!("../presets/surge_fx_dtmf.json")),
    ("Experimental", "surge_fx_evil_suckerseq", include_str!("../presets/surge_fx_evil_suckerseq.json")),
    ("Experimental", "surge_fx_fireworks", include_str!("../presets/surge_fx_fireworks.json")),
    ("Experimental", "surge_fx_fry_apos_s_holophoner", include_str!("../presets/surge_fx_fry_apos_s_holophoner.json")),
    ("Experimental", "surge_fx_geiger", include_str!("../presets/surge_fx_geiger.json")),
    ("Experimental", "surge_fx_harm", include_str!("../presets/surge_fx_harm.json")),
    ("Experimental", "surge_fx_healthcare", include_str!("../presets/surge_fx_healthcare.json")),
    ("Experimental", "surge_fx_metalpluck", include_str!("../presets/surge_fx_metalpluck.json")),
    ("Experimental", "surge_fx_radio_noise", include_str!("../presets/surge_fx_radio_noise.json")),
    ("Experimental", "surge_fx_rather_low", include_str!("../presets/surge_fx_rather_low.json")),
    ("Experimental", "surge_fx_space_adventure_1", include_str!("../presets/surge_fx_space_adventure_1.json")),
    ("Experimental", "surge_fx_space_adventure_2", include_str!("../presets/surge_fx_space_adventure_2.json")),
    ("Experimental", "surge_fx_space_cadet", include_str!("../presets/surge_fx_space_cadet.json")),
    ("Experimental", "surge_fx_spookyfish", include_str!("../presets/surge_fx_spookyfish.json")),
    ("Experimental", "surge_fx_unsettler", include_str!("../presets/surge_fx_unsettler.json")),
    ("Experimental", "surge_fx_vinyl", include_str!("../presets/surge_fx_vinyl.json")),
    // Surge XT: Keys
    ("Keys", "surge_keys_artificial", include_str!("../presets/surge_keys_artificial.json")),
    ("Keys", "surge_keys_artificial_2", include_str!("../presets/surge_keys_artificial_2.json")),
    ("Keys", "surge_keys_church", include_str!("../presets/surge_keys_church.json")),
    ("Keys", "surge_keys_circus", include_str!("../presets/surge_keys_circus.json")),
    ("Keys", "surge_keys_circus_2", include_str!("../presets/surge_keys_circus_2.json")),
    ("Keys", "surge_keys_dirt", include_str!("../presets/surge_keys_dirt.json")),
    ("Keys", "surge_keys_dx_ep_1", include_str!("../presets/surge_keys_dx_ep_1.json")),
    ("Keys", "surge_keys_ep", include_str!("../presets/surge_keys_ep.json")),
    ("Keys", "surge_keys_ep2", include_str!("../presets/surge_keys_ep2.json")),
    ("Keys", "surge_keys_house_organ", include_str!("../presets/surge_keys_house_organ.json")),
    ("Keys", "surge_keys_organ_1", include_str!("../presets/surge_keys_organ_1.json")),
    ("Keys", "surge_keys_organ_2", include_str!("../presets/surge_keys_organ_2.json")),
    ("Keys", "surge_keys_organ_3", include_str!("../presets/surge_keys_organ_3.json")),
    ("Keys", "surge_keys_sawteeth", include_str!("../presets/surge_keys_sawteeth.json")),
    ("Keys", "surge_keys_softsuit", include_str!("../presets/surge_keys_softsuit.json")),
    // Surge XT: Leads
    ("Leads", "surge_leads_acidofil", include_str!("../presets/surge_leads_acidofil.json")),
    ("Leads", "surge_leads_agroculture", include_str!("../presets/surge_leads_agroculture.json")),
    ("Leads", "surge_leads_asymptote", include_str!("../presets/surge_leads_asymptote.json")),
    ("Leads", "surge_leads_bad_childhood", include_str!("../presets/surge_leads_bad_childhood.json")),
    ("Leads", "surge_leads_banjo_remains", include_str!("../presets/surge_leads_banjo_remains.json")),
    ("Leads", "surge_leads_banter", include_str!("../presets/surge_leads_banter.json")),
    ("Leads", "surge_leads_bassline_tight", include_str!("../presets/surge_leads_bassline_tight.json")),
    ("Leads", "surge_leads_bee", include_str!("../presets/surge_leads_bee.json")),
    ("Leads", "surge_leads_bitten", include_str!("../presets/surge_leads_bitten.json")),
    ("Leads", "surge_leads_boll", include_str!("../presets/surge_leads_boll.json")),
    ("Leads", "surge_leads_broken_one", include_str!("../presets/surge_leads_broken_one.json")),
    ("Leads", "surge_leads_butter", include_str!("../presets/surge_leads_butter.json")),
    ("Leads", "surge_leads_caveman", include_str!("../presets/surge_leads_caveman.json")),
    ("Leads", "surge_leads_cell", include_str!("../presets/surge_leads_cell.json")),
    ("Leads", "surge_leads_chatter", include_str!("../presets/surge_leads_chatter.json")),
    ("Leads", "surge_leads_classic_lead_1", include_str!("../presets/surge_leads_classic_lead_1.json")),
    ("Leads", "surge_leads_classic_lead_2", include_str!("../presets/surge_leads_classic_lead_2.json")),
    ("Leads", "surge_leads_classical", include_str!("../presets/surge_leads_classical.json")),
    ("Leads", "surge_leads_clean_shit", include_str!("../presets/surge_leads_clean_shit.json")),
    ("Leads", "surge_leads_computer", include_str!("../presets/surge_leads_computer.json")),
    ("Leads", "surge_leads_condom", include_str!("../presets/surge_leads_condom.json")),
    ("Leads", "surge_leads_cottage", include_str!("../presets/surge_leads_cottage.json")),
    ("Leads", "surge_leads_cray", include_str!("../presets/surge_leads_cray.json")),
    ("Leads", "surge_leads_crisp_pwm", include_str!("../presets/surge_leads_crisp_pwm.json")),
    ("Leads", "surge_leads_digi_it", include_str!("../presets/surge_leads_digi_it.json")),
    ("Leads", "surge_leads_digi_portalead", include_str!("../presets/surge_leads_digi_portalead.json")),
    ("Leads", "surge_leads_distortionworks", include_str!("../presets/surge_leads_distortionworks.json")),
    ("Leads", "surge_leads_dna_sequencer", include_str!("../presets/surge_leads_dna_sequencer.json")),
    ("Leads", "surge_leads_dome", include_str!("../presets/surge_leads_dome.json")),
    ("Leads", "surge_leads_duck_and_cover", include_str!("../presets/surge_leads_duck_and_cover.json")),
    ("Leads", "surge_leads_eight", include_str!("../presets/surge_leads_eight.json")),
    ("Leads", "surge_leads_etwas", include_str!("../presets/surge_leads_etwas.json")),
    ("Leads", "surge_leads_fairy", include_str!("../presets/surge_leads_fairy.json")),
    ("Leads", "surge_leads_flawed_science", include_str!("../presets/surge_leads_flawed_science.json")),
    ("Leads", "surge_leads_fluff", include_str!("../presets/surge_leads_fluff.json")),
    ("Leads", "surge_leads_fm_is_growing_on_me", include_str!("../presets/surge_leads_fm_is_growing_on_me.json")),
    ("Leads", "surge_leads_fm_rock", include_str!("../presets/surge_leads_fm_rock.json")),
    ("Leads", "surge_leads_formantpulse", include_str!("../presets/surge_leads_formantpulse.json")),
    ("Leads", "surge_leads_fuji", include_str!("../presets/surge_leads_fuji.json")),
    ("Leads", "surge_leads_fundament", include_str!("../presets/surge_leads_fundament.json")),
    ("Leads", "surge_leads_fyllo_dual", include_str!("../presets/surge_leads_fyllo_dual.json")),
    ("Leads", "surge_leads_galliumarsenic", include_str!("../presets/surge_leads_galliumarsenic.json")),
    ("Leads", "surge_leads_generic", include_str!("../presets/surge_leads_generic.json")),
    ("Leads", "surge_leads_glisslead", include_str!("../presets/surge_leads_glisslead.json")),
    ("Leads", "surge_leads_harsh", include_str!("../presets/surge_leads_harsh.json")),
    ("Leads", "surge_leads_harsher", include_str!("../presets/surge_leads_harsher.json")),
    ("Leads", "surge_leads_hippo", include_str!("../presets/surge_leads_hippo.json")),
    ("Leads", "surge_leads_hof", include_str!("../presets/surge_leads_hof.json")),
    ("Leads", "surge_leads_in_the_distance", include_str!("../presets/surge_leads_in_the_distance.json")),
    ("Leads", "surge_leads_kilkenny", include_str!("../presets/surge_leads_kilkenny.json")),
    ("Leads", "surge_leads_koala", include_str!("../presets/surge_leads_koala.json")),
    ("Leads", "surge_leads_koala_2", include_str!("../presets/surge_leads_koala_2.json")),
    ("Leads", "surge_leads_kurasu", include_str!("../presets/surge_leads_kurasu.json")),
    ("Leads", "surge_leads_labcoat", include_str!("../presets/surge_leads_labcoat.json")),
    ("Leads", "surge_leads_later", include_str!("../presets/surge_leads_later.json")),
    ("Leads", "surge_leads_legoland", include_str!("../presets/surge_leads_legoland.json")),
    ("Leads", "surge_leads_lera", include_str!("../presets/surge_leads_lera.json")),
    ("Leads", "surge_leads_light", include_str!("../presets/surge_leads_light.json")),
    ("Leads", "surge_leads_log_log", include_str!("../presets/surge_leads_log_log.json")),
    ("Leads", "surge_leads_longstocking", include_str!("../presets/surge_leads_longstocking.json")),
    ("Leads", "surge_leads_markov", include_str!("../presets/surge_leads_markov.json")),
    ("Leads", "surge_leads_moogy_saw", include_str!("../presets/surge_leads_moogy_saw.json")),
    ("Leads", "surge_leads_mosquito", include_str!("../presets/surge_leads_mosquito.json")),
    ("Leads", "surge_leads_motion", include_str!("../presets/surge_leads_motion.json")),
    ("Leads", "surge_leads_mundane", include_str!("../presets/surge_leads_mundane.json")),
    ("Leads", "surge_leads_nastyfication", include_str!("../presets/surge_leads_nastyfication.json")),
    ("Leads", "surge_leads_not_nearly_as_harsh", include_str!("../presets/surge_leads_not_nearly_as_harsh.json")),
    ("Leads", "surge_leads_octavedodger", include_str!("../presets/surge_leads_octavedodger.json")),
    ("Leads", "surge_leads_oldest_trick_in_the_book_mw", include_str!("../presets/surge_leads_oldest_trick_in_the_book_mw.json")),
    ("Leads", "surge_leads_organ_donor", include_str!("../presets/surge_leads_organ_donor.json")),
    ("Leads", "surge_leads_owl", include_str!("../presets/surge_leads_owl.json")),
    ("Leads", "surge_leads_panda", include_str!("../presets/surge_leads_panda.json")),
    ("Leads", "surge_leads_pet", include_str!("../presets/surge_leads_pet.json")),
    ("Leads", "surge_leads_phasepass", include_str!("../presets/surge_leads_phasepass.json")),
    ("Leads", "surge_leads_photon", include_str!("../presets/surge_leads_photon.json")),
    ("Leads", "surge_leads_play_nice", include_str!("../presets/surge_leads_play_nice.json")),
    ("Leads", "surge_leads_probability", include_str!("../presets/surge_leads_probability.json")),
    ("Leads", "surge_leads_qealchee", include_str!("../presets/surge_leads_qealchee.json")),
    ("Leads", "surge_leads_quickbasic", include_str!("../presets/surge_leads_quickbasic.json")),
    ("Leads", "surge_leads_quirp", include_str!("../presets/surge_leads_quirp.json")),
    ("Leads", "surge_leads_quiz", include_str!("../presets/surge_leads_quiz.json")),
    ("Leads", "surge_leads_radon", include_str!("../presets/surge_leads_radon.json")),
    ("Leads", "surge_leads_resofest_1", include_str!("../presets/surge_leads_resofest_1.json")),
    ("Leads", "surge_leads_resofest_2", include_str!("../presets/surge_leads_resofest_2.json")),
    ("Leads", "surge_leads_resofest_3", include_str!("../presets/surge_leads_resofest_3.json")),
    ("Leads", "surge_leads_resofest_4", include_str!("../presets/surge_leads_resofest_4.json")),
    ("Leads", "surge_leads_riemann", include_str!("../presets/surge_leads_riemann.json")),
    ("Leads", "surge_leads_rough", include_str!("../presets/surge_leads_rough.json")),
    ("Leads", "surge_leads_rundfunk_funk", include_str!("../presets/surge_leads_rundfunk_funk.json")),
    ("Leads", "surge_leads_saw_octaves", include_str!("../presets/surge_leads_saw_octaves.json")),
    ("Leads", "surge_leads_scooped", include_str!("../presets/surge_leads_scooped.json")),
    ("Leads", "surge_leads_screamer", include_str!("../presets/surge_leads_screamer.json")),
    ("Leads", "surge_leads_screamlead", include_str!("../presets/surge_leads_screamlead.json")),
    ("Leads", "surge_leads_screamy_verby", include_str!("../presets/surge_leads_screamy_verby.json")),
    ("Leads", "surge_leads_semiclip", include_str!("../presets/surge_leads_semiclip.json")),
    ("Leads", "surge_leads_serial", include_str!("../presets/surge_leads_serial.json")),
    ("Leads", "surge_leads_shanai", include_str!("../presets/surge_leads_shanai.json")),
    ("Leads", "surge_leads_sharpish", include_str!("../presets/surge_leads_sharpish.json")),
    ("Leads", "surge_leads_sheep_clothing_mw", include_str!("../presets/surge_leads_sheep_clothing_mw.json")),
    ("Leads", "surge_leads_simple_atc", include_str!("../presets/surge_leads_simple_atc.json")),
    ("Leads", "surge_leads_simpler_times", include_str!("../presets/surge_leads_simpler_times.json")),
    ("Leads", "surge_leads_sinesaw_acidish", include_str!("../presets/surge_leads_sinesaw_acidish.json")),
    ("Leads", "surge_leads_sinlead", include_str!("../presets/surge_leads_sinlead.json")),
    ("Leads", "surge_leads_smoothness_world_cup", include_str!("../presets/surge_leads_smoothness_world_cup.json")),
    ("Leads", "surge_leads_smoothy_hollow", include_str!("../presets/surge_leads_smoothy_hollow.json")),
    ("Leads", "surge_leads_somewhere_mw", include_str!("../presets/surge_leads_somewhere_mw.json")),
    ("Leads", "surge_leads_square", include_str!("../presets/surge_leads_square.json")),
    ("Leads", "surge_leads_squelch", include_str!("../presets/surge_leads_squelch.json")),
    ("Leads", "surge_leads_squiggly", include_str!("../presets/surge_leads_squiggly.json")),
    ("Leads", "surge_leads_stepmother", include_str!("../presets/surge_leads_stepmother.json")),
    ("Leads", "surge_leads_sweepy", include_str!("../presets/surge_leads_sweepy.json")),
    ("Leads", "surge_leads_sync_lead_1a", include_str!("../presets/surge_leads_sync_lead_1a.json")),
    ("Leads", "surge_leads_syncharmonics", include_str!("../presets/surge_leads_syncharmonics.json")),
    ("Leads", "surge_leads_synth_guitar", include_str!("../presets/surge_leads_synth_guitar.json")),
    ("Leads", "surge_leads_synth_guitar_2", include_str!("../presets/surge_leads_synth_guitar_2.json")),
    ("Leads", "surge_leads_talky_mw", include_str!("../presets/surge_leads_talky_mw.json")),
    ("Leads", "surge_leads_talky_mw_2", include_str!("../presets/surge_leads_talky_mw_2.json")),
    ("Leads", "surge_leads_tanktop", include_str!("../presets/surge_leads_tanktop.json")),
    ("Leads", "surge_leads_tok", include_str!("../presets/surge_leads_tok.json")),
    ("Leads", "surge_leads_tolk", include_str!("../presets/surge_leads_tolk.json")),
    ("Leads", "surge_leads_triple", include_str!("../presets/surge_leads_triple.json")),
    ("Leads", "surge_leads_turbo", include_str!("../presets/surge_leads_turbo.json")),
    ("Leads", "surge_leads_turbosolo", include_str!("../presets/surge_leads_turbosolo.json")),
    ("Leads", "surge_leads_untamed", include_str!("../presets/surge_leads_untamed.json")),
    ("Leads", "surge_leads_updown", include_str!("../presets/surge_leads_updown.json")),
    ("Leads", "surge_leads_very_chorus", include_str!("../presets/surge_leads_very_chorus.json")),
    ("Leads", "surge_leads_violini_solo", include_str!("../presets/surge_leads_violini_solo.json")),
    ("Leads", "surge_leads_vocal_lead", include_str!("../presets/surge_leads_vocal_lead.json")),
    ("Leads", "surge_leads_wg_01", include_str!("../presets/surge_leads_wg_01.json")),
    ("Leads", "surge_leads_wombat", include_str!("../presets/surge_leads_wombat.json")),
    ("Leads", "surge_leads_zerozeroone", include_str!("../presets/surge_leads_zerozeroone.json")),
    // Surge XT: Pads
    ("Pads", "surge_pads_alias_pornography", include_str!("../presets/surge_pads_alias_pornography.json")),
    ("Pads", "surge_pads_assymetry_2", include_str!("../presets/surge_pads_assymetry_2.json")),
    ("Pads", "surge_pads_bell_pad", include_str!("../presets/surge_pads_bell_pad.json")),
    ("Pads", "surge_pads_bells_and_sweep", include_str!("../presets/surge_pads_bells_and_sweep.json")),
    ("Pads", "surge_pads_bright", include_str!("../presets/surge_pads_bright.json")),
    ("Pads", "surge_pads_buggy_brass", include_str!("../presets/surge_pads_buggy_brass.json")),
    ("Pads", "surge_pads_burden", include_str!("../presets/surge_pads_burden.json")),
    ("Pads", "surge_pads_canadians", include_str!("../presets/surge_pads_canadians.json")),
    ("Pads", "surge_pads_choirpad_thing", include_str!("../presets/surge_pads_choirpad_thing.json")),
    ("Pads", "surge_pads_chowning", include_str!("../presets/surge_pads_chowning.json")),
    ("Pads", "surge_pads_communication", include_str!("../presets/surge_pads_communication.json")),
    ("Pads", "surge_pads_computers_in_space", include_str!("../presets/surge_pads_computers_in_space.json")),
    ("Pads", "surge_pads_death_to_gator", include_str!("../presets/surge_pads_death_to_gator.json")),
    ("Pads", "surge_pads_distant", include_str!("../presets/surge_pads_distant.json")),
    ("Pads", "surge_pads_distorted_choir_1", include_str!("../presets/surge_pads_distorted_choir_1.json")),
    ("Pads", "surge_pads_distorted_choir_2", include_str!("../presets/surge_pads_distorted_choir_2.json")),
    ("Pads", "surge_pads_endgame", include_str!("../presets/surge_pads_endgame.json")),
    ("Pads", "surge_pads_flux_capacitor", include_str!("../presets/surge_pads_flux_capacitor.json")),
    ("Pads", "surge_pads_fm_pad", include_str!("../presets/surge_pads_fm_pad.json")),
    ("Pads", "surge_pads_formants_mw", include_str!("../presets/surge_pads_formants_mw.json")),
    ("Pads", "surge_pads_ghost_pad", include_str!("../presets/surge_pads_ghost_pad.json")),
    ("Pads", "surge_pads_gliss_movement", include_str!("../presets/surge_pads_gliss_movement.json")),
    ("Pads", "surge_pads_growth", include_str!("../presets/surge_pads_growth.json")),
    ("Pads", "surge_pads_harmonics_sweep", include_str!("../presets/surge_pads_harmonics_sweep.json")),
    ("Pads", "surge_pads_harshsaw", include_str!("../presets/surge_pads_harshsaw.json")),
    ("Pads", "surge_pads_hmm", include_str!("../presets/surge_pads_hmm.json")),
    ("Pads", "surge_pads_legacy", include_str!("../presets/surge_pads_legacy.json")),
    ("Pads", "surge_pads_louder", include_str!("../presets/surge_pads_louder.json")),
    ("Pads", "surge_pads_moody_statement", include_str!("../presets/surge_pads_moody_statement.json")),
    ("Pads", "surge_pads_mw_pulsating", include_str!("../presets/surge_pads_mw_pulsating.json")),
    ("Pads", "surge_pads_newton_was_evil", include_str!("../presets/surge_pads_newton_was_evil.json")),
    ("Pads", "surge_pads_ooh", include_str!("../presets/surge_pads_ooh.json")),
    ("Pads", "surge_pads_pad_1", include_str!("../presets/surge_pads_pad_1.json")),
    ("Pads", "surge_pads_pad_2_mw", include_str!("../presets/surge_pads_pad_2_mw.json")),
    ("Pads", "surge_pads_pad_3", include_str!("../presets/surge_pads_pad_3.json")),
    ("Pads", "surge_pads_pad_4", include_str!("../presets/surge_pads_pad_4.json")),
    ("Pads", "surge_pads_pad_5", include_str!("../presets/surge_pads_pad_5.json")),
    ("Pads", "surge_pads_pad_6", include_str!("../presets/surge_pads_pad_6.json")),
    ("Pads", "surge_pads_pad_7", include_str!("../presets/surge_pads_pad_7.json")),
    ("Pads", "surge_pads_pad_8", include_str!("../presets/surge_pads_pad_8.json")),
    ("Pads", "surge_pads_primes", include_str!("../presets/surge_pads_primes.json")),
    ("Pads", "surge_pads_retrochoir", include_str!("../presets/surge_pads_retrochoir.json")),
    ("Pads", "surge_pads_ringing", include_str!("../presets/surge_pads_ringing.json")),
    ("Pads", "surge_pads_robochoir_1", include_str!("../presets/surge_pads_robochoir_1.json")),
    ("Pads", "surge_pads_robochoir_2", include_str!("../presets/surge_pads_robochoir_2.json")),
    ("Pads", "surge_pads_safety", include_str!("../presets/surge_pads_safety.json")),
    ("Pads", "surge_pads_sawteeth", include_str!("../presets/surge_pads_sawteeth.json")),
    ("Pads", "surge_pads_semiconductor", include_str!("../presets/surge_pads_semiconductor.json")),
    ("Pads", "surge_pads_semihaunt", include_str!("../presets/surge_pads_semihaunt.json")),
    ("Pads", "surge_pads_smoothdist", include_str!("../presets/surge_pads_smoothdist.json")),
    ("Pads", "surge_pads_sparkly", include_str!("../presets/surge_pads_sparkly.json")),
    ("Pads", "surge_pads_sprinkly", include_str!("../presets/surge_pads_sprinkly.json")),
    ("Pads", "surge_pads_still", include_str!("../presets/surge_pads_still.json")),
    ("Pads", "surge_pads_stretch", include_str!("../presets/surge_pads_stretch.json")),
    ("Pads", "surge_pads_subtle_comb_strings", include_str!("../presets/surge_pads_subtle_comb_strings.json")),
    ("Pads", "surge_pads_sunday", include_str!("../presets/surge_pads_sunday.json")),
    ("Pads", "surge_pads_super", include_str!("../presets/surge_pads_super.json")),
    ("Pads", "surge_pads_synth_choir_mw_o_ah", include_str!("../presets/surge_pads_synth_choir_mw_o_ah.json")),
    ("Pads", "surge_pads_verb_pad", include_str!("../presets/surge_pads_verb_pad.json")),
    ("Pads", "surge_pads_well", include_str!("../presets/surge_pads_well.json")),
    ("Pads", "surge_pads_winter_warmer", include_str!("../presets/surge_pads_winter_warmer.json")),
    ("Pads", "surge_pads_worried", include_str!("../presets/surge_pads_worried.json")),
    ("Pads", "surge_pads_xbox", include_str!("../presets/surge_pads_xbox.json")),
    ("Pads", "surge_pads_xbox_2", include_str!("../presets/surge_pads_xbox_2.json")),
    ("Pads", "surge_pads_yeti_funeral", include_str!("../presets/surge_pads_yeti_funeral.json")),
    // Surge XT: Percussion
    ("Percussion", "surge_percussion_drum_1", include_str!("../presets/surge_percussion_drum_1.json")),
    ("Percussion", "surge_percussion_kick_909ish", include_str!("../presets/surge_percussion_kick_909ish.json")),
    ("Percussion", "surge_percussion_kick_tech_1", include_str!("../presets/surge_percussion_kick_tech_1.json")),
    ("Percussion", "surge_percussion_kick_tech_2", include_str!("../presets/surge_percussion_kick_tech_2.json")),
    ("Percussion", "surge_percussion_snaretight", include_str!("../presets/surge_percussion_snaretight.json")),
    ("Percussion", "surge_percussion_synthom_1", include_str!("../presets/surge_percussion_synthom_1.json")),
    ("Percussion", "surge_percussion_synthom_2", include_str!("../presets/surge_percussion_synthom_2.json")),
    ("Percussion", "surge_percussion_synthom_3", include_str!("../presets/surge_percussion_synthom_3.json")),
    ("Percussion", "surge_percussion_verber", include_str!("../presets/surge_percussion_verber.json")),
    // Surge XT: Plucks
    ("Plucks", "surge_plucks_80s_gliss", include_str!("../presets/surge_plucks_80s_gliss.json")),
    ("Plucks", "surge_plucks_acme_1", include_str!("../presets/surge_plucks_acme_1.json")),
    ("Plucks", "surge_plucks_agropop", include_str!("../presets/surge_plucks_agropop.json")),
    ("Plucks", "surge_plucks_ambient_e_guitar", include_str!("../presets/surge_plucks_ambient_e_guitar.json")),
    ("Plucks", "surge_plucks_artificial", include_str!("../presets/surge_plucks_artificial.json")),
    ("Plucks", "surge_plucks_assymetry", include_str!("../presets/surge_plucks_assymetry.json")),
    ("Plucks", "surge_plucks_battered_beauty", include_str!("../presets/surge_plucks_battered_beauty.json")),
    ("Plucks", "surge_plucks_bell_1", include_str!("../presets/surge_plucks_bell_1.json")),
    ("Plucks", "surge_plucks_bell_2", include_str!("../presets/surge_plucks_bell_2.json")),
    ("Plucks", "surge_plucks_belle", include_str!("../presets/surge_plucks_belle.json")),
    ("Plucks", "surge_plucks_bite", include_str!("../presets/surge_plucks_bite.json")),
    ("Plucks", "surge_plucks_blekinge", include_str!("../presets/surge_plucks_blekinge.json")),
    ("Plucks", "surge_plucks_brut_de_bollebygd", include_str!("../presets/surge_plucks_brut_de_bollebygd.json")),
    ("Plucks", "surge_plucks_clean", include_str!("../presets/surge_plucks_clean.json")),
    ("Plucks", "surge_plucks_clrkswrd", include_str!("../presets/surge_plucks_clrkswrd.json")),
    ("Plucks", "surge_plucks_combpluck", include_str!("../presets/surge_plucks_combpluck.json")),
    ("Plucks", "surge_plucks_convex", include_str!("../presets/surge_plucks_convex.json")),
    ("Plucks", "surge_plucks_cuto", include_str!("../presets/surge_plucks_cuto.json")),
    ("Plucks", "surge_plucks_delay_pops_1", include_str!("../presets/surge_plucks_delay_pops_1.json")),
    ("Plucks", "surge_plucks_delay_pops_2", include_str!("../presets/surge_plucks_delay_pops_2.json")),
    ("Plucks", "surge_plucks_delay_pops_3", include_str!("../presets/surge_plucks_delay_pops_3.json")),
    ("Plucks", "surge_plucks_delay_pops_4", include_str!("../presets/surge_plucks_delay_pops_4.json")),
    ("Plucks", "surge_plucks_delay_pops_5", include_str!("../presets/surge_plucks_delay_pops_5.json")),
    ("Plucks", "surge_plucks_delaydancer", include_str!("../presets/surge_plucks_delaydancer.json")),
    ("Plucks", "surge_plucks_diamonds", include_str!("../presets/surge_plucks_diamonds.json")),
    ("Plucks", "surge_plucks_east", include_str!("../presets/surge_plucks_east.json")),
    ("Plucks", "surge_plucks_eguitar", include_str!("../presets/surge_plucks_eguitar.json")),
    ("Plucks", "surge_plucks_enhanced_forest", include_str!("../presets/surge_plucks_enhanced_forest.json")),
    ("Plucks", "surge_plucks_falling_down", include_str!("../presets/surge_plucks_falling_down.json")),
    ("Plucks", "surge_plucks_fantasy_bell", include_str!("../presets/surge_plucks_fantasy_bell.json")),
    ("Plucks", "surge_plucks_fluortant", include_str!("../presets/surge_plucks_fluortant.json")),
    ("Plucks", "surge_plucks_fm_pluck", include_str!("../presets/surge_plucks_fm_pluck.json")),
    ("Plucks", "surge_plucks_fm_poops", include_str!("../presets/surge_plucks_fm_poops.json")),
    ("Plucks", "surge_plucks_fog", include_str!("../presets/surge_plucks_fog.json")),
    ("Plucks", "surge_plucks_forever", include_str!("../presets/surge_plucks_forever.json")),
    ("Plucks", "surge_plucks_freedom_fries", include_str!("../presets/surge_plucks_freedom_fries.json")),
    ("Plucks", "surge_plucks_friendly", include_str!("../presets/surge_plucks_friendly.json")),
    ("Plucks", "surge_plucks_frog", include_str!("../presets/surge_plucks_frog.json")),
    ("Plucks", "surge_plucks_glisspluck", include_str!("../presets/surge_plucks_glisspluck.json")),
    ("Plucks", "surge_plucks_glisspluck_distorted", include_str!("../presets/surge_plucks_glisspluck_distorted.json")),
    ("Plucks", "surge_plucks_good_childhood", include_str!("../presets/surge_plucks_good_childhood.json")),
    ("Plucks", "surge_plucks_gt1", include_str!("../presets/surge_plucks_gt1.json")),
    ("Plucks", "surge_plucks_half_fm", include_str!("../presets/surge_plucks_half_fm.json")),
    ("Plucks", "surge_plucks_happy", include_str!("../presets/surge_plucks_happy.json")),
    ("Plucks", "surge_plucks_harmonics_1", include_str!("../presets/surge_plucks_harmonics_1.json")),
    ("Plucks", "surge_plucks_harmonics_2", include_str!("../presets/surge_plucks_harmonics_2.json")),
    ("Plucks", "surge_plucks_hasselhoff", include_str!("../presets/surge_plucks_hasselhoff.json")),
    ("Plucks", "surge_plucks_hybrid_1", include_str!("../presets/surge_plucks_hybrid_1.json")),
    ("Plucks", "surge_plucks_hybrid_2", include_str!("../presets/surge_plucks_hybrid_2.json")),
    ("Plucks", "surge_plucks_icebreaker", include_str!("../presets/surge_plucks_icebreaker.json")),
    ("Plucks", "surge_plucks_late_fall", include_str!("../presets/surge_plucks_late_fall.json")),
    ("Plucks", "surge_plucks_light_1", include_str!("../presets/surge_plucks_light_1.json")),
    ("Plucks", "surge_plucks_light_2", include_str!("../presets/surge_plucks_light_2.json")),
    ("Plucks", "surge_plucks_lighter", include_str!("../presets/surge_plucks_lighter.json")),
    ("Plucks", "surge_plucks_lil_apos_exploders", include_str!("../presets/surge_plucks_lil_apos_exploders.json")),
    ("Plucks", "surge_plucks_lofipluck", include_str!("../presets/surge_plucks_lofipluck.json")),
    ("Plucks", "surge_plucks_magic_musicbox", include_str!("../presets/surge_plucks_magic_musicbox.json")),
    ("Plucks", "surge_plucks_magical_guitar", include_str!("../presets/surge_plucks_magical_guitar.json")),
    ("Plucks", "surge_plucks_man_machine", include_str!("../presets/surge_plucks_man_machine.json")),
    ("Plucks", "surge_plucks_messy", include_str!("../presets/surge_plucks_messy.json")),
    ("Plucks", "surge_plucks_metallic", include_str!("../presets/surge_plucks_metallic.json")),
    ("Plucks", "surge_plucks_mol", include_str!("../presets/surge_plucks_mol.json")),
    ("Plucks", "surge_plucks_molusk", include_str!("../presets/surge_plucks_molusk.json")),
    ("Plucks", "surge_plucks_mr_sparkle", include_str!("../presets/surge_plucks_mr_sparkle.json")),
    ("Plucks", "surge_plucks_mw_morph", include_str!("../presets/surge_plucks_mw_morph.json")),
    ("Plucks", "surge_plucks_mystic", include_str!("../presets/surge_plucks_mystic.json")),
    ("Plucks", "surge_plucks_nice_pluck_1", include_str!("../presets/surge_plucks_nice_pluck_1.json")),
    ("Plucks", "surge_plucks_nice_pluck_2", include_str!("../presets/surge_plucks_nice_pluck_2.json")),
    ("Plucks", "surge_plucks_nice_pluck_3", include_str!("../presets/surge_plucks_nice_pluck_3.json")),
    ("Plucks", "surge_plucks_nice_pluck_4", include_str!("../presets/surge_plucks_nice_pluck_4.json")),
    ("Plucks", "surge_plucks_nice_pluck_5", include_str!("../presets/surge_plucks_nice_pluck_5.json")),
    ("Plucks", "surge_plucks_nolla", include_str!("../presets/surge_plucks_nolla.json")),
    ("Plucks", "surge_plucks_norrland", include_str!("../presets/surge_plucks_norrland.json")),
    ("Plucks", "surge_plucks_piano_remains_1", include_str!("../presets/surge_plucks_piano_remains_1.json")),
    ("Plucks", "surge_plucks_piano_remains_2", include_str!("../presets/surge_plucks_piano_remains_2.json")),
    ("Plucks", "surge_plucks_pie", include_str!("../presets/surge_plucks_pie.json")),
    ("Plucks", "surge_plucks_pinkerton_tinfurter", include_str!("../presets/surge_plucks_pinkerton_tinfurter.json")),
    ("Plucks", "surge_plucks_pol_pot", include_str!("../presets/surge_plucks_pol_pot.json")),
    ("Plucks", "surge_plucks_pulsar", include_str!("../presets/surge_plucks_pulsar.json")),
    ("Plucks", "surge_plucks_pulsii", include_str!("../presets/surge_plucks_pulsii.json")),
    ("Plucks", "surge_plucks_pure_square", include_str!("../presets/surge_plucks_pure_square.json")),
    ("Plucks", "surge_plucks_retrofit", include_str!("../presets/surge_plucks_retrofit.json")),
    ("Plucks", "surge_plucks_reverend_b", include_str!("../presets/surge_plucks_reverend_b.json")),
    ("Plucks", "surge_plucks_rutherford_menskin_mw", include_str!("../presets/surge_plucks_rutherford_menskin_mw.json")),
    ("Plucks", "surge_plucks_saw_pluck", include_str!("../presets/surge_plucks_saw_pluck.json")),
    ("Plucks", "surge_plucks_scrapepluck", include_str!("../presets/surge_plucks_scrapepluck.json")),
    ("Plucks", "surge_plucks_sharpness", include_str!("../presets/surge_plucks_sharpness.json")),
    ("Plucks", "surge_plucks_simple_pw", include_str!("../presets/surge_plucks_simple_pw.json")),
    ("Plucks", "surge_plucks_simple_waveguide", include_str!("../presets/surge_plucks_simple_waveguide.json")),
    ("Plucks", "surge_plucks_sinus_verby_pops", include_str!("../presets/surge_plucks_sinus_verby_pops.json")),
    ("Plucks", "surge_plucks_snap", include_str!("../presets/surge_plucks_snap.json")),
    ("Plucks", "surge_plucks_soft_space_oboe_pops", include_str!("../presets/surge_plucks_soft_space_oboe_pops.json")),
    ("Plucks", "surge_plucks_softsyncsaw", include_str!("../presets/surge_plucks_softsyncsaw.json")),
    ("Plucks", "surge_plucks_soift", include_str!("../presets/surge_plucks_soift.json")),
    ("Plucks", "surge_plucks_spell", include_str!("../presets/surge_plucks_spell.json")),
    ("Plucks", "surge_plucks_squareblinks", include_str!("../presets/surge_plucks_squareblinks.json")),
    ("Plucks", "surge_plucks_squarepop", include_str!("../presets/surge_plucks_squarepop.json")),
    ("Plucks", "surge_plucks_sync_pluck", include_str!("../presets/surge_plucks_sync_pluck.json")),
    ("Plucks", "surge_plucks_syncecho", include_str!("../presets/surge_plucks_syncecho.json")),
    ("Plucks", "surge_plucks_syncsquare_pluck", include_str!("../presets/surge_plucks_syncsquare_pluck.json")),
    ("Plucks", "surge_plucks_that_comb_magic", include_str!("../presets/surge_plucks_that_comb_magic.json")),
    ("Plucks", "surge_plucks_the_1980s", include_str!("../presets/surge_plucks_the_1980s.json")),
    ("Plucks", "surge_plucks_thingamajob", include_str!("../presets/surge_plucks_thingamajob.json")),
    ("Plucks", "surge_plucks_tinker", include_str!("../presets/surge_plucks_tinker.json")),
    ("Plucks", "surge_plucks_trancy", include_str!("../presets/surge_plucks_trancy.json")),
    ("Plucks", "surge_plucks_ultra_violet", include_str!("../presets/surge_plucks_ultra_violet.json")),
    ("Plucks", "surge_plucks_vhs_soundtrack", include_str!("../presets/surge_plucks_vhs_soundtrack.json")),
    ("Plucks", "surge_plucks_wire", include_str!("../presets/surge_plucks_wire.json")),
    ("Plucks", "surge_plucks_wire_mw", include_str!("../presets/surge_plucks_wire_mw.json")),
    ("Plucks", "surge_plucks_woody_1", include_str!("../presets/surge_plucks_woody_1.json")),
    ("Plucks", "surge_plucks_you_fairy", include_str!("../presets/surge_plucks_you_fairy.json")),
    // Surge XT: Polysynths
    ("Polysynths", "surge_polysynths_1804", include_str!("../presets/surge_polysynths_1804.json")),
    ("Polysynths", "surge_polysynths_ahhpolly", include_str!("../presets/surge_polysynths_ahhpolly.json")),
    ("Polysynths", "surge_polysynths_analyse", include_str!("../presets/surge_polysynths_analyse.json")),
    ("Polysynths", "surge_polysynths_anthemish_1", include_str!("../presets/surge_polysynths_anthemish_1.json")),
    ("Polysynths", "surge_polysynths_anthemish_2", include_str!("../presets/surge_polysynths_anthemish_2.json")),
    ("Polysynths", "surge_polysynths_anthemish_3", include_str!("../presets/surge_polysynths_anthemish_3.json")),
    ("Polysynths", "surge_polysynths_bolibompa", include_str!("../presets/surge_polysynths_bolibompa.json")),
    ("Polysynths", "surge_polysynths_boss", include_str!("../presets/surge_polysynths_boss.json")),
    ("Polysynths", "surge_polysynths_buggy_brass", include_str!("../presets/surge_polysynths_buggy_brass.json")),
    ("Polysynths", "surge_polysynths_call", include_str!("../presets/surge_polysynths_call.json")),
    ("Polysynths", "surge_polysynths_concave", include_str!("../presets/surge_polysynths_concave.json")),
    ("Polysynths", "surge_polysynths_dirty_hole", include_str!("../presets/surge_polysynths_dirty_hole.json")),
    ("Polysynths", "surge_polysynths_disturbing_resonance", include_str!("../presets/surge_polysynths_disturbing_resonance.json")),
    ("Polysynths", "surge_polysynths_embrrass", include_str!("../presets/surge_polysynths_embrrass.json")),
    ("Polysynths", "surge_polysynths_eyan", include_str!("../presets/surge_polysynths_eyan.json")),
    ("Polysynths", "surge_polysynths_failure", include_str!("../presets/surge_polysynths_failure.json")),
    ("Polysynths", "surge_polysynths_fastpoly", include_str!("../presets/surge_polysynths_fastpoly.json")),
    ("Polysynths", "surge_polysynths_filtmod", include_str!("../presets/surge_polysynths_filtmod.json")),
    ("Polysynths", "surge_polysynths_fm_poly_1", include_str!("../presets/surge_polysynths_fm_poly_1.json")),
    ("Polysynths", "surge_polysynths_fonk", include_str!("../presets/surge_polysynths_fonk.json")),
    ("Polysynths", "surge_polysynths_formant_sweep", include_str!("../presets/surge_polysynths_formant_sweep.json")),
    ("Polysynths", "surge_polysynths_fun_with_feedback", include_str!("../presets/surge_polysynths_fun_with_feedback.json")),
    ("Polysynths", "surge_polysynths_gentle", include_str!("../presets/surge_polysynths_gentle.json")),
    ("Polysynths", "surge_polysynths_havoc", include_str!("../presets/surge_polysynths_havoc.json")),
    ("Polysynths", "surge_polysynths_hombre", include_str!("../presets/surge_polysynths_hombre.json")),
    ("Polysynths", "surge_polysynths_hugeness", include_str!("../presets/surge_polysynths_hugeness.json")),
    ("Polysynths", "surge_polysynths_instant_coffee_pwm", include_str!("../presets/surge_polysynths_instant_coffee_pwm.json")),
    ("Polysynths", "surge_polysynths_japanese_space_ulation_wheel", include_str!("../presets/surge_polysynths_japanese_space_ulation_wheel.json")),
    ("Polysynths", "surge_polysynths_japanese_unison", include_str!("../presets/surge_polysynths_japanese_unison.json")),
    ("Polysynths", "surge_polysynths_jim", include_str!("../presets/surge_polysynths_jim.json")),
    ("Polysynths", "surge_polysynths_keep_em_coming", include_str!("../presets/surge_polysynths_keep_em_coming.json")),
    ("Polysynths", "surge_polysynths_larger", include_str!("../presets/surge_polysynths_larger.json")),
    ("Polysynths", "surge_polysynths_licht", include_str!("../presets/surge_polysynths_licht.json")),
    ("Polysynths", "surge_polysynths_maclaurin", include_str!("../presets/surge_polysynths_maclaurin.json")),
    ("Polysynths", "surge_polysynths_megamega", include_str!("../presets/surge_polysynths_megamega.json")),
    ("Polysynths", "surge_polysynths_megasynth_1", include_str!("../presets/surge_polysynths_megasynth_1.json")),
    ("Polysynths", "surge_polysynths_megasynth_2", include_str!("../presets/surge_polysynths_megasynth_2.json")),
    ("Polysynths", "surge_polysynths_megasynth_3", include_str!("../presets/surge_polysynths_megasynth_3.json")),
    ("Polysynths", "surge_polysynths_megasynth_4", include_str!("../presets/surge_polysynths_megasynth_4.json")),
    ("Polysynths", "surge_polysynths_melon", include_str!("../presets/surge_polysynths_melon.json")),
    ("Polysynths", "surge_polysynths_messy", include_str!("../presets/surge_polysynths_messy.json")),
    ("Polysynths", "surge_polysynths_metalchonk", include_str!("../presets/surge_polysynths_metalchonk.json")),
    ("Polysynths", "surge_polysynths_mg", include_str!("../presets/surge_polysynths_mg.json")),
    ("Polysynths", "surge_polysynths_noisetone", include_str!("../presets/surge_polysynths_noisetone.json")),
    ("Polysynths", "surge_polysynths_notched_saws", include_str!("../presets/surge_polysynths_notched_saws.json")),
    ("Polysynths", "surge_polysynths_oiro", include_str!("../presets/surge_polysynths_oiro.json")),
    ("Polysynths", "surge_polysynths_ol_apos_sampler", include_str!("../presets/surge_polysynths_ol_apos_sampler.json")),
    ("Polysynths", "surge_polysynths_oldie", include_str!("../presets/surge_polysynths_oldie.json")),
    ("Polysynths", "surge_polysynths_past_tense", include_str!("../presets/surge_polysynths_past_tense.json")),
    ("Polysynths", "surge_polysynths_phasey_01", include_str!("../presets/surge_polysynths_phasey_01.json")),
    ("Polysynths", "surge_polysynths_play_louder", include_str!("../presets/surge_polysynths_play_louder.json")),
    ("Polysynths", "surge_polysynths_ploppy", include_str!("../presets/surge_polysynths_ploppy.json")),
    ("Polysynths", "surge_polysynths_poly_ahs", include_str!("../presets/surge_polysynths_poly_ahs.json")),
    ("Polysynths", "surge_polysynths_poly_lala", include_str!("../presets/surge_polysynths_poly_lala.json")),
    ("Polysynths", "surge_polysynths_ppg_choir", include_str!("../presets/surge_polysynths_ppg_choir.json")),
    ("Polysynths", "surge_polysynths_pwm_avenger", include_str!("../presets/surge_polysynths_pwm_avenger.json")),
    ("Polysynths", "surge_polysynths_quantization_choice", include_str!("../presets/surge_polysynths_quantization_choice.json")),
    ("Polysynths", "surge_polysynths_quasi", include_str!("../presets/surge_polysynths_quasi.json")),
    ("Polysynths", "surge_polysynths_quote", include_str!("../presets/surge_polysynths_quote.json")),
    ("Polysynths", "surge_polysynths_ralph", include_str!("../presets/surge_polysynths_ralph.json")),
    ("Polysynths", "surge_polysynths_reset", include_str!("../presets/surge_polysynths_reset.json")),
    ("Polysynths", "surge_polysynths_retrofit", include_str!("../presets/surge_polysynths_retrofit.json")),
    ("Polysynths", "surge_polysynths_retrograde", include_str!("../presets/surge_polysynths_retrograde.json")),
    ("Polysynths", "surge_polysynths_rez", include_str!("../presets/surge_polysynths_rez.json")),
    ("Polysynths", "surge_polysynths_ringo", include_str!("../presets/surge_polysynths_ringo.json")),
    ("Polysynths", "surge_polysynths_ringsweep", include_str!("../presets/surge_polysynths_ringsweep.json")),
    ("Polysynths", "surge_polysynths_ruler", include_str!("../presets/surge_polysynths_ruler.json")),
    ("Polysynths", "surge_polysynths_rusty", include_str!("../presets/surge_polysynths_rusty.json")),
    ("Polysynths", "surge_polysynths_sawteeth", include_str!("../presets/surge_polysynths_sawteeth.json")),
    ("Polysynths", "surge_polysynths_serious_distortion", include_str!("../presets/surge_polysynths_serious_distortion.json")),
    ("Polysynths", "surge_polysynths_shenanigans", include_str!("../presets/surge_polysynths_shenanigans.json")),
    ("Polysynths", "surge_polysynths_simplistic", include_str!("../presets/surge_polysynths_simplistic.json")),
    ("Polysynths", "surge_polysynths_sinesaw", include_str!("../presets/surge_polysynths_sinesaw.json")),
    ("Polysynths", "surge_polysynths_sinesaw7", include_str!("../presets/surge_polysynths_sinesaw7.json")),
    ("Polysynths", "surge_polysynths_sizzling_sweep", include_str!("../presets/surge_polysynths_sizzling_sweep.json")),
    ("Polysynths", "surge_polysynths_skatteverket", include_str!("../presets/surge_polysynths_skatteverket.json")),
    ("Polysynths", "surge_polysynths_slowpoly_mw", include_str!("../presets/surge_polysynths_slowpoly_mw.json")),
    ("Polysynths", "surge_polysynths_smooth_stabs", include_str!("../presets/surge_polysynths_smooth_stabs.json")),
    ("Polysynths", "surge_polysynths_space_fm", include_str!("../presets/surge_polysynths_space_fm.json")),
    ("Polysynths", "surge_polysynths_spacematron", include_str!("../presets/surge_polysynths_spacematron.json")),
    ("Polysynths", "surge_polysynths_spik", include_str!("../presets/surge_polysynths_spik.json")),
    ("Polysynths", "surge_polysynths_stepportamento", include_str!("../presets/surge_polysynths_stepportamento.json")),
    ("Polysynths", "surge_polysynths_taikonaut", include_str!("../presets/surge_polysynths_taikonaut.json")),
    ("Polysynths", "surge_polysynths_tarnce", include_str!("../presets/surge_polysynths_tarnce.json")),
    ("Polysynths", "surge_polysynths_thynchronization", include_str!("../presets/surge_polysynths_thynchronization.json")),
    ("Polysynths", "surge_polysynths_uni_1", include_str!("../presets/surge_polysynths_uni_1.json")),
    ("Polysynths", "surge_polysynths_uni_2", include_str!("../presets/surge_polysynths_uni_2.json")),
    ("Polysynths", "surge_polysynths_unisaw_fb", include_str!("../presets/surge_polysynths_unisaw_fb.json")),
    ("Polysynths", "surge_polysynths_vel2cutoff", include_str!("../presets/surge_polysynths_vel2cutoff.json")),
    ("Polysynths", "surge_polysynths_violini_poly", include_str!("../presets/surge_polysynths_violini_poly.json")),
    ("Polysynths", "surge_polysynths_waver", include_str!("../presets/surge_polysynths_waver.json")),
    ("Polysynths", "surge_polysynths_zizzly_saw", include_str!("../presets/surge_polysynths_zizzly_saw.json")),
    // Surge XT: Sequences
    // Surge XT: Winds
    ("Winds", "surge_winds_clarinet", include_str!("../presets/surge_winds_clarinet.json")),
    ("Winds", "surge_winds_cyberflute", include_str!("../presets/surge_winds_cyberflute.json")),
    ("Winds", "surge_winds_dreamy_flutee", include_str!("../presets/surge_winds_dreamy_flutee.json")),
    ("Winds", "surge_winds_fake_ethno", include_str!("../presets/surge_winds_fake_ethno.json")),
    ("Winds", "surge_winds_flute_1", include_str!("../presets/surge_winds_flute_1.json")),
    ("Winds", "surge_winds_flute_2", include_str!("../presets/surge_winds_flute_2.json")),
    ("Winds", "surge_winds_low", include_str!("../presets/surge_winds_low.json")),
    ("Winds", "surge_winds_tragic_winds", include_str!("../presets/surge_winds_tragic_winds.json")),
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
    /// If true, this layer uses SF2 soundfont instead of DSP preset.
    #[serde(default)]
    pub sf2_mode: bool,
    /// SF2 program number (0-127).
    #[serde(default)]
    pub sf2_program: u8,
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

#[allow(dead_code)]
#[derive(Clone, Serialize, Deserialize)]
pub struct SetList {
    pub name: String,
    pub entries: Vec<String>, // performance names, in order
}

#[allow(dead_code)]
fn setlist_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("mini_midi_synth").join("setlists"))
}

#[allow(dead_code)]
pub fn save_setlist(setlist: &SetList) -> Result<PathBuf> {
    let dir = setlist_dir().context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let filename = setlist.name.to_lowercase().replace(' ', "_") + ".json";
    let path = dir.join(filename);
    let json = serde_json::to_string_pretty(setlist)?;
    fs::write(&path, json)?;
    Ok(path)
}

#[allow(dead_code)]
pub fn load_setlist(path: &std::path::Path) -> Result<SetList> {
    let contents = fs::read_to_string(path).context("Failed to read setlist file")?;
    let sl: SetList = serde_json::from_str(&contents).context("Failed to parse setlist JSON")?;
    Ok(sl)
}

#[allow(dead_code)]
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
