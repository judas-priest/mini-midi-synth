//! TECHDEBT #53 regression probes: Brass blowout, Bowed low-pitch silence +
//! NaN, BassGuitar slap crusher. Late-window peaks (blocks 100..=600)
//! distinguish sustain from onset clicks. Red before the fix, green after.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn engine_with(patch_name: &str) -> SynthEngine {
    let patches = preset::load_factory_patches(); // hermetic (preset.rs:560)
    let idx = patches.iter().position(|p| p.name == patch_name)
        .unwrap_or_else(|| panic!("patch {patch_name} must exist"));
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches.clone());
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 127 });
    let ev = ControlEvent::LoadPatch(Box::new(mini_midi_synth::synth::LoadPatchEvent {
        part: 0,
        params: mini_midi_synth::synth::PatchParams::from_map(&patches[idx].params),
        mod_matrix: Default::default(),
        mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
    }));
    let _ = e.handle_control(ev);
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    e
}

/// (early [0..30], late [100..=600]) peaks; note held the whole time.
fn render(e: &mut SynthEngine, note: u8) -> (f32, f32, bool) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let (mut early, mut late) = (0.0f32, 0.0f32);
    let mut finite = true;
    for blk in 0..600 {
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            let a = l[i].abs().max(r[i].abs());
            finite &= l[i].is_finite() && r[i].is_finite();
            if blk < 30 { early = early.max(a); }
            if blk >= 100 { late = late.max(a); }
        }
    }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note });
    let mut l2 = [0.0f32; BLOCK_SIZE]; let mut r2 = [0.0f32; BLOCK_SIZE];
    for _ in 0..200 { e.tick_block(&mut l2, &mut r2); }
    (early, late, finite)
}

#[test]
fn tc1_brass_sustains_at_c4() {
    let (early, late, _) = render(&mut engine_with("French Horn"), 60);
    assert!(late > 0.05, "brass silent at C4: early {early:.3} late {late:.3}");
}

#[test]
fn tc2_brass_sustains_at_c3() {
    let (_, late, _) = render(&mut engine_with("French Horn"), 48);
    assert!(late > 0.03, "brass silent at C3: late {late:.3}");
}

#[test]
fn tc3_brass_attack_not_a_blast() {
    let (early, late, _) = render(&mut engine_with("French Horn"), 60);
    assert!(early <= 0.9, "brass onset blast: early {early:.3}");
    assert!(early <= late * 3.0 + 0.1, "onset dominates: early {early:.3} late {late:.3}");
}

#[test]
fn tc4_bowed_sustains_at_c3() {
    let (_, late, _) = render(&mut engine_with("Violin"), 48);
    assert!(late > 0.05, "bowed silent at C3: late {late:.3}");
}

#[test]
fn tc5_bowed_stays_finite_5s() {
    let mut e = engine_with("Violin");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut finite = true;
    for _ in 0..7500 { // 5 c
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE { finite &= l[i].is_finite() && r[i].is_finite(); }
    }
    assert!(finite, "bowed output went non-finite during 5s hold");
}

#[test]
fn tc6_slap_sustains() {
    let (_, late, _) = render(&mut engine_with("Bass Guitar (Slap)"), 48);
    assert!(late >= 0.08, "slap decays instantly: late {late:.3}");
}

#[test]
fn tc7_slap_click_tamed() {
    let (early, _, _) = render(&mut engine_with("Bass Guitar (Slap)"), 48);
    assert!(early <= 0.35, "slap onset blast: early {early:.3}");
}

#[test]
fn tc8_no_regression_finger_and_high_brass() {
    let (_, late_f, _) = render(&mut engine_with("Bass Guitar (Finger)"), 48);
    assert!(late_f >= 0.1, "finger bass regressed: late {late_f:.3}");
    let (_, late_b, _) = render(&mut engine_with("Trumpet"), 72);
    assert!(late_b >= 0.2, "brass working band regressed: late {late_b:.3}");
}
