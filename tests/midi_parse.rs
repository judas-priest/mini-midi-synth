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
fn smf_header_check_rejects_what_makes_midly_panic() {
    use mini_midi_synth::midi::check_smf_header;

    // Found by fuzzing import_midi_drums: an SMPTE timing field with an
    // illegal frame rate makes midly 0.5.3 panic with "attempt to negate with
    // overflow". The release profile aborts on panic, so this must never
    // reach the parser.
    let crasher = [
        0x4d, 0x54, 0x68, 0x64, // "MThd"
        0x00, 0x00, 0x00, 0x06, // header length
        0x00, 0x00, 0x00, 0x2d, // format, ntrks
        0x80, 0xff, // division: SMPTE with fps = -128
    ];
    assert!(check_smf_header(&crasher).is_err(), "the known crasher must be rejected");

    assert!(check_smf_header(&[]).is_err());
    assert!(check_smf_header(b"not a midi file at all").is_err());

    // Zero ticks per beat would divide by zero downstream.
    let zero_ppq = [
        0x4d, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06,
        0x00, 0x01, 0x00, 0x02, 0x00, 0x00,
    ];
    assert!(check_smf_header(&zero_ppq).is_err());
}

#[test]
fn smf_header_check_accepts_valid_headers() {
    use mini_midi_synth::midi::check_smf_header;

    // Ordinary metrical timing: 480 ticks per quarter note.
    let metrical = [
        0x4d, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06,
        0x00, 0x01, 0x00, 0x02, 0x01, 0xe0,
    ];
    assert!(check_smf_header(&metrical).is_ok());

    // Legal SMPTE: 25 fps, 40 ticks per frame.
    let smpte = [
        0x4d, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06,
        0x00, 0x01, 0x00, 0x02, 0xe7, 0x28,
    ];
    assert!(check_smf_header(&smpte).is_ok(), "25 fps SMPTE is valid");
}

#[test]
fn garbage_does_not_panic() {
    assert!(parse_midi_message(&[]).is_none());
    assert!(parse_midi_message(&[0xF7]).is_none());
    assert!(parse_midi_message(&[0x90]).is_none());
}

#[test]
fn console_notes_swallowed_and_mirrored() {
    let (prod, mut cons) = rtrb::RingBuffer::<mini_midi_synth::synth::MidiEvent>::new(64);
    let tx: mini_midi_synth::midi::SharedMidiTx =
        std::sync::Arc::new(std::sync::Mutex::new(prod));
    let note_state: mini_midi_synth::midi::NoteState =
        std::sync::Arc::new(std::array::from_fn(|_| Default::default()));
    let pad_state: mini_midi_synth::midi::PadState =
        std::sync::Arc::new(std::array::from_fn(|_| Default::default()));
    let (snd, rx) = std::sync::mpsc::channel();
    *mini_midi_synth::midi::CONSOLE_TX.get_or_init(Default::default)
        .lock().unwrap() = Some(snd);
    // NoteOn ch9 note 60 vel 127: 99 3C 7F
    let used = mini_midi_synth::midi::parse_and_push(&[0x99, 0x3C, 0x7F], &tx, &note_state, &pad_state);
    assert_eq!(used, 3);
    assert_eq!(rx.recv().unwrap(), (60, true));      // mirrored to GUI
    assert!(cons.pop().is_err());                    // never reached the engine
    // NoteOff ch9 note 60: 89 3C 7F
    let _ = mini_midi_synth::midi::parse_and_push(&[0x89, 0x3C, 0x7F], &tx, &note_state, &pad_state);
    assert_eq!(rx.recv().unwrap(), (60, false));
}

// ── SMPTE true-time rate (audit rhythm #15) ────────────────────

/// Minimal SMF, format 0, SMPTE division 30 fps / 4 sub-frames
/// (division bytes 0xE2 0x04: high byte is the negated fps).
/// Track: tempo 500_000 (must be IGNORED in SMPTE), note-on at tick 240, EOT.
fn smpte_smf() -> Vec<u8> {
    let track: Vec<u8> = [
        &[0x00u8][..], // delta 0
        &[0xFF, 0x51, 0x03, 0x07, 0xA1, 0x20][..], // tempo 500_000
        &[0x81, 0x70][..], // delta 240 (varlen)
        &[0x90, 0x3C, 0x40][..], // note-on ch0 note60 vel64
        &[0x00, 0xFF, 0x2F, 0x00][..], // end of track
    ].concat();
    let header: Vec<u8> = [
        b"MThd".as_slice(),
        &[0x00, 0x00, 0x00, 0x06][..],
        &[0x00, 0x01][..], // format 1
        &[0x00, 0x01][..], // one track
        &[0xE2, 0x04][..], // SMPTE 30fps x 4 sub
    ].concat();
    let mut raw = header;
    raw.extend(b"MTrk");
    raw.extend((track.len() as u32).to_be_bytes());
    raw.extend(track);
    raw
}

#[test]
fn smpte_file_reports_ticks_per_second() {
    let data = mini_midi_synth::synth::midi_player::parse_midi(&smpte_smf())
        .expect("valid SMPTE file");
    assert_eq!(data.smpte_ticks_per_sec, Some(120.0), "30fps x 4 sub = 120 ticks/sec");
    assert_eq!(data.ticks_per_beat, 120);
}

#[test]
fn smpte_event_sounds_at_true_time() {
    use mini_midi_synth::synth::midi_player::{MidiPlayer, SeqEventData};

    let data = Box::new(
        mini_midi_synth::synth::midi_player::parse_midi(&smpte_smf())
            .expect("valid SMPTE file"),
    );
    let mut player = MidiPlayer::new();
    let _ = player.load(data);
    player.playing = true;

    // Note at tick 240 with 120 ticks/sec must sound at 2.0 s = 96000 samples,
    // i.e. block 750 of 128 samples (±1 block). The embedded tempo event
    // (500_000) must NOT retime this to 1.0 s.
    let mut out = Vec::new();
    let mut blocks = 0usize;
    let mut fired_at = None;
    for _ in 0..1500 {
        player.process_block(128, 48000.0, &mut out);
        blocks += 1;
        if out.iter().any(|(_, d)| matches!(d, SeqEventData::NoteOn { .. })) {
            fired_at = Some(blocks);
            break;
        }
    }
    let fired_at = fired_at.expect("note never fired");
    let expected = 2.0 * 48000.0 / 128.0; // 750 blocks
    assert!((fired_at as f64 - expected).abs() <= 1.0,
        "note fired at block {fired_at}, expected ~{expected} (2.0 s at 120 ticks/sec)");
}
