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

    // The event must come back out of the guarded region rather than being
    // dropped inside it — that is exactly what the trash queue does in the
    // real callback.
    let mut spent = None;
    assert_no_alloc(|| {
        spent = engine.handle_control(event);
    });

    assert!(spent.is_some(), "LoadPatch owns a Box and must be handed back for deferred freeing");
    drop(spent);
}

#[test]
fn looper_control_events_do_not_allocate() {
    // start_record / undo / clear all call update_display, which used to push
    // up to MAX_LOOP_EVENTS entries into an empty Vec on the audio thread.
    let mut engine = SynthEngine::new(SR);

    let _ = engine.handle_control(ControlEvent::LooperRecord);
    for note in 60..72u8 {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 100 });
    }

    assert_no_alloc(|| {
        let _ = engine.handle_control(ControlEvent::LooperTogglePlay);
        let _ = engine.handle_control(ControlEvent::LooperUndo);
        let _ = engine.handle_control(ControlEvent::LooperClear);
    });
}

#[test]
fn switching_between_wavetable_patches_does_not_free_in_rt() {
    // Loading a patch overwrites part.wavetable_data, dropping the previous
    // Arc<Vec<f32>>. With a wavetable patch on both sides that is a real free
    // on the audio thread.
    let patches = preset::load_all_patches();
    let with_wt: Vec<_> = patches.iter().filter(|p| p.wavetable_data.is_some()).cloned().collect();
    if with_wt.len() < 2 {
        // No wavetable presets installed on this machine — nothing to prove.
        return;
    }

    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);

    let first = ControlEvent::load_patch_from(0, &with_wt[0]);
    let spent = engine.handle_control(first);
    drop(spent);

    let second = ControlEvent::load_patch_from(0, &with_wt[1]);
    let mut spent = None;
    assert_no_alloc(|| {
        spent = engine.handle_control(second);
    });
    drop(spent);
}
