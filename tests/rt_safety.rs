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
fn metronome_click_does_not_allocate() {
    // 400 BPM → beat every 7200 samples; 800 blocks = 25600 samples
    // crosses 3 beat boundaries with click triggers inside the guarded region.
    let mut engine = SynthEngine::new(SR);
    let _ = engine.handle_control(ControlEvent::DrumSetMetronome { on: true });
    let _ = engine.handle_control(ControlEvent::DrumSeqBpm { bpm: 400.0 });
    let _ = engine.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        for _ in 0..800 {
            engine.tick_block(&mut l, &mut r);
        }
    });
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

#[test]
fn lfo_sine_lut_does_not_allocate() {
    use mini_midi_synth::synth::lfo::{Lfo, LfoWaveform};
    // The 8 KB sine LUT is hit by every Sine tick AND by apply_deform at
    // deform >= 0.5 for ANY waveform (lfo.rs apply_deform). Without warmup it
    // is built lazily on the audio thread's first tick.
    mini_midi_synth::synth::lfo::warmup_sine_lut();
    let mut lfo = Lfo::new(SR);
    assert_no_alloc(|| {
        for _ in 0..64 {
            // deform 0.8 exercises both LUT consumers
            let _ = lfo.tick_with_deform(5.0, LfoWaveform::Sine, 0.8);
            let _ = lfo.tick_with_deform(5.0, LfoWaveform::Triangle, 0.8);
        }
    });
}

#[test]
fn wavetable_note_on_does_not_allocate() {
    // Built-in wavetable bank: every note-on used to rebuild the 8x256 table
    // (take_buf + Arc::new + 13312 sin()) and drop an empty placeholder Arc
    // from reset(). The OnceLock cache plus warmup must make the whole
    // note-on path allocation-free. Plan: docs/superpowers/plans/2026-10-30-refactor-qw-batch.md
    let mut patch = preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists");
    patch.params.insert("osc_type".into(), 27.0); // OscType::Wavetable, built-in bank
    patch.wavetable_data = None;                  // custom .wt path is already Arc-shared

    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });

    // init_common does this before the audio stream exists; the test mirrors it
    // so the guard below measures the note-on path, not the lazy build.
    mini_midi_synth::synth::oscillator::warmup_builtin_wavetable();

    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        for _ in 0..8 { engine.tick_block(&mut l, &mut r); }
        engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
        for _ in 0..8 { engine.tick_block(&mut l, &mut r); }
    });
}
