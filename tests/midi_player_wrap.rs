//! Audit rhythm #16: loop wrap must keep the current tempo — the first block
//! after wrap must not fall back to the 120 BPM default before the file's
//! TempoChange event re-applies.

use std::sync::atomic::Ordering;

use mini_midi_synth::synth::midi_player::{MidiPlayer, MidiSeqData, MidiSeqEvent, SeqEventData};

fn empty_data() -> Box<MidiSeqData> {
    Box::new(MidiSeqData {
        ticks_per_beat: 480,
        total_ticks: 480,
        events: Vec::new(),
        track_names: Vec::new(),
        track_channels: Vec::new(),
    })
}

#[test]
fn wrap_keeps_current_tempo() {
    let mut player = MidiPlayer::new();
    let data = Box::new(MidiSeqData {
        ticks_per_beat: 480,
        total_ticks: 12, // wrap shortly after the tempo event
        events: vec![MidiSeqEvent {
            tick: 8, // deliberately NOT tick 0
            channel: 0,
            data: SeqEventData::TempoChange { us_per_beat: 428_571 }, // 140 BPM
        }],
        track_names: Vec::new(),
        track_channels: Vec::new(),
    });
    let (_old_data, _old_tracks) = player.load(data);
    player.playing = true;

    let mut out = Vec::new();
    let mut prev_pos = 0;
    let mut wrapped = false;
    for _ in 0..50 {
        player.process_block(128, 48000.0, &mut out);
        let pos = player.position_atom.load(Ordering::Relaxed);
        if prev_pos > 500 && pos < prev_pos / 2 {
            wrapped = true;
            break;
        }
        prev_pos = pos;
    }
    assert!(wrapped, "player never wrapped");

    // The wrap block just happened: tempo must still be 140 BPM, not the
    // 120 BPM default for the one block until the TempoChange re-fires.
    assert_eq!(player.us_per_beat, 428_571, "wrap reset tempo to 120 BPM for one block");
}

#[test]
fn load_resets_tempo_for_new_file() {
    let mut player = MidiPlayer::new();
    player.us_per_beat = 428_571; // leftover from a previous 140 BPM file
    let _ = player.load(empty_data());
    assert_eq!(player.us_per_beat, 500_000,
        "loading a new file inherited the previous file's tempo");
}
