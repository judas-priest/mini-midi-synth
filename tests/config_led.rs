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
