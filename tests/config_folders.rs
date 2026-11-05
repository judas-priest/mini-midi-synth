//! #57: PerfFolder (setlists/tracks) — serde contract.
use mini_midi_synth::config::{self, PerfFolder};

/// Минимальный legacy-JSON UiSettings: обязательные поля (window_width,
/// window_height, last_preset — без #[serde(default)]) + ни одного нового
/// ключа #57.
const LEGACY_UI: &str =
    r#"{"window_width":720.0,"window_height":400.0,"last_preset":null}"#;

#[test]
fn tc4_folders_roundtrip() {
    // UiSettings::default() НЕ существует — собираем через serde
    let mut ui: config::UiSettings = serde_json::from_str(LEGACY_UI).unwrap();
    ui.perf_folders = vec![
        PerfFolder { name: "Track 1".into(), perfs: vec!["Bass".into(), "Piano".into()] },
        PerfFolder { name: "Track 2".into(), perfs: vec!["Piano".into(), "Synth".into()] },
    ];
    ui.active_folder = Some(1);
    let json = serde_json::to_string(&ui).unwrap();
    let back: config::UiSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.perf_folders.len(), 2);
    assert_eq!(back.perf_folders[0].name, "Track 1");
    assert_eq!(back.perf_folders[1].perfs, vec!["Piano".to_string(), "Synth".to_string()]);
    assert_eq!(back.active_folder, Some(1));
}

#[test]
fn tc4_legacy_config_without_folders_gets_defaults() {
    let ui: config::UiSettings = serde_json::from_str(LEGACY_UI).unwrap();
    assert!(ui.perf_folders.is_empty());
    assert_eq!(ui.active_folder, None);
}
