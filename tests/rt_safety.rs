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

// ── S0 coverage expansion (audit REFACTOR_AUDIT S0) ─────────────────────────

fn init_patch_with(mutations: &[(&str, f32)]) -> preset::Patch {
    let mut patch = preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists");
    for (k, v) in mutations {
        patch.params.insert(k.to_string(), *v);
    }
    patch
}

fn guarded_note_cycle(engine: &mut SynthEngine) {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        for _ in 0..4 { engine.tick_block(&mut l, &mut r); }
        engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
        for _ in 0..8 { engine.tick_block(&mut l, &mut r); }
    });
}

/// RT-guarded note-on for EVERY oscillator family. The existing suite kept
/// NoteOn OUTSIDE the guard (rt_safety.rs tick_block_does_not_allocate) — the
/// exact coverage gap that let the wavetable per-note-on alloc live.
/// OscType 27 (Wavetable) is covered by the QW-batch plan
/// (wavetable_note_on_does_not_allocate) and intentionally excluded here.
///
/// KNOWN-RED families (2026-10-30, finding of audit S0) are NOT in this table:
/// they each abort under the guard and are pinned by the ignored
/// `*_note_on_allocates` tests below — 8 of the 29 families allocate/free on
/// note-on today. TODO(S1): fix the pool-swap / voice-rebuild paths, move the
/// entries back into FAMILIES and un-ignore the pins.
#[test]
fn note_on_guarded_for_every_oscillator_family() {
    // (osc_type value, family name) — OscType::from_param, oscillator.rs:95-126
    const FAMILIES: &[(f32, &str)] = &[
        (0.0, "Sine"), (1.0, "Saw"), (2.0, "Square"), (3.0, "Triangle"),
        (4.0, "Fm"), (5.0, "Noise"),
        // 6.0 KarplusStrong  — KNOWN RED, see karplus_strong_note_on_allocates
        (7.0, "Organ"),
        (8.0, "FmPiano"),
        // 9.0 CommutedPiano — KNOWN RED, see commuted_piano_note_on_allocates
        // 10.0 BandedWG     — KNOWN RED, see banded_wg_note_on_allocates
        (11.0, "AdditivePiano"), (12.0, "DrumSynth"),
        // 13.0 BassGuitar   — KNOWN RED, see bass_guitar_note_on_allocates
        // 14.0 BowedString  — KNOWN RED, see bowed_string_note_on_allocates
        // 15.0 Brass        — KNOWN RED, see brass_note_on_allocates
        (16.0, "PhaseDistortion"), (17.0, "Wavefolder"), (18.0, "ModalResonator"),
        (19.0, "HardSync"), (20.0, "Supersaw"), (21.0, "PianoModel"),
        (22.0, "Accordion"),
        // 23.0 Saxophone    — KNOWN RED, see saxophone_note_on_allocates
        (24.0, "ElectricPiano"), (25.0, "Alias"), (26.0, "Window"),
        (28.0, "Fm3"),
        // 29.0 Twist        — KNOWN RED, see twist_note_on_allocates
    ];
    for (osc, _name) in FAMILIES {
        let patch = init_patch_with(&[("osc_type", *osc)]);
        let mut engine = SynthEngine::new(SR);
        let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
        drop(spent);
        let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
        guarded_note_cycle(&mut engine); // aborts (process dies) on any alloc
    }
}

// ── KNOWN-RED family pins (found 2026-10-30, audit S0 — NOT fixed here) ─────
//
// Every test below is a RED finding of this suite: the family's note-on
// aborts under assert_no_alloc (the abort message reports the layout of the
// denied op — note that a forbidden DEALLOC of N bytes is also reported as
// "memory allocation of N bytes failed"). Each pin documents the alloc site.
// Do NOT fix prod code in this plan; TODO(S1) un-ignores them.
//
// Shared mechanism for the pool families (6/9/10/13/14/15/23): every note-on
// runs Voice::init (voice.rs:249+) → Oscillator::reset/init_* →
// reclaim_buffers (oscillator.rs:481) → return_buf (oscillator.rs:468),
// which OVERWRITES the pool slot — dropping the previous pooled
// Vec::with_capacity(MAX_DELAY = 7000 f32 = 28000 bytes) while the audio
// guard is held. The pools need swap-not-clobber semantics (S1).

fn red_family_cycle(osc_type: f32) {
    let patch = init_patch_with(&[("osc_type", osc_type)]);
    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    guarded_note_cycle(&mut engine);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): KS note-on clobbers pool slot — return_buf drops the previous 28000 B pooled Vec inside the guard (oscillator.rs:468 via reclaim_buffers:488, init_ks:702). TODO(S1): swap pools instead of clobbering, then un-ignore and re-add (6.0, KarplusStrong) to FAMILIES."]
fn karplus_strong_note_on_allocates() {
    red_family_cycle(6.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): CommutedPiano note-on clobbers pool slots a+b (oscillator.rs:468/490-492, init_commuted_piano:738) — 2x 28000 B freed inside the guard. TODO(S1): fix pool semantics, then un-ignore and re-add (9.0) to FAMILIES."]
fn commuted_piano_note_on_allocates() {
    red_family_cycle(9.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): BandedWG note-on clobbers pool slots a..d (oscillator.rs:468/494-499, init_banded_wg:905) — up to 4x 28000 B freed inside the guard. TODO(S1): fix pool semantics, then un-ignore and re-add (10.0) to FAMILIES."]
fn banded_wg_note_on_allocates() {
    red_family_cycle(10.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): BassGuitar note-on clobbers pool slots a+b (oscillator.rs:468/501-503, init_bass_guitar:1439). TODO(S1): fix pool semantics, then un-ignore and re-add (13.0) to FAMILIES."]
fn bass_guitar_note_on_allocates() {
    red_family_cycle(13.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): BowedString note-on clobbers pool slots a+b (oscillator.rs:468/504-507, init_bowed_string:1521). TODO(S1): fix pool semantics, then un-ignore and re-add (14.0) to FAMILIES."]
fn bowed_string_note_on_allocates() {
    red_family_cycle(14.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): Brass note-on clobbers pool slot a (oscillator.rs:468/508-510, init_brass:1662). TODO(S1): fix pool semantics, then un-ignore and re-add (15.0) to FAMILIES."]
fn brass_note_on_allocates() {
    red_family_cycle(15.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): Saxophone note-on clobbers pool slot a (oscillator.rs:468/511-513, init_saxophone:2412). TODO(S1): fix pool semantics, then un-ignore and re-add (23.0) to FAMILIES."]
fn saxophone_note_on_allocates() {
    red_family_cycle(23.0);
}

#[test]
#[ignore = "KNOWN RED (2026-10-30): Twist (Plaits) note-on rebuilds the voice — TwistVoice::new Box-allocates a fresh mi_plaits_dsp Voice per note (oscillator.rs:2752, struct at :64). TODO(S1): keep the Voice allocated and only re-init, then un-ignore and re-add (29.0) to FAMILIES."]
fn twist_note_on_allocates() {
    red_family_cycle(29.0);
}

/// 16-slot fx_chain: slot params are read per block, slot DSP instances are
/// preallocated — tick_block with an active chain must stay allocation-free.
#[test]
fn tick_block_with_active_fx_chain_does_not_allocate() {
    let patch = init_patch_with(&[("fx0_type", 13.0), ("fx0_en", 1.0), ("fx0_mix", 0.5)]);
    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    assert_no_alloc(|| {
        for _ in 0..128 { engine.tick_block(&mut l, &mut r); }
    });
}

/// Engine-level LFO site: part LFOs tick in tick_block, and apply_deform
/// routes ANY waveform through the sine LUT at deform >= 0.5 (lfo.rs:286).
/// lfo1_active requires a routing depth key (mod.rs:2758-2763): deform alone
/// never activates the LFO — lfo_amp_depth switches the site on.
/// Distinct from the QW-batch unit test (Lfo::tick_with_deform + warmup).
#[test]
fn tick_block_with_lfo_deform_does_not_allocate() {
    let patch = init_patch_with(&[("lfo_deform", 0.8), ("lfo_waveform", 0.0), ("lfo_amp_depth", 0.5)]);
    let mut engine = SynthEngine::new(SR);
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    // Warmup OUTSIDE the guard: the global sine LUT is lazily allocated via
    // OnceLock (lfo.rs:6-18, 8 KiB) on the first tick_with_deform — a
    // one-time process-wide cost (the QW2 unit test warms it the same way).
    // It is never reallocated afterwards; the guard pins steady-state ticks.
    for _ in 0..4 { engine.tick_block(&mut l, &mut r); }
    assert_no_alloc(|| {
        for _ in 0..64 { engine.tick_block(&mut l, &mut r); }
    });
}

/// set_sample_rate runs in the audio callback (audio.rs:199-205 reacts to the
/// jack_sr atom) — after the S1 fix the whole chain resizes preallocated
/// buffers in place: zero alloc/free. Duplicate call is covered too
/// (audio.rs gates on `new_sr != synth_sr`, but the method itself must stay
/// harmless when invoked twice at the same rate).
#[test]
fn set_sample_rate_is_allocation_free() {
    let mut engine = SynthEngine::new(SR);
    assert_no_alloc(|| {
        engine.set_sample_rate(96_000.0);
        engine.set_sample_rate(96_000.0); // duplicate call must also be free
    });
}

/// JACK mid-callback scenario (audit S1 TC-C): a note is held, one block was
/// rendered, then JACK reports a new rate and the callback applies it before
/// the next block. Green since the S1 fix (in-place SR updates everywhere).
#[test]
fn sr_change_mid_render_does_not_allocate() {
    let mut engine = SynthEngine::new(SR);
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    engine.tick_block(&mut l, &mut r);
    assert_no_alloc(|| {
        engine.set_sample_rate(96_000.0);
        engine.tick_block(&mut l, &mut r);
    });
}

/// DrumEngine::set_sample_rate updates SR-dependent fields explicitly
/// (audit S1): plain-Copy voice rebuilds, click-state reset, choke-holder
/// reset — no Arc allocations, no `*self = Self::new` dance.
#[test]
fn drum_set_sample_rate_is_allocation_free() {
    let mut engine = SynthEngine::new(SR);
    assert_no_alloc(|| engine.drum_engine.set_sample_rate(96_000.0));
}

/// Regrid (REFACTOR_AUDIT S5) rescales up to MAX_LOOP_EVENTS event ticks and
/// calls update_display inside tick_block — must stay allocation-free.
#[test]
fn looper_bpm_regrid_does_not_allocate() {
    let mut engine = SynthEngine::new(SR);
    let _ = engine.handle_control(ControlEvent::DrumSeqBpm { bpm: 120.0 });
    let _ = engine.handle_control(ControlEvent::LooperRecord);
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    engine.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    engine.tick_block(&mut l, &mut r);
    let _ = engine.handle_control(ControlEvent::LooperStopRecord);

    assert_no_alloc(|| {
        let _ = engine.handle_control(ControlEvent::DrumSeqBpm { bpm: 90.0 });
        for _ in 0..4 { engine.tick_block(&mut l, &mut r); } // first block regrids
    });
}
