//! Parsing of raw MIDI bytes into engine events.

use mini_midi_synth::midi::parse_midi_message;
use mini_midi_synth::synth::MidiEvent;

#[test]
fn note_on_parses() {
    let (event, consumed) = parse_midi_message(&[0x90, 60, 100]).expect("valid note on");
    assert_eq!(consumed, 3);
    match event {
        MidiEvent::NoteOn { channel, note, velocity } => {
            assert_eq!((channel, note, velocity), (0, 60, 100));
        }
        _ => panic!("expected NoteOn"),
    }
}

#[test]
fn note_off_parses() {
    let (event, _) = parse_midi_message(&[0x80, 60, 0]).expect("valid note off");
    match event {
        MidiEvent::NoteOff { channel, note } => assert_eq!((channel, note), (0, 60)),
        _ => panic!("expected NoteOff"),
    }
}

#[test]
fn mod_wheel_is_normalised() {
    let (event, _) = parse_midi_message(&[0xB0, 1, 127]).expect("CC 1 is the mod wheel");
    match event {
        MidiEvent::ModWheel { value, .. } => assert!((value - 1.0).abs() < 1e-6),
        _ => panic!("expected ModWheel"),
    }
}

#[test]
fn all_notes_off_reaches_the_engine() {
    // CC 123 — the panic button on most controllers.
    let (event, _) = parse_midi_message(&[0xB0, 123, 0]).expect("CC 123 must not be filtered out");
    match event {
        MidiEvent::ControlChange { cc, .. } => assert_eq!(cc, 123),
        _ => panic!("expected ControlChange"),
    }
}

#[test]
fn all_sound_off_reaches_the_engine() {
    let (event, _) = parse_midi_message(&[0xB0, 120, 0]).expect("CC 120 must not be filtered out");
    match event {
        MidiEvent::ControlChange { cc, .. } => assert_eq!(cc, 120),
        _ => panic!("expected ControlChange"),
    }
}

#[test]
fn bank_select_is_still_filtered() {
    // CC 0 and CC 32 are bank select. Nothing downstream uses them and letting
    // them through would collide with mapped parameters.
    assert!(parse_midi_message(&[0xB0, 0, 0]).is_none());
    assert!(parse_midi_message(&[0xB0, 32, 0]).is_none());
}

#[test]
fn pitch_bend_centre_is_zero() {
    // 0x2000 is centre.
    let (event, _) = parse_midi_message(&[0xE0, 0x00, 0x40]).expect("valid pitch bend");
    match event {
        MidiEvent::PitchBend { value, .. } => assert!(value.abs() < 1e-6, "centre bend was {value}"),
        _ => panic!("expected PitchBend"),
    }
}

#[test]
fn garbage_does_not_panic() {
    assert!(parse_midi_message(&[]).is_none());
    assert!(parse_midi_message(&[0xF7]).is_none());
    assert!(parse_midi_message(&[0x90]).is_none());
}
