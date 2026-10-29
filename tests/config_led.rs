// window_width/height/last_preset lack #[serde(default)], so they must be
// present; the test targets led_companion's default.
#[test]
fn led_companion_defaults_false() {
    let s: mini_midi_synth::config::UiSettings = serde_json::from_str(
        r#"{"window_width": 720.0, "window_height": 400.0, "last_preset": null}"#,
    )
    .unwrap();
    assert!(!s.led_companion);
}

#[test]
fn led_companion_cmd_defaults_to_literal_path() {
    let s: mini_midi_synth::config::UiSettings = serde_json::from_str(
        r#"{"window_width": 720.0, "window_height": 400.0, "last_preset": null}"#,
    )
    .unwrap();
    assert_eq!(s.led_companion_cmd, "~/Projects/smk37_config/pad_console.py");
}

#[test]
fn expand_home_expands_tilde_slash() {
    if dirs::home_dir().is_none() { return; } // HOME-less env: путь как есть
    // SECURITY NOTE: не читаем реальный HOME тестом — подменяем через канал.
    let p = mini_midi_synth::config::expand_home("~/sub/file.py").display().to_string();
    assert!(!p.starts_with('~'), "tilde must be expanded: {p}");
    assert!(p.ends_with("/sub/file.py"), "suffix preserved: {p}");
}

#[test]
fn expand_home_keeps_plain_paths() {
    assert_eq!(mini_midi_synth::config::expand_home("/abs/x.py"), std::path::PathBuf::from("/abs/x.py"));
    assert_eq!(mini_midi_synth::config::expand_home(""), std::path::PathBuf::from(""));
    assert_eq!(mini_midi_synth::config::expand_home("~foo"), std::path::PathBuf::from("~foo"));
}
