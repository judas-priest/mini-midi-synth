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

/// #56 roadmap 1: доминантная частота позднего окна (блоки 50..=550)
/// автокорреляцией. Lag 60..=760 → 63..800 Гц (C2: период 733 сэмпла —
/// окно 60..=500 физически не видит фундаментал C2, только гармоники).
/// Плюс честный sustain-щуп: RMS окна блоков 300..=550 (0.19–0.34 с) —
/// нота должна ещё звучать, а не лежать в шумовом полу.
/// lag-диапазон автокорреляции задаёт полосу фундаментала
/// (48Гц = lag 500 → 96 Гц; lag 120 → 400 Гц; lag 40 → 1200 Гц).
/// (60..=760) = 63..800 Гц — C2..C5; (40..=120) — для 600..1200 Гц.
fn render_freq_lag(e: &mut SynthEngine, note: u8, lag_lo: usize, lag_hi: usize) -> (f32, bool, f32) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut sig: Vec<f32> = Vec::with_capacity(4096);
    let mut finite = true;
    let mut late_energy = 0.0f64;
    let mut late_count = 0usize;
    for blk in 0..550 {
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            finite &= l[i].is_finite() && r[i].is_finite();
            if blk >= 50 { sig.push(l[i]); }
            if blk >= 300 { late_energy += (l[i] as f64) * (l[i] as f64); late_count += 1; }
        }
    }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note });
    let late_rms = (late_energy / late_count.max(1) as f64).sqrt() as f32;
    let n = sig.len();
    // Периодический сигнал: c(2P) ≈ c(P) — strict-max/descending ловит 2P
    // (субгармоника). Канон: НАИМЕНЬШИЙ ЛОКАЛЬНЫЙ максимум с c ≥ 0.9·max
    // = фундаментал.
    let mut corr: Vec<(usize, f32)> = Vec::with_capacity(lag_hi - lag_lo + 1);
    let mut global_max = 0.0f32;
    for lag in lag_lo..=lag_hi {
        let mut c = 0.0f32;
        for i in lag..n { c += sig[i] * sig[i - lag]; }
        global_max = global_max.max(c);
        corr.push((lag, c));
    }
    let mut freq = 0.0f32;
    for (idx, &(lag, c)) in corr.iter().enumerate() {
        if c < 0.9 * global_max { continue; }
        let left_ok = idx == 0 || corr[idx - 1].1 < c;
        let right_ok = idx + 1 == corr.len() || corr[idx + 1].1 < c;
        if left_ok && right_ok { freq = 48000.0 / lag as f32; break; }
    }
    (freq, finite, late_rms)
}

/// Дефолтный диапазон (60..=760) = 63..800 Гц — C2..C5.
fn render_freq(e: &mut SynthEngine, note: u8) -> (f32, bool, f32) {
    render_freq_lag(e, note, 60, 760)
}

#[test]
fn tc_b1_pitch_c4_is_nominal() {
    let (f, _, late_rms) = render_freq(&mut engine_with("Violin"), 60);
    assert!((f - 261.63).abs() <= 13.0, "C4 dominant {f:.1} Hz (expected ~261.6)");
    assert!(late_rms >= 0.03, "C4 dies to noise floor: late rms {late_rms:.5}");
}

#[test]
fn tc_b2_pitch_c3_is_nominal() {
    let (f, _, late_rms) = render_freq(&mut engine_with("Violin"), 48);
    assert!((f - 130.81).abs() <= 7.0, "C3 dominant {f:.1} Hz (expected ~130.8)");
    assert!(late_rms >= 0.03, "C3 dies to noise floor: late rms {late_rms:.5}");
}

#[test]
fn tc_b4_pitch_c2_is_nominal() {
    let (f, _, late_rms) = render_freq(&mut engine_with("Violin"), 36);
    assert!((f - 65.41).abs() <= 4.0, "C2 dominant {f:.1} Hz (expected ~65.4)");
    assert!(late_rms >= 0.03, "C2 dies to noise floor: late rms {late_rms:.5}");
}

#[test]
fn tc_b5_pitch_c5_is_nominal() {
    // C5: период ≈ 91.7 сэмпла; диапазон (40..=500) = 96..1200 Гц.
    let (f, finite, late_rms) = render_freq_lag(&mut engine_with("Violin"), 72, 40, 500);
    assert!(finite, "C5 went non-finite during probe");
    assert!((f - 523.25).abs() <= 20.0, "C5 dominant {f:.1} Hz (expected ~523.3)");
    assert!(late_rms >= 0.02, "C5 dies to noise floor: late rms {late_rms:.5}");
}

/// #56a round-2 дефектная зона: ноты 49–51 (D3±1) запирались на 4·f0.
#[test]
fn tc_b6_pitch_d3_is_nominal() {
    let (f, _, late_rms) = render_freq_lag(&mut engine_with("Violin"), 50, 40, 500);
    assert!((f - 146.83).abs() <= 8.0, "D3 dominant {f:.1} Hz (expected ~146.8)");
    assert!(late_rms >= 0.03, "D3 dies to noise floor: late rms {late_rms:.5}");
}

/// #56a round-2 дефектная зона: нота 62 (D4) запиралась на 2·f0 (~587 Гц).
#[test]
fn tc_b7_pitch_d4_is_nominal() {
    let (f, _, late_rms) = render_freq_lag(&mut engine_with("Violin"), 62, 40, 500);
    assert!((f - 293.66).abs() <= 10.0, "D4 dominant {f:.1} Hz (expected ~293.7)");
    assert!(late_rms >= 0.03, "D4 dies to noise floor: late rms {late_rms:.5}");
}

/// #56a round-2 дефектная зона: ноты 81–84 запирались на ~0.55·f0.
/// C6 ≈ 1046.5 Гц: период ≈ 45.9 сэмпла — отдельный щуп (40..=120) =
/// 400..1200 Гц; нижние ноты этим щупом не меряются.
#[test]
fn tc_b8_pitch_c6_is_nominal() {
    let (f, finite, late_rms) = render_freq_lag(&mut engine_with("Violin"), 84, 40, 120);
    assert!(finite, "C6 went non-finite during probe");
    assert!((f - 1046.5).abs() <= 30.0, "C6 dominant {f:.1} Hz (expected ~1046.5)");
    assert!(late_rms >= 0.02, "C6 dies to noise floor: late rms {late_rms:.5}");
}

#[test]
fn tc_b3_5s_hold_stays_finite() {
    let mut e = engine_with("Violin");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut finite = true;
    for _ in 0..7500 {
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE { finite &= l[i].is_finite() && r[i].is_finite(); }
    }
    assert!(finite, "5s hold went non-finite");
}
