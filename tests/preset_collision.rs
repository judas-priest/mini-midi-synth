//! Preset filename collision/legacy-migration contract (review finding #3).

use mini_midi_synth::preset;

fn test_patch(name: &str) -> preset::Patch {
    let mut p = preset::load_factory_patches()[0].clone();
    p.name = name.into();
    p
}

/// (a) guard: same name upserts the same file
#[test]
fn same_name_save_updates_same_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let p = test_patch("bass lead");
    let path1 = preset::save_patch_in(&p, Some(dir.clone())).unwrap();
    let path2 = preset::save_patch_in(&p, Some(dir.clone())).unwrap();
    assert_eq!(path1, path2, "same name must upsert the same file");
}

/// (b) punctuation/case-only distinct names must not silently overwrite
#[test]
fn punctuation_only_distinct_names_do_not_collide() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let a = test_patch("bass lead");
    let b = test_patch("bass-lead!");
    let pa = preset::save_patch_in(&a, Some(dir.clone())).unwrap();
    let pb = preset::save_patch_in(&b, Some(dir.clone())).unwrap();
    assert_ne!(pa, pb, "distinct names silently overwrote each other");
    let saved: preset::Patch =
        serde_json::from_str(&std::fs::read_to_string(&pa).unwrap()).unwrap();
    assert_eq!(saved.name, "bass lead", "first preset was overwritten");
}

/// (b2) re-saving a collided name upserts its suffixed file instead of growing suffixes
#[test]
fn collided_name_re_save_upserts_suffixed_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let a = test_patch("bass lead");
    let b = test_patch("bass-lead!");
    let _ = preset::save_patch_in(&a, Some(dir.clone())).unwrap();
    let p1 = preset::save_patch_in(&b, Some(dir.clone())).unwrap();
    let p2 = preset::save_patch_in(&b, Some(dir.clone())).unwrap();
    assert_eq!(p1, p2, "collided re-save grew a new suffix instead of upserting");
    // and the owner's file is untouched
    let saved: preset::Patch =
        serde_json::from_str(&std::fs::read_to_string(dir.join("bass_lead.json")).unwrap()).unwrap();
    assert_eq!(saved.name, "bass lead");
}

/// (c) foreign/malformed JSON at the target path gets a suffix
#[test]
fn foreign_json_at_target_path_gets_suffix() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    std::fs::write(dir.join("bass_lead.json"), r#"{"not_a_patch": 1}"#).unwrap();
    let p = test_patch("bass lead");
    let path = preset::save_patch_in(&p, Some(dir.clone())).unwrap();
    assert_ne!(
        path.file_name().unwrap().to_str().unwrap(),
        "bass_lead.json",
        "save overwrote a foreign file"
    );
}

/// (d) legacy filename is migrated on directory load
#[test]
fn legacy_filename_migrated_on_load() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let body = serde_json::to_string(&test_patch("bass lead (user)")).unwrap();
    std::fs::write(dir.join("bass_lead_(user).json"), body).unwrap();
    let mut presets = Vec::new();
    preset::load_presets_from_dir(&dir, &mut presets);
    assert_eq!(presets.len(), 1);
    assert!(dir.join("bass_lead_user.json").exists(), "legacy file not renamed");
    assert!(!dir.join("bass_lead_(user).json").exists(), "legacy file left in place");
}
