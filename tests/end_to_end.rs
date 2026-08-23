//! Whole-engine behaviour: control events in, audio out.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
/// Below this a block counts as silence. The engine's own tests use the same
/// order of magnitude for "no signal".
const SILENCE: f32 = 1e-3;

/// Renders `blocks` blocks and returns the peak absolute sample.
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

/// An engine with a plain subtractive patch on part 0 — no reverb tail to wait
/// out, so "did it stop" assertions stay fast and unambiguous.
fn engine_with_init_patch() -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();

    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);
    let _ = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = engine.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    engine
}

/// Renders long enough for release stages and effect tails to die out.
fn settle(engine: &mut SynthEngine) {
    let _ = render_peak(engine, 6000);
}

#[test]
fn note_on_produces_sound() {
    let mut engine = engine_with_init_patch();
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let peak = render_peak(&mut engine, 200);
    assert!(peak > SILENCE, "NoteOn produced silence (peak {peak})");
}

#[test]
fn note_off_stops_the_sound() {
    let mut engine = engine_with_init_patch();
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut engine, 200) > SILENCE, "no sound to stop");

    engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut engine);

    let after = render_peak(&mut engine, 200);
    assert!(after < SILENCE, "sound continued after NoteOff (peak {after})");
}

#[test]
fn all_notes_off_silences_a_held_chord() {
    let mut engine = engine_with_init_patch();
    for note in [60u8, 64, 67, 72] {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    }
    assert!(render_peak(&mut engine, 200) > SILENCE, "chord produced silence");

    let _ = engine.handle_control(ControlEvent::AllNotesOff);
    settle(&mut engine);

    let after = render_peak(&mut engine, 200);
    assert!(after < SILENCE, "AllNotesOff left sound behind (peak {after})");
}

#[test]
fn cc_123_from_the_wire_silences_a_held_chord() {
    let mut engine = engine_with_init_patch();
    for note in [60u8, 64, 67] {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    }
    assert!(render_peak(&mut engine, 200) > SILENCE, "chord produced silence");

    // Exactly the bytes a controller's panic button sends.
    let (event, _) = mini_midi_synth::midi::parse_midi_message(&[0xB0, 123, 0])
        .expect("CC 123 must parse");
    engine.handle_event(event);
    settle(&mut engine);

    let after = render_peak(&mut engine, 200);
    assert!(after < SILENCE, "CC 123 did not stop the notes (peak {after})");
}

#[test]
fn disabled_part_is_silent() {
    let mut engine = engine_with_init_patch();
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: false });

    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let peak = render_peak(&mut engine, 200);
    assert!(peak < SILENCE, "disabled part produced sound (peak {peak})");
}

#[test]
fn notes_outside_the_part_range_do_not_sound() {
    let mut engine = engine_with_init_patch();
    let _ = engine.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 60, max_note: 72 });

    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 127 });
    let peak = render_peak(&mut engine, 200);
    assert!(peak < SILENCE, "note below the range sounded (peak {peak})");

    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 127 });
    let peak = render_peak(&mut engine, 200);
    assert!(peak > SILENCE, "note inside the range stayed silent (peak {peak})");
}

#[test]
fn engine_output_stays_finite_under_full_polyphony() {
    let mut engine = engine_with_init_patch();
    for note in 24..96u8 {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    }

    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..500 {
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            assert!(l[i].is_finite() && r[i].is_finite(), "engine emitted NaN or inf");
        }
    }
}

#[test]
fn every_factory_patch_loads_and_can_be_played() {
    // Catches a preset whose params map is missing something the engine needs,
    // which would otherwise only surface when a user picks that preset.
    let patches = preset::load_all_patches();
    assert!(!patches.is_empty(), "no factory presets compiled in");

    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches.clone());
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });

    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for patch in &patches {
        let _ = engine.handle_control(ControlEvent::load_patch_from(0, patch));
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
        for _ in 0..40 {
            engine.tick_block(&mut l, &mut r);
            for i in 0..BLOCK_SIZE {
                assert!(
                    l[i].is_finite() && r[i].is_finite(),
                    "patch '{}' emitted NaN or inf",
                    patch.name
                );
            }
        }
        engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
        let _ = engine.handle_control(ControlEvent::AllNotesOff);
    }
}
