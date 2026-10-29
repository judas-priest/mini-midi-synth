//! Pad transport (bank-2 notes 81/82/83 ch9) follows seq_target: drums or looper.

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
fn pad_play_82_follows_target_drums() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    assert!(!e.drum_engine.sequencer.playing);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing, "82 must toggle drums at target=0");
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
    assert_eq!(e.drum_engine.sequencer.current_step, 0, "stop resets position");
}

#[test]
fn pad_play_82_follows_target_looper() {
    let mut e = engine();
    e.seq_target_atom().store(1, Ordering::Relaxed);
    record_loop(&mut e); // Playing
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Idle), "82 stops playing looper");
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Playing), "82 restarts looper");
    // drum transport must NOT have moved
    assert!(!e.drum_engine.sequencer.playing);
}

#[test]
fn pad_rec_83_follows_target_and_snapshots() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "rec on stopped transport auto-plays");
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(!e.drum_engine.sequencer.recording);

    e.seq_target_atom().store(1, Ordering::Relaxed);
    record_loop(&mut e);
    let _ = e.handle_control(ControlEvent::LooperTogglePlay); // -> Idle
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Recording), "83 records looper at target=1");
}

#[test]
fn pad_stop_81_follows_target() {
    let mut e = engine();
    // drums: stop + reset
    e.seq_target_atom().store(0, Ordering::Relaxed);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);

    // looper: stop only if playing; Idle must stay Idle
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Idle), "81 must not START an idle looper");
    record_loop(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn transport_notes_on_other_channels_play_notes_not_transport() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 82, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 1, note: 83, velocity: 100 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(!e.drum_engine.sequencer.recording);
}
