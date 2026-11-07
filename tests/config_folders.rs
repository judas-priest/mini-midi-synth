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

#[test]
fn tc1_resolve_keeps_tombstones() {
    use mini_midi_synth::config::resolve_folder_entries;
    let pl = vec![
        ("A".to_string(), std::path::PathBuf::from("/a.json")),
        ("B".to_string(), std::path::PathBuf::from("/b.json")),
        ("C".to_string(), std::path::PathBuf::from("/c.json")),
    ];
    let folder = PerfFolder { name: "T".into(),
        perfs: vec!["B".into(), "Ghost".into(), "A".into()] };
    let r = resolve_folder_entries(&pl, Some(&folder));
    assert_eq!(r.len(), 3); // tombstone НЕ выбрасывается — индексы стабильны
    assert_eq!(r[0], ("B".to_string(), Some(std::path::PathBuf::from("/b.json"))));
    assert_eq!(r[1], ("Ghost".to_string(), None));
    assert_eq!(r[2], ("A".to_string(), Some(std::path::PathBuf::from("/a.json"))));
}

#[test]
fn tc2_no_folder_resolves_to_full_list() {
    use mini_midi_synth::config::resolve_folder_entries;
    let pl = vec![("(Init)".to_string(), std::path::PathBuf::from("/i.json"))];
    let r = resolve_folder_entries(&pl, None);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].0, "(Init)");
}

// QW10f: keybinds Option<HashMap> — "user unbound everything" (empty map)
// must be distinguishable from "no key in config" (defaults).

/// Config with `"keybinds": {}` → Some(empty map), NOT defaults.
#[test]
fn qw10f_keybinds_empty_map_is_some_empty() {
    let json = r#"{"window_width":720.0,"window_height":400.0,"last_preset":null,"keybinds":{}}"#;
    let ui: config::UiSettings = serde_json::from_str(json).unwrap();
    let binds = ui.keybinds.expect("empty keybinds map must deserialize as Some, not None");
    assert!(binds.is_empty(), "expected the explicit empty set, got {} binds", binds.len());
}

/// Config without the `keybinds` key → None (defaults apply downstream).
#[test]
fn qw10f_keybinds_absent_key_is_none() {
    let ui: config::UiSettings = serde_json::from_str(LEGACY_UI).unwrap();
    assert_eq!(ui.keybinds, None);
}

/// Roundtrip pin: a non-empty set survives save/load as Some(map).
#[test]
fn qw10f_keybinds_nonempty_roundtrip() {
    let json = r#"{"window_width":720.0,"window_height":400.0,"last_preset":null,"keybinds":{"F1":"SwitchPart(0)"}}"#;
    let ui: config::UiSettings = serde_json::from_str(json).unwrap();
    let out = serde_json::to_string(&ui).unwrap();
    let back: config::UiSettings = serde_json::from_str(&out).unwrap();
    assert_eq!(back.keybinds.as_ref().map(|m| m.get("F1").map(String::as_str)), Some(Some("SwitchPart(0)")));
}
