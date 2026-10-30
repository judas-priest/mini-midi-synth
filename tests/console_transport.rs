//! Transport pads (bank-2 notes 81/82/83 ch9) + keys 123/124 ALWAYS drive the
//! master transport (#32 фаза 4). seq_target no longer selects a transport —
//! it only gates drum-note mode (0 = pads trigger drums on any channel).

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

#[test]
fn pad_play_82_is_master_any_target() {
    for target in [0u8, 1, 2] {
        let mut e = engine();
        e.seq_target_atom().store(target, Ordering::Relaxed);
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
        assert!(e.drum_engine.sequencer.playing, "target {target}: 82 must start master");
        e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
        assert!(!e.drum_engine.sequencer.playing);
        assert_eq!(e.drum_engine.sequencer.current_step, 0, "stop resets position");
    }
}

#[test]
fn pad_rec_83_arms_master_and_snapshots() {
    let mut e = engine();
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(e.drum_engine.sequencer.recording);
    assert!(e.drum_engine.sequencer.playing, "rec on stopped transport auto-plays");
    assert!(matches!(e.looper.state, LooperState::Recording));
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert!(!e.drum_engine.sequencer.recording);
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn pad_stop_81_stops_master() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
    // idle looper must NOT be started by stop
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn key_123_is_master() {
    let mut e = engine();
    e.seq_target_atom().store(1, Ordering::Relaxed);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
    assert!(e.drum_engine.sequencer.playing);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 123, velocity: 127 });
    assert!(!e.drum_engine.sequencer.playing);
}

#[test]
fn transport_notes_on_other_channels_play_notes_not_transport() {
    let mut e = engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 82, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 1, note: 83, velocity: 100 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(!e.drum_engine.sequencer.recording);
}

#[test]
fn seq_target_zero_enables_drum_note_mode_for_plain_pads() {
    let mut e = engine();
    e.seq_target_atom().store(0, Ordering::Relaxed);
    // note 40 on channel 0: with drum_mode on it hits the drum engine (no panic)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    assert!(!e.drum_engine.sequencer.playing);
    assert!(matches!(e.looper.state, LooperState::Idle));
}
