//! Behavior contracts of set_sample_rate after the audit-S1 RT-safety refactor:
//! SR-dependent buffers resize in place, user state and atoms survive, and the
//! engine keeps rendering sane audio after a rate change.

use std::sync::Arc;

use mini_midi_synth::synth::chorus::Chorus;
use mini_midi_synth::synth::reverb::Reverb;
use mini_midi_synth::synth::spring_reverb::SpringReverb;
use mini_midi_synth::synth::{MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
}

fn finite(buf: &[f32]) -> bool {
    buf.iter().all(|s| s.is_finite())
}

/// TC-D: after an SR change (up and down), a fresh note_on must render finite,
/// non-silent audio.
#[test]
fn engine_sr_change_then_note_on_stays_sane() {
    let mut engine = SynthEngine::new(SR);
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];

    engine.set_sample_rate(96_000.0);
    engine.set_sample_rate(44_100.0); // down again — double call must be harmless
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..64 {
        engine.tick_block(&mut l, &mut r);
    }
    assert!(finite(&l) && finite(&r), "NaN/inf after SR change + note_on");
    assert!(rms(&l) > 1e-4, "silent after SR change + note_on (rms={})", rms(&l));
}

/// TC-E: Dattorro reverb resizes in place across the whole negotiated range.
#[test]
fn reverb_resizes_in_place_stays_sane() {
    let mut rev = Reverb::new(44_100.0);
    // Warm up with noise so the tank has energy, then change SR mid-stream.
    for i in 0..1000 {
        let s = ((i as f32) * 0.01).sin();
        let (l, r) = rev.tick(s, s * 0.5, 0.5, 0.3, 0.7, 0.02, 0.4);
        assert!(l.is_finite() && r.is_finite());
    }
    rev.set_sample_rate(192_000.0);
    let mut energy = 0.0;
    for i in 0..1000 {
        let s = ((i as f32) * 0.01).sin();
        let (l, r) = rev.tick(s, s * 0.5, 0.5, 0.3, 0.7, 0.02, 0.4);
        assert!(l.is_finite() && r.is_finite(), "NaN after SR change");
        energy += l.abs() + r.abs();
    }
    assert!(energy > 1e-3, "reverb dead after SR change (energy={energy})");
}

/// TC-F: spring reverb + chorus resize in place.
#[test]
fn spring_and_chorus_resize_stays_sane() {
    let mut spring = SpringReverb::new(44_100.0);
    spring.set_sample_rate(192_000.0);
    for _ in 0..500 {
        let (l, r) = spring.tick(0.5, 0.5, 0.5, 0.6, 0.4, 0.2, 0.3, 0.0, 0.5);
        assert!(l.is_finite() && r.is_finite());
    }

    let mut chorus = Chorus::new(44_100.0);
    chorus.set_sample_rate(96_000.0);
    for _ in 0..500 {
        let (l, r) = chorus.tick(0.5, 0.5);
        assert!(l.is_finite() && r.is_finite());
    }
}

/// TC-G: GUI-shared drum atoms must not be recreated by an SR change (the old
/// `*self = Self::new` dance cloned them back; a future refactor that drops
/// them would silently detach the GUI from the engine).
#[test]
fn drum_atoms_survive_sr_change() {
    let mut engine = SynthEngine::new(SR);
    let step = engine.drum_engine.step_atom();
    let play = engine.drum_engine.play_atom();
    let rec = engine.drum_engine.rec_atom();

    engine.drum_engine.set_sample_rate(44_100.0);

    assert!(Arc::ptr_eq(&step, &engine.drum_engine.step_atom()), "step_atom recreated");
    assert!(Arc::ptr_eq(&play, &engine.drum_engine.play_atom()), "play_atom recreated");
    assert!(Arc::ptr_eq(&rec, &engine.drum_engine.rec_atom()), "rec_atom recreated");
}
