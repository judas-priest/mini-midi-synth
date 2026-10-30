//! Master transport (#32): Play/Rec/Stop drive drums + looper together.

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

/// Record a minimal loop so the looper can play (Idle + empty loop won't start).
fn record_loop(e: &mut SynthEngine) {
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn tc3_default_target_is_non_drum_mode() {
    let e = engine();
    assert_eq!(e.seq_target_atom().load(Ordering::Relaxed), 1);
}

#[test]
fn tc1_master_record_arms_both() {
    let mut e = engine();
    assert!(!e.drum_engine.sequencer.playing);
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "drums auto-play on master rec");
    assert!(matches!(e.looper.state, LooperState::Recording));
}

#[test]
fn tc2_master_record_writes_both() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    // drum hit (ch9, note 40 -> slot 4): пишется в steps[SLOT][STEP] —
    // steps[slot] — линия инструмента, второй индекс — шаг. Без tick_block
    // phase_acc = 0, квант даёт шаг (0+16-1)%16 = 15 (just-passed).
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 40 });
    let hit = e.drum_engine.sequencer.patterns
        [e.drum_engine.sequencer.current_pattern as usize]
        .steps[4]
        .iter()
        .any(|s| s.velocity > 0);
    assert!(hit, "pad note 40 must land in drum pattern slot 4 (any step)");
    // Драмы в лупер НЕ пишутся (дубили удары сетки на реплее — 2026-10-31)
    assert_eq!(e.looper.len(), 0, "drum hits must not enter the looper");
    // pitched note (ch0) lands in the looper. Атомы публикуются только в
    // update_atoms (per-sample из tick) — без tick_block они нулевые,
    // поэтому проверяем len() (увеличивается синхронно в record_event).
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(e.looper.len() > 0, "note must be in the looper");
}

#[test]
fn tc3_master_record_off_finishes_both() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(!e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "drums keep playing after disarm");
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn tc4_master_play_toggle() {
    let mut e = engine();
    record_loop(&mut e);
    let _ = e.handle_control(ControlEvent::LooperTogglePlay); // -> Idle
    assert!(!e.drum_engine.sequencer.playing);

    let _ = e.handle_control(ControlEvent::MasterPlay);
    assert!(e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Playing));

    let _ = e.handle_control(ControlEvent::MasterPlay);
    assert!(!e.drum_engine.sequencer.playing);
    assert_eq!(e.drum_engine.sequencer.current_step, 0, "stop resets position");
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn tc5_master_stop_from_recording() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord); // both recording
    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(!e.drum_engine.sequencer.playing);
    assert_eq!(e.drum_engine.sequencer.current_step, 0);
    assert!(matches!(e.looper.state, LooperState::Idle), "finish layer then stop");
}

#[test]
fn tc1_pads_are_always_master_any_target() {
    for target in [0u8, 1, 2] {
        let mut e = engine();
        e.seq_target_atom().store(target, Ordering::Relaxed);
        // 83 = master rec arm
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
        assert!(e.drum_engine.sequencer.recording, "target {target}");
        assert!(matches!(e.looper.state, LooperState::Recording), "target {target}");
        // 81 = master stop
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
        assert!(!e.drum_engine.sequencer.playing, "target {target}");
        assert!(matches!(e.looper.state, LooperState::Idle), "target {target}");
        // 82 = master play toggle
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
        assert!(e.drum_engine.sequencer.playing, "target {target}");
        assert!(matches!(e.looper.state, LooperState::Idle), "82 must not start empty looper, target {target}");
    }
}

#[test]
fn tc1b_key_123_is_always_master() {
    for target in [0u8, 1] {
        let mut e = engine();
        e.seq_target_atom().store(target, Ordering::Relaxed);
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
        assert!(e.drum_engine.sequencer.playing, "target {target}");
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
        assert!(!e.drum_engine.sequencer.playing, "target {target}");
    }
}

#[test]
fn tc9_master_stop_idle_looper_stays_idle() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(matches!(e.looper.state, LooperState::Idle), "stop must not start an idle looper");
    assert!(!e.drum_engine.sequencer.playing);
}

#[test]
fn tc2_target_still_gates_drum_note_mode() {
    // target=0: note 40 on channel 0 triggers drums (drum_mode)
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    // transport untouched by a plain pad note
    assert!(!e.drum_engine.sequencer.playing);
    // target=1: same note goes to the part synth path (no panic, no transport)
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Idle));
}
