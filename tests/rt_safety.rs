//! Verifies that the audio path neither allocates nor frees memory.
//!
//! Run with: cargo test --features rt-assert --test rt_safety
//!
//! The global allocator lives in the library behind the same feature, so this
//! file must not declare one of its own — a second `#[global_allocator]` in the
//! test binary would not compile.
#![cfg(feature = "rt-assert")]

use assert_no_alloc::assert_no_alloc;
use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

#[test]
fn tick_block_does_not_allocate() {
    let mut engine = SynthEngine::new(SR);
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];

    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

    assert_no_alloc(|| {
        for _ in 0..64 {
            engine.tick_block(&mut l, &mut r);
        }
    });
}

#[test]
fn loading_a_patch_does_not_free_on_the_audio_thread() {
    let patches = preset::load_all_patches();
    let patch = patches.first().expect("factory presets are compiled in").clone();
    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);

    let event = ControlEvent::load_patch_from(0, &patch);

    // Before the trash queue exists this aborts: handle_control takes the event
    // by value and drops the Box<LoadPatchEvent> right here, on what would be
    // the audio thread.
    assert_no_alloc(|| {
        engine.handle_control(event);
    });
}
