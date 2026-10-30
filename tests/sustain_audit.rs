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
fn cc120_kills_sound_immediately() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE, "no sound to kill");
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    // No settle: All Sound Off must be audible silence within two blocks
    let right_after = render_peak(&mut e, 2);
    assert!(right_after < SILENCE, "CC120 left a release tail (peak {right_after})");
}

#[test]
fn cc123_keeps_release_tail_then_silences() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    // All Notes Off is release-style: the tail must still be audible right after
    let tail = render_peak(&mut e, 2);
    assert!(tail > SILENCE, "CC123 lost its release tail (peak {tail})");
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC123 left ringing (peak {after})");
}

#[test]
fn cc120_on_silent_engine_is_noop() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    let peak = render_peak(&mut e, 10);
    assert!(peak < SILENCE, "CC120 on a silent engine produced sound (peak {peak})");
}

#[test]
fn patch_switch_seamless_tail_and_pedal_up_releases() {
    let mut e = engine_with_init_patch();
    // pedal down, play a note, release key (pedal holds it)
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 4) > SILENCE);
    // switch patch (seamless): pedal-held voice KEEPS SUSTAINING.
    // Дискриминатор против kill-семантики: kill = voice в release → после
    // settle (8s) тишина; seamless = sustain продолжается → громко.
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "patch switch killed a pedal-held voice (it should keep sustaining)");
    // pedal up: everything releases
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn held_key_keeps_sounding_across_patch_switch() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    // key STILL HELD: kill-семантика выпустила бы голос в release → после
    // settle тишина; seamless → голос продолжает сустейниться
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "held key was killed by patch switch");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn program_change_seamless_tail() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 1 });
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "program change killed a pedal-held voice");
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn new_notes_sound_after_patch_switch() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let sounded = render_peak(&mut e, 20);
    assert!(sounded > SILENCE, "new patch produced no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    // grand_piano has ~5s decay — assert DECAY, not absolute silence
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < sounded / 10.0);
}

#[test]
fn panic_still_kills_after_seamless_switch() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    assert!(render_peak(&mut e, 4) < SILENCE, "CC120 must still hard-kill");
}

#[test]
fn latch_mismatch_releases_latch_voices() {
    let mut e = engine_with_init_patch();
    // LoadPatch с play_mode=3 (latch): берём второй фабричный патч и правим
    // параметр (ключ "play_mode" — см. src/synth/tests.rs:750 прецедент)
    let mut latch_patch = second_patch();
    latch_patch.params.insert("play_mode".into(), 3.0);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &latch_patch));
    // аккорд: две ноты, отпускание — latch держит
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 64 });
    assert!(render_peak(&mut e, 4) > SILENCE, "latch must hold the chord");
    // switch to non-latch patch: latch voices released
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    // release tail length depends on the new patch's release — assert DECAY,
    // not absolute silence (robust against long-release factory patches);
    // если first non-init factory patch окажется latch-ным (маловероятно) —
    // взять другой патч явно
    let right_after = render_peak(&mut e, 20);
    assert!(right_after > SILENCE, "release tail should still be decaying");
    settle(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 20) < right_after / 10.0,
        "latch voices must release on latch-mismatch switch (peak {} -> {})",
        right_after, render_peak(&mut e, 20));
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
fn mute_gates_the_mix_within_10ms() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to mute");
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    // Discard the ~5 ms fade window (240 samples @48k = 8 blocks of 32);
    // everything after it must be gated to zero even though the voice is
    // still in its 0.3 s release.
    let _ = render_peak(&mut e, 8);
    let silenced = render_peak(&mut e, 16);
    assert!(silenced < SILENCE, "muted part still sounding ~10ms after mute (peak {silenced})");
}

#[test]
fn unmute_restores_sound() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    let _ = render_peak(&mut e, 200);
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: false });
    // The release tail (0.3 s) is still alive; fade-in must bring it back
    let back = render_peak(&mut e, 200);
    assert!(back > SILENCE, "unmute did not restore sound (peak {back})");
}

#[test]
fn mute_toggles_on_silent_engine_are_noop() {
    let mut e = engine_with_init_patch();
    for _ in 0..5 {
        let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
        let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: false });
    }
    let peak = render_peak(&mut e, 50);
    assert!(peak < SILENCE, "mute toggling produced sound (peak {peak})");
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
fn out_of_range_program_change_is_seamless() {
    let mut e = SynthEngine::new(SR); // no set_patches: preset_params_cache is empty
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 200 }); // out of range
    // Seamless policy (TECHDEBT #28): out-of-range PC changes nothing —
    // the pedal-held note keeps sustaining; pedal-up releases it.
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "out-of-range PC must not kill a held voice");
    // key-off (pedal still holds) → pedal-up → now it must release
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    pedal_up(&mut e);
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "key-off + pedal-up must release it (peak {after})");
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
        root_only: true,
    };
    let json = serde_json::to_string(&orig).expect("serialize");
    let back: PartConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.pan, -0.5);
    assert_eq!(back.transpose, -12);
    assert!(back.mute);
    assert!(back.root_only);
    assert_eq!(back.vel_min, 40);
    assert_eq!(back.vel_max, 100);
    // Old file without the new fields must deserialize to neutral defaults
    let old_json = r#"{"patch_name":"init","enabled":true,"volume":0.8,"key_low":0,"key_high":127,"param_overrides":{},"sf2_mode":false,"sf2_program":0}"#;
    let legacy: PartConfig = serde_json::from_str(old_json).expect("legacy deserialize");
    assert_eq!(legacy.pan, 0.0);
    assert_eq!(legacy.transpose, 0);
    assert!(!legacy.mute);
    assert!(!legacy.root_only);
    assert_eq!(legacy.vel_min, 1);
    assert_eq!(legacy.vel_max, 127);
}

#[test]
fn init_performance_exists_and_loads() {
    let perf = mini_midi_synth::preset::init_performance();
    assert_eq!(perf.parts.len(), 1);
    let p = &perf.parts[0];
    assert!(p.enabled);
    assert!(!p.sf2_mode);
    assert_eq!(p.key_low, 0);
    assert_eq!(p.key_high, 127);
    let json = serde_json::to_string(&perf).expect("serialize");
    let back: mini_midi_synth::preset::Performance = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.name, "(Init)");
}

#[test]
fn pad_transport_controls_drum_seq() {
    let mut e = engine_with_init_patch();
    e.seq_target_atom().store(0, Ordering::Relaxed); // pad transport: drums
    // NOTE: record_hit fires only while recording AND playing (drum.rs:799);
    // rec auto-plays a stopped transport, so playback starts with the record arm.
    // record on (B5)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert_eq!(e.drum_engine.rec_atom().load(Ordering::Relaxed), 1, "B5 did not start recording");
    assert_eq!(e.drum_engine.play_atom().load(Ordering::Relaxed), 1, "B5 did not auto-start playback");
    // play toggle (A#5): off, then back on so record_hit actually captures
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert_eq!(e.drum_engine.play_atom().load(Ordering::Relaxed), 0, "A#5 did not stop playback");
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
    e.seq_target_atom().store(0, Ordering::Relaxed); // pad transport: drums
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
#[test]
fn looper_records_all_sounding_parts() {
    let mut e = engine_with_init_patch();
    // Part A: full range (default). Part B: 60-127.
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 1, volume: 1.0 });
    // Record: two notes, one per part's range. Events land at distinct looper
    // ticks, so a few blocks must be rendered between them (all events sent
    // with no time in between would collapse onto tick 0 and cancel out).
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 36, velocity: 127 });
    let _ = render_peak(&mut e, 20);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 36 });
    let _ = render_peak(&mut e, 20);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 84, velocity: 127 });
    let _ = render_peak(&mut e, 20);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 84 });
    // NOTE: LooperRecord is a no-op while already Recording (looper.rs:220-235);
    // stopping uses LooperStopRecord, which moves Recording -> Playing.
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    // During playback both notes must sound again
    let peak = render_peak(&mut e, 6000);
    assert!(peak > 0.05, "looper playback produced no sound (peak {peak})");

    // Negative: a note outside every part's range records nothing -> silent playback.
    let mut e2 = engine_with_init_patch();
    let _ = e2.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e2.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e2.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 100 });
    let _ = e2.handle_control(ControlEvent::LooperRecord);
    e2.handle_event(MidiEvent::NoteOn { channel: 0, note: 110, velocity: 127 }); // outside 0-59 and 60-100
    let _ = render_peak(&mut e2, 20);
    e2.handle_event(MidiEvent::NoteOff { channel: 0, note: 110 });
    let _ = e2.handle_control(ControlEvent::LooperStopRecord);
    let silent_peak = render_peak(&mut e2, 4000);
    assert!(silent_peak < 1e-3, "out-of-range note was recorded (peak {silent_peak})");
}

fn looper_recorded_part_ids(e: &mut SynthEngine) -> Vec<u8> {
    e.looper
        .display()
        .lock()
        .expect("looper display lock")
        .events
        .iter()
        .map(|ev| ev.part_id)
        .collect()
}

#[test]
fn looper_source_part_records_only_active_part() {
    // Mix source (default) — covered by looper_records_all_sounding_parts.
    // Part source: a note sounding on TWO parts records only the active
    // part's events.
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 0, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 1, volume: 1.0 });
    // Negative control first: with the default Mix source BOTH parts are
    // recorded for a note in the overlap.
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 4);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 4);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    let mix_parts = looper_recorded_part_ids(&mut e);
    assert!(mix_parts.contains(&0) && mix_parts.contains(&1),
        "Mix source must record every sounding part: {mix_parts:?}");

    // Part source (active part = 0): same overlap note -> only part 0 events.
    let _ = e.handle_control(ControlEvent::LooperClear);
    let _ = e.handle_control(ControlEvent::LooperSetSource { mix: false });
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 4);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 4);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    let part_parts = looper_recorded_part_ids(&mut e);
    assert!(!part_parts.is_empty(), "Part source recorded nothing");
    assert!(part_parts.iter().all(|&p| p == 0),
        "Part source must record only the active part: {part_parts:?}");

    // Back to Mix mid-session: both parts again.
    let _ = e.handle_control(ControlEvent::LooperClear);
    let _ = e.handle_control(ControlEvent::LooperSetSource { mix: true });
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 4);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 4);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    let again = looper_recorded_part_ids(&mut e);
    assert!(again.contains(&0) && again.contains(&1),
        "toggling back to Mix must record every sounding part: {again:?}");
}

#[test]
fn held_key_survives_pedal_up() {
    let mut e = engine_with_init_patch();
    // pedal-up on an idle engine must not panic
    pedal_up(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    // pedal-up while the KEY IS STILL HELD: note must keep sounding.
    // Discriminator: settle first — a killed voice decays below SILENCE
    // (release 0.3s), a held one stays at sustain level forever.
    pedal_up(&mut e);
    settle(&mut e);
    let held = render_peak(&mut e, 200);
    assert!(held > SILENCE, "pedal-up killed a held key (peak {held})");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "key-off after pedal-up left ringing (peak {after})");
}

#[test]
fn repedal_articulation_keeps_held_key() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    // re-pedal: down, up, down, up — key held the whole time
    pedal_down(&mut e);
    pedal_up(&mut e);
    pedal_down(&mut e);
    pedal_up(&mut e);
    settle(&mut e);
    let held = render_peak(&mut e, 200);
    assert!(held > SILENCE, "re-pedal killed a held key (peak {held})");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE);
}

#[test]
fn multi_held_keys_pedal_up_then_individual_offs() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 200) > SILENCE, "pedal-up killed held keys");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 48 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) > SILENCE, "note 60 should still sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE);
}

#[test]
fn duplicate_note_on_then_pedal_up_releases_once() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    pedal_up(&mut e);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE, "duplicate note-on left ringing");
}

#[test]
fn panic_after_held_pedal_up_kills_all() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    pedal_up(&mut e);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE, "CC123 left ringing");
    // a later pedal cycle must not resurrect anything
    pedal_down(&mut e);
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 200) < SILENCE, "pedal cycle resurrected a note");
}

#[test]
fn drum_record_with_out_of_range_remap_target_does_not_panic() {
    use mini_midi_synth::synth::ControlEvent;
    let mut e = engine_with_init_patch();
    // remap pad slot 0 (note 36) to target 30 (outside 36..=51 — legal SF2 target)
    let mut remap = [36u8; 16];
    remap[0] = 30;
    let _ = e.handle_control(ControlEvent::DrumSetRemap { table: Box::new(remap) });
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    // live hit on ch10, note 36 -> remapped target 30 -> slot math 30-36
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 36, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 36 });
    // must not panic (debug overflow) and must not record a bogus hit
    assert!(render_peak(&mut e, 10) >= 0.0);
}

#[test]
fn rec_on_stopped_transport_auto_plays() {
    let mut e = engine_with_init_patch();
    e.seq_target_atom().store(0, Ordering::Relaxed); // pad transport: drums
    assert!(!e.drum_engine.sequencer.playing);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing, "rec must auto-start play");
    assert!(e.drum_engine.sequencer.recording);
    // toggle off while playing: auto-play must NOT fire
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(!e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "play persists after rec-off");
}

// ── Sustain-atom coherence on MIDI-seq stop/load ───────────────

fn empty_seq_data() -> Box<mini_midi_synth::synth::midi_player::MidiSeqData> {
    Box::new(mini_midi_synth::synth::midi_player::MidiSeqData {
        ticks_per_beat: 480,
        total_ticks: 480,
        events: Vec::new(),
        track_names: Vec::new(),
        track_channels: Vec::new(),
    })
}

#[test]
fn midi_seq_stop_lifts_sustain_atom() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held by pedal");
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    assert!(!e.sustain_atom().load(Ordering::Relaxed),
        "seq stop left sustain atom ON");
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "seq stop left pedal-held note ringing (peak {after})");
}

#[test]
fn midi_seq_stop_stops_sustaining_new_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    // The stop force-lifted the pedal everywhere: a NEW note must not be
    // held after its key-off (no new CC64 edge arrives in between).
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "seq stop left DSP sustaining new notes (peak {after})");
}

#[test]
fn midi_seq_load_lifts_sustain_and_stops_sustaining_new_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: empty_seq_data() });
    assert!(!e.sustain_atom().load(Ordering::Relaxed),
        "seq load left sustain atom ON");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "seq load left DSP sustaining new notes (peak {after})");
}

// ── Static pedal-up release (TECHDEBT #7, restored from 9a200a4) ──

#[test]
fn pedal_rel_setting_shortens_pedal_up_tail() {
    // Static SR 0.03s: pedal-up tail is far below the patch's amp_release
    // (0.3s on init) — ≈ silent at ~100ms post-release.
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { seconds: Some(0.03) });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held");
    pedal_up(&mut e);
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..150 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~100ms
    let peak = render_peak(&mut e, 10);
    assert!(peak < 0.15, "SR did not shorten the pedal-up tail (peak {peak})");
}

#[test]
fn pedal_rel_off_keeps_full_default_tails() {
    // SR off: pedal-up takes the patch's full amp_release (0.3s on init)
    // — still audible at ~250ms post-release.
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held");
    pedal_up(&mut e);
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..375 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~250ms
    let peak = render_peak(&mut e, 10);
    assert!(peak > 0.15, "default tail lost with SR off (peak {peak})");
}

#[test]
fn pedal_rel_held_keys_keep_patch_release_on_their_own_key_up() {
    // The override must touch only pedal-released voices: a key-held voice
    // survives pedal-up and keeps the PATCH release when its key goes up.
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { seconds: Some(0.03) });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    pedal_up(&mut e); // key still held — voice must keep sounding untouched
    assert!(render_peak(&mut e, 20) > SILENCE, "pedal-up killed a key-held voice");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // patch release 0.3s
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..375 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~250ms
    let peak = render_peak(&mut e, 10);
    assert!(peak > 0.15, "key-held voice lost its patch release after SR pedal-up (peak {peak})");
}

#[test]
fn sf2_release_scale_event_updates_sampler_scale() {
    // Unit-level: the ControlEvent plumbing reaches SamplerEngine and the
    // scale persists (it is re-applied on every rustysynth rebuild, which is
    // where it matters — no SF2 is loaded here, so no audio assertion).
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSf2ReleaseScale { scale: 2.5 });
    assert!((e.sampler_release_scale() - 2.5).abs() < 1e-6, "scale did not reach the sampler");
    let _ = e.handle_control(ControlEvent::SetSf2ReleaseScale { scale: 1.0 });
    assert!((e.sampler_release_scale() - 1.0).abs() < 1e-6, "scale reset did not stick");
}

// ── Mono legato (TECHDEBT 14.5) ────────────────────────────────

fn legato_engine(play_mode: f32) -> SynthEngine {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("play_mode".to_string(), play_mode);
    patch.params.insert("amp_attack".to_string(), 0.2);
    patch.params.insert("amp_decay".to_string(), 0.0); // review patch: stable sustain plateau
    patch.params.insert("amp_sustain".to_string(), 0.7);
    patch.params.insert("amp_release".to_string(), 0.1);
    patch.params.insert("filter_env_amount".to_string(), 0.0);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    e
}

/// Sustain level reached, then peak of the 150 ms window after the second
/// note-on, relative to the sustain peak.
fn reattack_ratio(e: &mut SynthEngine) -> f32 {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(e, 600); // 0.4 s — attack (0.2 s) done, at sustain
    let before = render_peak(e, 100);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    let after = render_peak(e, 225); // 150 ms window
    after / before
}

#[test]
fn mono_legato_keeps_sustain_level_through_note_change() {
    let mut e = legato_engine(2.0); // Mono-ST: no re-attack while a key is held
    let ratio = reattack_ratio(&mut e);
    // CALIBRATED: measured legato ratio = 0.99994 (flat sustain); the mono
    // re-attack measures 1.140. Threshold 1.05 sits mid-gap between them.
    assert!(
        ratio < 1.05,
        "legato re-attacked the envelope (peak ratio {ratio})"
    );
}

#[test]
fn mono_retrigger_restarts_attack() {
    let mut e = legato_engine(1.0); // Mono: full re-attack
    let ratio = reattack_ratio(&mut e);
    // CALIBRATED: measured re-attack ratio = 1.140 (Sqrt-shape IIR toward the
    // 1.3 overshoot target from sustain — the real bump is ~14 %, not the
    // plan's estimated 25 %+); legato measures 1.000. Threshold 1.10 sits
    // mid-gap between the two modes.
    assert!(
        ratio > 1.10,
        "mono retrigger did not restart the attack (peak ratio {ratio})"
    );
}

#[test]
fn legato_with_no_held_key_is_normal_attack() {
    let mut e = legato_engine(2.0);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let before = render_peak(&mut e, 100); // mid-attack: well below the peak
    let _ = render_peak(&mut e, 600);      // reach sustain
    // Let the first note go fully, then play again: single note = full attack
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 600); // release 0.1 s → idle
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    let after = render_peak(&mut e, 225);
    // CALIBRATED: measured mid-attack/window ratio = 1.793 (plan guessed 1.8+;
    // it lands just under, so the threshold is 1.7).
    assert!(
        after > before * 1.7,
        "first note after silence must attack normally (mid-attack {before}, re-attack window {after})"
    );
}

// ── Arp latch (TECHDEBT 14.4) ──────────────────────────────────

fn latched_arp_engine() -> SynthEngine {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("arp_enabled".to_string(), 1.0);
    patch.params.insert("arp_rate".to_string(), 2.0); // 1/16 ≈ 125 ms steps @120 BPM
    patch.params.insert("arp_gate".to_string(), 0.5);
    patch.params.insert("arp_latch".to_string(), 1.0);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    e
}

#[test]
fn arp_latch_keeps_playing_after_key_release() {
    let mut e = latched_arp_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "arp produced no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let latched_peak = render_peak(&mut e, 300); // ~200 ms — spans further steps
    assert!(
        latched_peak > SILENCE,
        "latch did not keep the arp alive after key release (peak {latched_peak})"
    );
    // Second latch-toggle press (latch off) stops the frozen set
    let _ = e.handle_control(ControlEvent::SetArpLatch { latch: false });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "latch-off left sound ringing (peak {after})");
}

#[test]
fn arp_latch_new_chord_takes_over() {
    let mut e = latched_arp_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 100);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 100);
    assert!(render_peak(&mut e, 100) > SILENCE, "set did not freeze");
    // New chord replaces the frozen set without stopping playback
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let _ = render_peak(&mut e, 50);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    let peak = render_peak(&mut e, 300);
    assert!(peak > SILENCE, "new chord broke the latched arp (peak {peak})");
    let _ = e.handle_control(ControlEvent::SetArpLatch { latch: false });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "latch-off left sound ringing (peak {after})");
}

#[test]
fn panic_cc_clears_latched_arp() {
    let mut e = latched_arp_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 100);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 100) > SILENCE, "set did not freeze");
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC120 left the latched arp ringing (peak {after})");
}

#[test]
fn arp_latch_persists_through_patch_load() {
    let e = latched_arp_engine();
    assert!(e.arpeggiator.latch_enabled(), "arp_latch param did not sync on patch load");
    // Absent key defaults to off
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists")
        .clone();
    let mut e2 = SynthEngine::new(SR);
    e2.set_patches(patches);
    let _ = e2.handle_control(ControlEvent::load_patch_from(0, &patch));
    assert!(!e2.arpeggiator.latch_enabled(), "default latch must be off");
}

