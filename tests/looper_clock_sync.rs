//! Looper/transport clock sync (REFACTOR_AUDIT S5).
//! Plan: docs/superpowers/plans/2026-10-30-refactor-s5-clock-sync.md
//!
//! Drum sequencer BPM is the canonical clock: looper bpm/beats_per_bar are
//! synced per block in tick_block; LooperSetBpm no longer exists.

use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn block(e: &mut SynthEngine) {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    e.tick_block(&mut l, &mut r);
}

#[test]
fn tc12_tick_block_syncs_looper_clock_from_sequencer() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 90.0 });
    block(&mut e);
    assert_eq!(e.looper.bpm, 90.0);
}

#[test]
fn tc13_beats_per_bar_plumbed_from_current_pattern() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqTimeSig { beats_per_bar: 3 });
    block(&mut e);
    assert_eq!(e.looper.beats_per_bar, 3);
}

#[test]
fn tc14_bpm_change_mid_loop_regrids_engine_looper() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 120.0 });
    block(&mut e); // sync: looper.bpm = 120

    let _ = e.handle_control(ControlEvent::LooperRecord); // loop established @120
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    let before = e.looper.loop_length_ticks;
    assert!(before > 0, "record must establish the loop");

    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 240.0 });
    block(&mut e); // first block after the change does the regrid
    assert_eq!(e.looper.loop_length_ticks, before / 2);
}

#[test]
fn tc15_record_at_new_bpm_quantizes_to_new_grid() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 120.0 });
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperRecord);
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    // Regrid to 240, then overdub a note and check the quantized tick.
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm: 240.0 });
    block(&mut e);
    let _ = e.handle_control(ControlEvent::LooperSetQuantize { quantize: 1 }); // Quarter
    let _ = e.handle_control(ControlEvent::LooperRecord); // overdub
    // Advance to the middle of the (now halved) loop, then record a note.
    for _ in 0..(BLOCK_SIZE * 30) { block(&mut e); }
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    // samples_per_beat @240bpm = 12_000; quantize Quarter grid == beat == 12_000.
    let beat = e.looper.bpm; // 240.0
    let spb = (60.0 / beat) * SR;
    // Read the recorded note via the display snapshot (events are private);
    // LooperDisplayEvent.position is an f32 loop fraction — the round-trip
    // through f32 carries ±1-2 samples of error, hence the tolerance.
    let display = e.looper.display();
    let d = display.try_lock().unwrap();
    let qnote = d.events.iter().find(|ev| ev.note == 64).expect("overdubbed note recorded");
    let tick = (qnote.position * e.looper.loop_length_ticks as f32).round() as u32;
    let rem = tick % spb as u32;
    assert!(rem < 2 || (spb as u32 - rem) < 2, "tick {tick} not on the 240bpm quarter grid");
}
