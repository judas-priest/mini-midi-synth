//! Sustain-stick regression: no note may outlive its pedal/panic/preset-switch.
//!
//! All 15 cases green since the remediation plan landed
//! (docs/superpowers/plans/2026-10-27-eternal-sustain-remediation.md).

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;

fn render_peak(engine: &mut SynthEngine, blocks: usize) -> f32 {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
    }
    peak
}

fn settle(engine: &mut SynthEngine) {
    let _ = render_peak(engine, 6000);
}

fn engine_with_init_patch() -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();
    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);
    let _ = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = engine.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    engine
}

fn second_patch() -> preset::Patch {
    let patches = preset::load_all_patches();
    patches
        .iter()
        .find(|p| !p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone()
}

fn init_patch_mut() -> preset::Patch {
    preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists")
}

fn pedal_down(e: &mut SynthEngine) {
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 });
}
fn pedal_up(e: &mut SynthEngine) {
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 0 });
}

#[test]
fn pedal_up_releases_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "pedal did not hold the note");
    pedal_up(&mut e);
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "pedal-up left sound ringing (peak {after})");
}

#[test]
fn all_notes_off_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held by pedal");
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "AllNotesOff left pedal note ringing (peak {after})");
    assert!(!e.sustain_atom().load(std::sync::atomic::Ordering::Relaxed),
        "AllNotesOff left sustain atom ON");
}

#[test]
fn cc123_panic_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC123 left pedal note ringing (peak {after})");
}

#[test]
fn cc120_panic_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC120 left pedal note ringing (peak {after})");
}

#[test]
fn patch_switch_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "patch switch left pedal note ringing (peak {after})");
}

#[test]
fn program_change_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 1 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "ProgramChange left pedal note ringing (peak {after})");
}

#[test]
fn transposed_part_note_off_stops_sound() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to stop");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "transposed note-off did not match note-on (peak {after})");
}

#[test]
fn arp_transposed_note_off_stops_sound() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    // Patch params live in a BTreeMap<String, f32>; PatchParams::from_map
    // reads exactly these keys.
    let mut patch = init_patch_mut();
    patch.params.insert("arp_enabled".to_string(), 1.0);
    patch.params.insert("arp_rate".to_string(), 0.0); // 1/4 note; first step fires at tick_counter==0
    patch.params.insert("arp_gate".to_string(), 0.5);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "arp produced no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "arp left a voice hanging (peak {after})");
}

#[test]
fn mono_pedal_up_clears_note_stack() {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("play_mode".to_string(), 1.0); // mono
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    pedal_up(&mut e);
    settle(&mut e);
    // Stale mono stack would resurrect A (60) when B (62) is pressed/released
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "stale mono stack resurrected a note (peak {after})");
}

#[test]
fn part_disable_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: false });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "part disable left pedal note ringing (peak {after})");
}

#[test]
fn mute_releases_sounding_notes() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to mute");
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "mute left notes sounding (peak {after})");
}

#[test]
fn midi_learn_rejects_cc64() {
    use mini_midi_synth::cc_map::{CcBindingRaw, CcMap, CcMapRaw};
    use std::collections::BTreeMap;
    let mut bindings = BTreeMap::new();
    bindings.insert(64u8, CcBindingRaw {
        param_key: "master_volume".to_string(),
        min_val: 0.0, max_val: 1.0, logarithmic: false,
        scope: mini_midi_synth::cc_map::ParamScope::Global,
    });
    let config = mini_midi_synth::config::Config::default();
    let mut config = config;
    config.cc_map = Some(CcMapRaw { bindings });
    let map = CcMap::from_config(&config);
    assert!(map.bindings[64].is_none(), "CC64 must never be a parameter binding");
}

#[test]
fn repeated_cc123_is_idempotent() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    pedal_up(&mut e); // idempotent too: already up via the handler
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "repeated CC123 left sound (peak {after})");
}

#[test]
fn program_change_with_empty_patch_cache_cleans_voices() {
    let mut e = SynthEngine::new(SR); // no set_patches: preset_params_cache is empty
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 200 }); // out of range
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "empty-cache ProgramChange left sound (peak {after})");
}

#[test]
fn performance_part_config_roundtrip() {
    use mini_midi_synth::preset::PartConfig;
    let orig = PartConfig {
        patch_name: "init".to_string(),
        enabled: true,
        volume: 0.75,
        key_low: 0,
        key_high: 127,
        param_overrides: Default::default(),
        sf2_mode: false,
        sf2_program: 0,
        pan: -0.5,
        transpose: -12,
        mute: true,
        vel_min: 40,
        vel_max: 100,
    };
    let json = serde_json::to_string(&orig).expect("serialize");
    let back: PartConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.pan, -0.5);
    assert_eq!(back.transpose, -12);
    assert!(back.mute);
    assert_eq!(back.vel_min, 40);
    assert_eq!(back.vel_max, 100);
    // Old file without the new fields must deserialize to neutral defaults
    let old_json = r#"{"patch_name":"init","enabled":true,"volume":0.8,"key_low":0,"key_high":127,"param_overrides":{},"sf2_mode":false,"sf2_program":0}"#;
    let legacy: PartConfig = serde_json::from_str(old_json).expect("legacy deserialize");
    assert_eq!(legacy.pan, 0.0);
    assert_eq!(legacy.transpose, 0);
    assert!(!legacy.mute);
    assert_eq!(legacy.vel_min, 1);
    assert_eq!(legacy.vel_max, 127);
}

#[test]
fn dsp_sustain_release_override() {
    // Init patch: amp_release 0.3s, amp_sustain 0.7, IIR sqrt release.
    // Override fraction 0.1 → tail = 0.1 * 0.3 = 0.03s: after ~100ms the
    // envelope is idle, far below the patch's natural decay (≈ 0.50).
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { tail: Some(0.1) });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    pedal_up(&mut e); // pedal-held note released with the 0.05s override
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..150 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~100ms
    let peak = render_peak(&mut e, 10);
    assert!(peak < 0.15, "override release did not shorten the tail (peak {peak})");
}

#[test]
fn key_release_uses_patch_release() {
    // Fraction 0.1 (tail 0.03s) vs patch release 0.3s. At ~250ms post-key-off the
    // override envelope is fully idle (< SILENCE, ~180ms to Idle) while the
    // patch tail still holds ≈ 0.7 * exp(-250/300) ≈ 0.30 — the assert
    // discriminates (a plain > 0.0 would pass even if the override leaked).
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { tail: Some(0.1) });
    pedal_up(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..375 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~250ms
    let peak = render_peak(&mut e, 10);
    assert!(peak > 0.15, "key release appears overridden by sustain-release (peak {peak})");
}

#[test]
fn pad_transport_controls_drum_seq() {
    let mut e = engine_with_init_patch();
    // NOTE: record_hit fires only while recording AND playing (drum.rs:799),
    // so playback is enabled before recording the kick.
    // record on (B5)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert_eq!(e.drum_engine.rec_atom().load(Ordering::Relaxed), 1, "B5 did not start recording");
    // play on (A#5) so record_hit actually captures
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert_eq!(e.drum_engine.play_atom().load(Ordering::Relaxed), 1, "A#5 did not start playback");
    // hit a kick while recording (goes through the normal drum path)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 36, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 36 });
    // record off (B5 toggle)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert_eq!(e.drum_engine.rec_atom().load(Ordering::Relaxed), 0, "B5 did not stop recording");
    // undo (E4) must revert the recorded hit
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 64, velocity: 127 });
    let pat = e.drum_engine.sequencer.current_pattern as usize;
    let any_hit: bool = e.drum_engine.sequencer.patterns[pat]
        .steps.iter().flatten()
        .any(|s| s.velocity > 0);
    assert!(!any_hit, "undo did not revert the recorded kick");
    // pause (A5)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert_eq!(e.drum_engine.play_atom().load(Ordering::Relaxed), 0, "A5 did not pause playback");
}

#[test]
fn pad_transport_notes_do_not_trigger_drums() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 64, velocity: 127 });
    let peak = render_peak(&mut e, 200);
    assert!(peak < SILENCE, "transport notes leaked into drum audio (peak {peak})");
}

#[test]
fn transpose_change_midnote_does_not_orphan_voice() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 0 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE, "no sound");
    // Change transpose WHILE the note sounds
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 7 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "transpose change orphaned the voice (peak {after})");
}

#[test]
fn transpose_change_with_pedal_held_note_releases() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held");
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 5 });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "pedal-up left orphaned voice after transpose change (peak {after})");
}

#[test]
fn latch_mode_works_with_transpose() {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("play_mode".to_string(), 3.0); // latch
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 7 });
    // Latch toggle on: voice must start
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE, "latch + transpose: no voice started");
    // Toggle off (same raw note releases the latched voice)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "latch toggle-off did not release (peak {after})");
}
