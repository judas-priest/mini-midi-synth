//! Cost of one audio block under realistic load.
//!
//! The number that matters is time per block against the callback budget: at
//! 48 kHz with a 256-frame buffer the callback has 5333 µs and calls
//! tick_block 256/BLOCK_SIZE times, so a block must stay well under
//! BLOCK_SIZE / 48000 seconds — 667 µs at BLOCK_SIZE 32 — with room to spare
//! for the rest of the callback and for slower target hardware
//! (Orange Pi 5, phones).

use criterion::{criterion_group, criterion_main, Criterion};
use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn engine_with(patch_name: &str, voices: usize) -> SynthEngine {
    let patches = preset::load_factory_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(patch_name))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();

    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);
    let _ = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });

    for i in 0..voices {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 48 + i as u8, velocity: 100 });
    }
    engine
}

fn bench_block(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_block");

    for &voices in &[1usize, 8, 16] {
        let mut engine = engine_with("Init", voices);
        let mut l = [0.0f32; BLOCK_SIZE];
        let mut r = [0.0f32; BLOCK_SIZE];

        group.bench_function(format!("init_{voices}_voices"), |b| {
            b.iter(|| engine.tick_block(&mut l, &mut r));
        });
    }

    // A patch that exercises the effect chain, not just the oscillator.
    for name in ["Ambient Pad", "Grand Piano"] {
        let mut engine = engine_with(name, 8);
        let mut l = [0.0f32; BLOCK_SIZE];
        let mut r = [0.0f32; BLOCK_SIZE];
        let label = name.to_lowercase().replace(' ', "_");
        group.bench_function(format!("{label}_8_voices"), |b| {
            b.iter(|| engine.tick_block(&mut l, &mut r));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_block);
criterion_main!(benches);
