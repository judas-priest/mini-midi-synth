//! Preset naming, serialization and on-disk safety.

use mini_midi_synth::preset::{self, sanitize_preset_filename};

#[test]
fn ordinary_names_become_snake_case() {
    assert_eq!(sanitize_preset_filename("Warm Pad"), "warm_pad");
    assert_eq!(sanitize_preset_filename("TB303 Acid v2"), "tb303_acid_v2");
}

#[test]
fn path_separators_cannot_escape_the_preset_directory() {
    assert_eq!(sanitize_preset_filename("../../etc/passwd"), "etc_passwd");
    assert_eq!(sanitize_preset_filename("/absolute/path"), "absolute_path");
    assert_eq!(sanitize_preset_filename(".."), "preset");
    // Whatever comes out must be a single path component.
    for weird in ["../x", "a/b/c", "..\\windows", "./."] {
        let name = sanitize_preset_filename(weird);
        assert!(!name.contains('/'), "{weird} produced {name}");
        assert!(!name.contains('\\'), "{weird} produced {name}");
        assert!(name != "." && name != "..", "{weird} produced {name}");
    }
}

#[test]
fn empty_and_exotic_names_still_produce_a_usable_filename() {
    assert_eq!(sanitize_preset_filename(""), "preset");
    assert_eq!(sanitize_preset_filename("???"), "preset");
    // Non-ASCII survives as long as it is alphanumeric.
    assert_eq!(sanitize_preset_filename("Бас"), "бас");
}

#[test]
fn atomic_write_leaves_no_temp_file() {
    let dir = std::env::temp_dir().join("mms-atomic-write-test");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("thing.json");

    mini_midi_synth::config::write_atomic(&target, "{\"a\":1}").unwrap();

    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"a\":1}");
    assert!(!dir.join("thing.json.tmp").exists(), "temp file must be renamed away");

    // Overwriting keeps exactly one file with the new content.
    mini_midi_synth::config::write_atomic(&target, "{\"a\":2}").unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{\"a\":2}");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn factory_presets_have_unique_names() {
    // Duplicate names make the preset list ambiguous and break "load by name",
    // which is how performances reference their parts.
    //
    // Only the compiled-in set is checked: converted Surge banks in the user's
    // data directory legitimately reuse names across categories, and a test
    // must not depend on what happens to be installed on the machine.
    let patches = preset::load_factory_patches();
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    for p in &patches {
        if !seen.insert(p.name.clone()) {
            duplicates.push(p.name.clone());
        }
    }
    assert!(duplicates.is_empty(), "duplicate preset names: {duplicates:?}");
}

#[test]
fn parse_wt_rejects_garbage_without_panicking() {
    assert!(preset::parse_wt(&[]).is_none());
    assert!(preset::parse_wt(b"not a wavetable").is_none());
    // Right magic, truncated payload.
    let mut truncated = b"vawt".to_vec();
    truncated.extend_from_slice(&1024u32.to_le_bytes());
    truncated.extend_from_slice(&16u16.to_le_bytes());
    truncated.extend_from_slice(&0u16.to_le_bytes());
    assert!(preset::parse_wt(&truncated).is_none());
}

#[test]
fn init_performance_references_existing_factory_patch() {
    let perf = preset::init_performance();
    assert_eq!(perf.split_point, None, "Init perf must not force a split point");
    assert_eq!(perf.parts.len(), 1);
    // The part references a preset by NAME — a factory rename would silently
    // fall back to patch_idx 0 (gui/mod.rs:1928) and change the Init sound.
    let patches = preset::load_factory_patches();
    assert!(patches.iter().any(|p| p.name == perf.parts[0].patch_name),
        "Init perf part references '{}', which is not a factory preset",
        perf.parts[0].patch_name);
}
