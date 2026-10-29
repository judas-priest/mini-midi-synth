//! Root-only mode audio guard: suppressed notes must leave no stuck voices.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;

fn render_peak(engine: &mut SynthEngine, blocks: usize) -> f32 {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
    }
    peak
}

fn settle(engine: &mut SynthEngine) {
    let _ = render_peak(engine, 6000);
}

#[test]
fn root_only_suppressed_note_leaves_no_stuck_voice() {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    let _ = e.handle_control(ControlEvent::SetPartRootOnly { part: 0, on: true });

    // Chord: only the lowest key must sound on this part.
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 67, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE, "lowest note did not sound");

    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 67 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    settle(&mut e);
    assert!(render_peak(&mut e, 20) < SILENCE, "stuck voice after all keys released");
}
