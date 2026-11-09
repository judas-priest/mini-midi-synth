//! Audit rhythm #14: a seq drum note-off must follow the note-on's route.
//! Mechanism: channels with active seq drum notes are tracked in a u16
//! bitmask, so an off still reaches the drum engine after an intermediate
//! non-drum note-on flipped seq_route away from drums.

use mini_midi_synth::synth::midi_player::{MidiSeqData, MidiSeqEvent, SeqEventData, TrackInstrument};
use mini_midi_synth::synth::{ControlEvent, SynthEngine};

fn overlapping_drums_data() -> Box<MidiSeqData> {
    // Channel 9 (GM drums): drum note-on, then (after the track flips to a
    // synth) a synth note-on, then the drum note-off (must still reach
    // drums), then the synth note-off.
    Box::new(MidiSeqData {
        ticks_per_beat: 480,
        total_ticks: 1200,
        events: vec![
            MidiSeqEvent { tick: 0, channel: 9, data: SeqEventData::NoteOn { note: 60, velocity: 100 } },
            MidiSeqEvent { tick: 240, channel: 9, data: SeqEventData::NoteOn { note: 62, velocity: 100 } },
            MidiSeqEvent { tick: 480, channel: 9, data: SeqEventData::NoteOff { note: 60 } },
            MidiSeqEvent { tick: 720, channel: 9, data: SeqEventData::NoteOff { note: 62 } },
        ],
        track_names: vec!["drums".into()],
        track_channels: vec![9],
        smpte_ticks_per_sec: None,
    })
}

fn run_blocks(e: &mut SynthEngine, n: usize) {
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    for _ in 0..n { e.tick_block(&mut l, &mut r); }
}

fn run_until_pos(e: &mut SynthEngine, pos_per_mille: u32) {
    let (mut l, mut r) = ([0.0f32; 128], [0.0f32; 128]);
    for _ in 0..10_000 {
        e.tick_block(&mut l, &mut r);
        let p = e.midi_player.position_atom.load(std::sync::atomic::Ordering::Relaxed);
        if p >= pos_per_mille { return; }
    }
    panic!("position never reached {pos_per_mille}");
}

#[test]
fn seq_drum_note_off_reaches_drums_after_route_flip() {
    let mut e = SynthEngine::new(48_000.0);
    e.drum_engine.sf2_mode = true;
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: overlapping_drums_data() });
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: true });

    // tick 0: drum note-on routed to drums — channel 9 (GM drums — defaults to Drums on load) is marked active.
    run_until_pos(&mut e, 10);
    assert_eq!(e.seq_route_for_test()[9], 2, "note-on did not route to drums");
    assert!(e.seq_drum_active() & (1 << 9) != 0, "drum note-on did not set the channel bit");

    // Flip the track to a synth; the next note-on re-routes the channel.
    let _ = e.handle_control(ControlEvent::MidiSeqSetTrackInstrument {
        track_idx: 0,
        instrument: TrackInstrument::DspLayer0,
    });
    run_until_pos(&mut e, 430); // past tick 240 (synth note-on) and 480 (drum off)
    assert_eq!(e.seq_route_for_test()[9], 0, "synth note-on did not re-route the channel");
    assert_eq!(e.seq_drum_active() & (1 << 9), 0,
        "drum note-off was not delivered to the drum engine: bit still set after route flip");

    // The synth note-off (route 0, bit cleared) must NOT take the drum path:
    // the bit must stay cleared past tick 720.
    // The file wraps as soon as its last event (tick 720) has fired — the
    // position atom never shows the tail. Run through the wrap (50 blocks is
    // several percent of a cycle) and confirm the bit stays clear.
    run_until_pos(&mut e, 590);
    run_blocks(&mut e, 50);
    assert_eq!(e.seq_drum_active() & (1 << 9), 0, "synth note-off spuriously set the drum bit");
}

#[test]
fn seq_drum_bitmask_resets() {
    let mut e = SynthEngine::new(48_000.0);
    e.drum_engine.sf2_mode = true;

    // (a) MidiSeqPlay { playing: false } clears the mask.
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: overlapping_drums_data() });
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: true });
    run_until_pos(&mut e, 10);
    assert!(e.seq_drum_active() != 0, "precondition: bit set after drum note-on");
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    assert_eq!(e.seq_drum_active(), 0, "seq stop did not clear the drum bitmask");

    // (b) panic (CC123) clears the mask.
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: overlapping_drums_data() });
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: true });
    run_until_pos(&mut e, 10);
    e.handle_event(mini_midi_synth::synth::MidiEvent::ControlChange {
        channel: 0, cc: 123, value: 0,
    });
    assert_eq!(e.seq_drum_active(), 0, "panic CC123 did not clear the drum bitmask");

    // (c) MidiSeqLoad clears the mask.
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: overlapping_drums_data() });
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: true });
    run_until_pos(&mut e, 10);
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: overlapping_drums_data() });
    assert_eq!(e.seq_drum_active(), 0, "seq load did not clear the drum bitmask");
}
