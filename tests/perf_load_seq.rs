//! Performance file load/migration contract (REFACTOR_AUDIT S0, prep for QW5).
//! Pins preset::load_performance / migrate_performance_parts behavior before
//! the GUI loader merge (audit quick-win #5).

use mini_midi_synth::preset::{self, PartConfig, Performance};

fn part(patch_name: &str, enabled: bool) -> PartConfig {
    PartConfig {
        patch_name: patch_name.to_string(),
        enabled,
        volume: 0.8,
        key_low: 0,
        key_high: 127,
        param_overrides: Default::default(),
        sf2_mode: false,
        sf2_program: 0,
        pan: 0.0,
        transpose: 0,
        mute: false,
        vel_min: 1,
        vel_max: 127,
        root_only: false,
    }
}

fn write_perf(dir: &tempfile::TempDir, name: &str, perf: &Performance) -> std::path::PathBuf {
    let path = dir.path().join(format!("{name}.json"));
    serde_json::to_writer(std::fs::File::create(&path).unwrap(), perf).unwrap();
    path
}

#[test]
fn legacy_8_part_performance_migrates_zone_b_to_8_12() {
    // Legacy 8-part layout kept zone B at indices 4..8; migration relocates
    // it to 8..12 (preset.rs:818-829).
    let perf = Performance {
        name: "legacy".into(),
        category: String::new(),
        split_point: None,
        parts: (0..8)
            .map(|i| part(&format!("P{i}"), !(4..6).contains(&i)))
            .collect(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = write_perf(&dir, "legacy", &perf);
    let loaded = preset::load_performance(&path).unwrap();
    assert_eq!(loaded.parts.len(), 12, "8-part file migrates to 16-part layout slots");
    for i in 0..4 {
        assert_eq!(loaded.parts[i].patch_name, format!("P{i}"));
        assert!(loaded.parts[i].enabled, "zone A preserved enabled");
    }
    for i in 4..8 {
        assert!(!loaded.parts[i].enabled, "gap 4..8 must be disabled");
    }
    for i in 8..12 {
        assert_eq!(loaded.parts[i].patch_name, format!("P{}", i - 4), "zone B relocated");
    }
    assert!(loaded.split_point.is_none(), "None split_point survives");
}

#[test]
fn missing_optional_fields_get_defaults() {
    // Hand-written minimal JSON: vel_min/vel_max/mute/pan/transpose/
    // sf2_*/root_only/category/split_point omitted.
    let json = r#"{
        "name": "minimal",
        "parts": [ { "patch_name": "Init", "enabled": true, "volume": 0.8,
                     "key_low": 0, "key_high": 127 } ]
    }"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("minimal.json");
    std::fs::write(&path, json).unwrap();
    let loaded = preset::load_performance(&path).unwrap();
    let p = &loaded.parts[0];
    assert_eq!(p.vel_min, 1, "serde default (preset.rs:786)");
    assert_eq!(p.vel_max, 127);
    assert!(!p.mute);
    assert_eq!(p.pan, 0.0);
    assert_eq!(p.transpose, 0);
    assert!(!p.sf2_mode);
    assert_eq!(p.sf2_program, 0);
    assert!(!p.root_only);
}

#[test]
fn sixteen_part_performance_passes_through_unchanged() {
    let perf = Performance {
        name: "modern".into(),
        category: String::new(),
        split_point: Some(60),
        parts: (0..16)
            .map(|i| part(&format!("Q{i}"), i % 2 == 0))
            .collect(),
    };
    let dir = tempfile::tempdir().unwrap();
    let path = write_perf(&dir, "modern", &perf);
    let loaded = preset::load_performance(&path).unwrap();
    assert_eq!(loaded.parts.len(), 16, "16-part file must not be touched");
    for i in 0..16 {
        assert_eq!(loaded.parts[i].patch_name, format!("Q{i}"));
        assert_eq!(loaded.parts[i].enabled, i % 2 == 0, "order and flags preserved");
    }
    assert_eq!(loaded.split_point, Some(60));
}
