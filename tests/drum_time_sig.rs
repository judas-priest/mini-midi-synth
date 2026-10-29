//! Time signatures для драм-секвенсора (TECHDEBT #21).
//! Plan: docs/superpowers/plans/2026-10-29-time-signatures.md
//!
//! Подписи ограничены 2/4..4/4: MAX_STEPS = 16 (drum.rs), длина такта =
//! beats_per_bar * 4 (16-е), поэтому 5/4 и 6/4 требуют расширения массива —
//! вне скоупа (см. docs/TECHDEBT.md §21).

use mini_midi_synth::preset::DrumKit;
use mini_midi_synth::synth::{ControlEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

#[test]
fn tc1_time_sig_sets_beats_and_length() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 3 });
    {
        let seq = &e.drum_engine.sequencer;
        assert_eq!(seq.patterns[0].beats_per_bar, 3);
        assert_eq!(seq.patterns[0].length, 12);
    }
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 4 });
    let seq = &e.drum_engine.sequencer;
    assert_eq!(seq.patterns[0].beats_per_bar, 4);
    assert_eq!(seq.patterns[0].length, 16);
}

#[test]
fn tc2_time_sig_clamped_to_2_4() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 0 });
    assert_eq!(e.drum_engine.sequencer.patterns[0].beats_per_bar, 2);
    assert_eq!(e.drum_engine.sequencer.patterns[0].length, 8);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 255 });
    // MAX_STEPS=16: 5/4+ невозможно, верхний кламп 4.
    assert_eq!(e.drum_engine.sequencer.patterns[0].beats_per_bar, 4);
    assert_eq!(e.drum_engine.sequencer.patterns[0].length, 16);
}

#[test]
fn tc3_length_zero_from_ring_is_clamped_and_rt_safe() {
    // reg: record_hit делит на len (drum.rs) — length=0 из кольца = panic.
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqLength { length: 0 });
    assert_eq!(e.drum_engine.sequencer.patterns[0].length, 1);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: true });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..200 { e.tick_block(&mut l, &mut r); }
    // record_hit при length=1 (playing+recording) не должен паниковать
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    for _ in 0..200 { e.tick_block(&mut l, &mut r); }
}

#[test]
fn tc4_undo_restores_time_sig_wholesale() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 3 });
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true }); // snapshot
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 4 });
    let _ = e.handle_control(ControlEvent::DrumSeqUndo);
    let seq = &e.drum_engine.sequencer;
    assert_eq!(seq.patterns[0].length, 12);
    assert_eq!(seq.patterns[0].beats_per_bar, 3);
}

#[test]
fn tc5_legacy_kit_json_without_field_defaults_to_4() {
    let pat = mini_midi_synth::synth::drum::DrumPattern { length: 12, ..Default::default() };
    let kit = DrumKit {
        name: "legacy".into(),
        patterns: vec![pat],
        params: vec![],
        bpm: 120.0,
        swing: 0.0,
        volume: 0.8,
    };
    let mut json = serde_json::to_string(&kit).unwrap();
    // стереть новое поле — симуляция старого kit-файла
    let needle = format!(r#","beats_per_bar":{}"#, pat.beats_per_bar);
    let pos = json.find(&needle).expect("field must be serialized");
    json.replace_range(pos..pos + needle.len(), "");
    let back: DrumKit = serde_json::from_str(&json).expect("legacy kit must parse");
    assert_eq!(back.patterns[0].length, 12);
    assert_eq!(back.patterns[0].beats_per_bar, 4);
}

#[test]
fn tc6_kit_roundtrip_keeps_time_sig() {
    let pat = mini_midi_synth::synth::drum::DrumPattern {
        length: 12,
        beats_per_bar: 3,
        ..Default::default()
    };
    let kit = DrumKit {
        name: "waltz".into(),
        patterns: vec![pat],
        params: vec![],
        bpm: 120.0,
        swing: 0.0,
        volume: 0.8,
    };
    let json = serde_json::to_string(&kit).unwrap();
    let back: DrumKit = serde_json::from_str(&json).unwrap();
    assert_eq!(back.patterns[0].beats_per_bar, 3);
    assert_eq!(back.patterns[0].length, 12);
}
