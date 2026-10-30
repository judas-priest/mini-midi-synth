//! Drum grid recording (#22): pad hits land in the sequencer grid under ANY
//! kit remap. The grid slot comes from the PAD note (36-51); the remap table
//! only changes the SOUND. Regression: with Metal_Drums_v2-style remaps
//! (targets outside 36..=51) recording was silently skipped.

use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};

const SR: f32 = 48_000.0;
const METAL_REMAP: [u8; 16] = [30, 38, 51, 48, 49, 52, 51, 50, 48, 44, 43, 45, 52, 38, 38, 38];

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

fn grid_has_hits(e: &SynthEngine, slot: usize) -> bool {
    e.drum_engine.sequencer.patterns[e.drum_engine.sequencer.current_pattern as usize]
        .steps[slot]
        .iter()
        .any(|s| s.velocity > 0)
}

#[test]
fn tc1_remapped_kit_records_into_grid() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::DrumSetRemap { table: Box::new(METAL_REMAP) });
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm rec + auto-play
    assert!(e.drum_engine.sequencer.recording);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 36, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 36 });
    assert!(grid_has_hits(&e, 0), "pad 36 must write grid slot 0 even though remap target is 30");
}

#[test]
fn tc2_slot_follows_pad_not_remap_target() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::DrumSetRemap { table: Box::new(METAL_REMAP) });
    let _ = e.handle_control(ControlEvent::MasterRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 40 });
    assert!(grid_has_hits(&e, 4), "pad 40 -> grid slot 4 (remap target 48 must not redirect)");
    assert!(!grid_has_hits(&e, 12), "no spill into other slots");
}

#[test]
fn tc3_identity_remap_still_records() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 38, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 38 });
    assert!(grid_has_hits(&e, 2), "no-remap kit records as before");
}

#[test]
fn tc4_non_pad_notes_do_not_record() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 30, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 30 });
    let any = (0..16usize).any(|s| grid_has_hits(&e, s));
    assert!(!any, "ch9 note 30 is not a pad (36-51) — grid untouched");
}
