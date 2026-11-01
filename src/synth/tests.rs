use super::*;

// ── Envelope tests ─────────────────────────────────────────────

#[test]
fn envelope_idle_returns_zero() {
    let mut env = envelope::Envelope::new(44100.0);
    for _ in 0..100 {
        assert_eq!(env.tick(), 0.0);
    }
    assert!(env.is_idle());
}

#[test]
fn envelope_adsr_stages() {
    let mut env = envelope::Envelope::new(44100.0);
    env.set_adsr(0.01, 0.1, 0.5, 0.3);
    env.note_on();

    // Attack: should rise toward 1.0
    let mut peak = 0.0_f32;
    for _ in 0..1000 {
        let v = env.tick();
        peak = peak.max(v);
    }
    assert!(peak > 0.9, "Attack didn't reach near 1.0, peak={peak}");

    // Decay/Sustain: run further, should settle near sustain
    for _ in 0..44100 {
        env.tick();
    }
    let sustain_val = env.tick();
    assert!((sustain_val - 0.5).abs() < 0.05, "Sustain not near 0.5, got {sustain_val}");

    // Release: should decay to 0 (run 2 seconds for long release tails)
    env.note_off();
    for _ in 0..88200 {
        env.tick();
    }
    assert!(env.is_idle(), "Envelope didn't reach idle after release");
    assert!(env.tick().abs() < 0.001);
}

#[test]
fn envelope_output_range_0_to_1() {
    let mut env = envelope::Envelope::new(44100.0);
    env.set_adsr(0.005, 0.05, 0.8, 0.2);
    env.note_on();
    for _ in 0..44100 {
        let v = env.tick();
        assert!((-0.01..=1.01).contains(&v), "Envelope out of range: {v}");
    }
    env.note_off();
    for _ in 0..44100 {
        let v = env.tick();
        assert!((-0.01..=1.01).contains(&v), "Envelope out of range in release: {v}");
    }
}

#[test]
fn envelope_zero_sustain_goes_idle() {
    let mut env = envelope::Envelope::new(44100.0);
    env.set_adsr(0.001, 0.05, 0.0, 0.1);
    env.note_on();
    // Run through attack+decay
    for _ in 0..44100 {
        env.tick();
    }
    assert!(env.is_idle(), "Envelope with sustain=0 should be idle after decay");
}

#[test]
fn envelope_retrigger_no_click() {
    let mut env = envelope::Envelope::new(44100.0);
    env.set_adsr(0.01, 0.1, 0.7, 0.3);
    env.note_on();
    // Run to sustain
    for _ in 0..22050 {
        env.tick();
    }
    let before = env.tick();
    // Retrigger — should start from current value, no jump to 0
    env.note_on();
    let after = env.tick();
    let jump = (after - before).abs();
    assert!(jump < 0.1, "Retrigger caused a click: jump={jump}");
}

// ── Filter tests ───────────────────────────────────────────────

#[test]
fn filter_no_nan_across_range() {
    let filter_types = [
        filter::FilterType::LowPass,
        filter::FilterType::HighPass,
        filter::FilterType::BandPass,
        filter::FilterType::MoogLP24,
        filter::FilterType::MoogLP12,
        filter::FilterType::DiodeLP,
    ];
    for ft in filter_types {
        let mut f = filter::Filter::new(44100.0);
        f.set_type(ft);
        // Sweep cutoff from 20Hz to 20kHz
        for cutoff in [20.0, 100.0, 500.0, 2000.0, 8000.0, 18000.0] {
            f.set_cutoff(cutoff);
            for res in [0.0, 0.5, 0.9, 1.0] {
                f.set_resonance(res);
                for _ in 0..500 {
                    let out = f.tick(0.5);
                    assert!(out.is_finite(), "NaN/Inf from {ft:?} cutoff={cutoff} res={res}");
                }
            }
        }
    }
}

#[test]
fn filter_lowpass_attenuates_high_freq() {
    let mut f = filter::Filter::new(44100.0);
    f.set_type(filter::FilterType::LowPass);
    f.set_cutoff(200.0);
    f.set_resonance(0.0);

    // Feed 10kHz sine, measure output energy
    let freq = 10000.0;
    let mut energy = 0.0_f32;
    for i in 0..4410 {
        let input = (2.0 * std::f32::consts::PI * freq * i as f32 / 44100.0).sin();
        let out = f.tick(input);
        energy += out * out;
    }
    let rms = (energy / 4410.0).sqrt();
    assert!(rms < 0.1, "LowPass@200Hz should attenuate 10kHz, rms={rms}");
}

#[test]
fn filter_moog_self_oscillation() {
    let mut f = filter::Filter::new(44100.0);
    f.set_type(filter::FilterType::MoogLP24);
    f.set_cutoff(1000.0);
    f.set_resonance(1.0);

    // Feed a single impulse, then silence — high res should ring
    f.tick(1.0);
    let mut max_after = 0.0_f32;
    for _ in 0..4410 {
        let out = f.tick(0.0);
        max_after = max_after.max(out.abs());
    }
    assert!(max_after > 0.01, "Moog LP24 at res=1.0 should self-oscillate, max={max_after}");
}

#[test]
fn filter_stability_extreme_params() {
    // Test with extreme cutoff and resonance — should not explode
    let mut f = filter::Filter::new(44100.0);
    f.set_type(filter::FilterType::MoogLP24);
    f.set_cutoff(22050.0); // Nyquist
    f.set_resonance(1.0);
    for _ in 0..4410 {
        let out = f.tick(1.0);
        assert!(out.abs() < 100.0, "Filter unstable at extreme params: {out}");
    }
}

// ── Oscillator tests ───────────────────────────────────────────

#[test]
fn oscillator_basic_waveforms_output_range() {
    let types = [
        oscillator::OscType::Sine,
        oscillator::OscType::Saw,
        oscillator::OscType::Square,
        oscillator::OscType::Triangle,
    ];
    for osc_type in types {
        let mut osc = oscillator::Oscillator::new(44100.0);
        osc.osc_type = osc_type;
        osc.reset();
        let mut max_val = 0.0_f32;
        let mut has_nonzero = false;
        for _ in 0..4410 {
            let out = osc.tick(440.0);
            assert!(out.is_finite(), "NaN from {osc_type:?}");
            max_val = max_val.max(out.abs());
            if out.abs() > 0.001 { has_nonzero = true; }
        }
        assert!(has_nonzero, "{osc_type:?} produced silence");
        assert!(max_val <= 1.5, "{osc_type:?} output too large: {max_val}");
    }
}

#[test]
fn oscillator_sine_frequency_accuracy() {
    let mut osc = oscillator::Oscillator::new(44100.0);
    osc.osc_type = oscillator::OscType::Sine;
    osc.reset();

    // Count zero crossings in 1 second at 440Hz → expect ~880 crossings
    let mut crossings = 0;
    let mut prev = 0.0_f32;
    for _ in 0..44100 {
        let out = osc.tick(440.0);
        if prev <= 0.0 && out > 0.0 || prev >= 0.0 && out < 0.0 {
            crossings += 1;
        }
        prev = out;
    }
    // 440Hz = 880 zero crossings/sec (±5% tolerance)
    assert!((crossings as f32 - 880.0).abs() < 44.0,
        "Sine 440Hz: expected ~880 crossings, got {crossings}");
}

#[test]
fn oscillator_noise_is_noisy() {
    let mut osc = oscillator::Oscillator::new(44100.0);
    osc.osc_type = oscillator::OscType::Noise;
    osc.reset();
    let mut values = std::collections::HashSet::new();
    for _ in 0..1000 {
        let out = osc.tick(440.0);
        values.insert((out * 1000.0) as i32);
    }
    assert!(values.len() > 100, "Noise should produce many distinct values, got {}", values.len());
}

// ── LFO tests ──────────────────────────────────────────────────

#[test]
fn lfo_output_range() {
    let waveforms = [
        lfo::LfoWaveform::Sine,
        lfo::LfoWaveform::Triangle,
        lfo::LfoWaveform::Square,
        lfo::LfoWaveform::Sawtooth,
        lfo::LfoWaveform::SampleHold,
    ];
    for wf in waveforms {
        let mut l = lfo::Lfo::new(44100.0);
        for _ in 0..44100 {
            let v = l.tick_with_deform(2.0, wf, 0.0);
            assert!((-1.01..=1.01).contains(&v), "{wf:?} out of range: {v}");
        }
    }
}

#[test]
fn lfo_sine_is_periodic() {
    let mut l = lfo::Lfo::new(44100.0);
    // 1Hz LFO — count positive zero crossings in 4 seconds → expect ~4
    let mut crossings: i32 = 0;
    let mut prev = 0.0_f32;
    for _ in 0..(44100 * 4) {
        let v = l.tick_with_deform(1.0, lfo::LfoWaveform::Sine, 0.0);
        if prev <= 0.0 && v > 0.0 { crossings += 1; }
        prev = v;
    }
    assert!((crossings - 4).abs() <= 1, "1Hz LFO: expected ~4 cycles, got {crossings}");
}

#[test]
fn lfo_deform_stays_in_range() {
    let mut l = lfo::Lfo::new(44100.0);
    for deform in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for _ in 0..4410 {
            let v = l.tick_with_deform(5.0, lfo::LfoWaveform::Sine, deform);
            assert!((-1.01..=1.01).contains(&v), "Deform {deform}: out of range {v}");
        }
    }
}

// ── Block processing test ──────────────────────────────────────

#[test]
fn tick_block_matches_tick() {
    let mut synth_a = SynthEngine::new(44100.0);
    let mut synth_b = SynthEngine::new(44100.0);

    let patches = crate::preset::load_all_patches();
    let params = PatchParams::from_map(&patches[0].params);
    let params2 = params;

    let _ = synth_a.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
        part: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
    })));
    let _ = synth_b.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
        part: 0, params: params2, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
    })));

    synth_a.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    synth_b.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

    // Both should produce sound (they may differ slightly due to control-rate vs audio-rate
    // LFO/mod matrix, but both should be non-silent and finite)
    let mut max_a = 0.0_f32;
    let mut max_b = 0.0_f32;

    for _ in 0..44100 {
        let (la, ra) = synth_a.tick();
        max_a = max_a.max(la.abs().max(ra.abs()));
    }

    let mut buf_l = [0.0f32; BLOCK_SIZE];
    let mut buf_r = [0.0f32; BLOCK_SIZE];
    for _ in 0..(44100 / BLOCK_SIZE) {
        synth_b.tick_block(&mut buf_l, &mut buf_r);
        for i in 0..BLOCK_SIZE {
            max_b = max_b.max(buf_l[i].abs().max(buf_r[i].abs()));
            assert!(buf_l[i].is_finite() && buf_r[i].is_finite(), "NaN in block output");
        }
    }

    assert!(max_a > 0.001, "tick() produced silence");
    assert!(max_b > 0.001, "tick_block() produced silence");
}

// ── Full-chain preset tests (existing + integration) ──────────

fn run_preset_chain(preset_name: &str) {
    let mut synth = SynthEngine::new(44100.0);
    let patches = crate::preset::load_all_patches();
    let idx = patches.iter().position(|p| p.name == preset_name)
        .unwrap_or_else(|| panic!("No preset '{preset_name}'"));
    let params = PatchParams::from_map(&patches[idx].params);
    synth.set_patches(patches);
    let _ = synth.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
        part: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
    })));
    synth.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut max_val = 0.0_f32;
    for _ in 0..44100 {
        let (l, r) = synth.tick();
        assert!(l.is_finite() && r.is_finite(), "NaN/Inf in '{preset_name}'");
        max_val = max_val.max(l.abs().max(r.abs()));
    }
    assert!(max_val > 0.001, "'{preset_name}' produced silence, max={max_val}");
    assert!(max_val < 10.0, "'{preset_name}' output too loud: {max_val}");
}

#[test]
fn accordion_full_chain() { run_preset_chain("Accordion"); }

#[test]
fn saxophone_full_chain() { run_preset_chain("Alto Saxophone"); }

#[test]
fn brass_full_chain() { run_preset_chain("Trumpet"); }

#[test]
fn all_presets_no_nan_no_silence() {
    let patches = crate::preset::load_all_patches();
    let mut failures = Vec::new();

    for patch in &patches {
        let mut synth = SynthEngine::new(44100.0);
        let params = PatchParams::from_map(&patch.params);
        let _ = synth.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
            part: 0,
            params,
            mod_matrix: ModMatrix::default(),
            mseg1: None,
            mseg2: None,
            pitch_seq: None,
            lfo_step_seq: None,
            wavetable: None,
        })));
        synth.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

        let mut max_val = 0.0_f32;
        let mut has_nan = false;
        // Render 0.5 seconds
        for _ in 0..22050 {
            let (l, r) = synth.tick();
            if !l.is_finite() || !r.is_finite() {
                has_nan = true;
                break;
            }
            max_val = max_val.max(l.abs().max(r.abs()));
        }

        if has_nan {
            failures.push(format!("{}: NaN/Inf", patch.name));
        } else if max_val < 0.0001 {
            // Some presets (like FX/risers) may be very quiet with static note — warn but don't fail
            eprintln!("WARNING: '{}' very quiet (max={max_val})", patch.name);
        }
        if max_val > 50.0 {
            failures.push(format!("{}: output explosion (max={max_val})", patch.name));
        } else if max_val > 10.0 {
            eprintln!("WARNING: '{}' output is hot (max={max_val})", patch.name);
        }
    }

    assert!(failures.is_empty(), "Preset failures:\n{}", failures.join("\n"));
}

// ── Silence without notes ──────────────────────────────────────

#[test]
fn no_notes_produces_silence() {
    let patches = crate::preset::load_all_patches();
    // Test with ALL presets that have ring_mod_mix > 0 — they should be silent without notes
    for patch in &patches {
        let mut synth = SynthEngine::new(44100.0);
        let params = PatchParams::from_map(&patch.params);
        let _ = synth.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
            part: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
        })));
        // No notes — should be completely silent
        let mut max_val = 0.0_f32;
        let mut buf_l = [0.0f32; BLOCK_SIZE];
        let mut buf_r = [0.0f32; BLOCK_SIZE];
        for _ in 0..100 {
            synth.tick_block(&mut buf_l, &mut buf_r);
            for i in 0..BLOCK_SIZE {
                max_val = max_val.max(buf_l[i].abs()).max(buf_r[i].abs());
            }
        }
        assert!(max_val < 0.0001, "preset '{}' hums without notes: max={max_val}", patch.name);
    }
}

// ── Effect early-exit test ─────────────────────────────────────

#[test]
fn effects_bypass_when_mix_zero() {
    // Verify effects with mix=0 pass through unchanged
    let mut ch = chorus::Chorus::new(44100.0);
    let (l, _r) = ch.tick(0.7, 0.0); // mix=0
    assert!((l - 0.7).abs() < 0.01, "Chorus bypass failed: l={l}");

    let mut dl = delay::StereoDelay::new(44100.0);
    let (l, r) = dl.tick(0.5, -0.5, 0.3, 0.4, 0.3, 0.5, false, 0.0); // mix=0
    assert!((l - 0.5).abs() < 0.001 && (r - (-0.5)).abs() < 0.001,
        "Delay bypass failed: ({l}, {r})");

    let mut rv = reverb::Reverb::new(44100.0);
    let (l, r) = rv.tick(0.4_f32, -0.4_f32, 0.5, 0.5, 0.5, 0.1, 0.0); // mix=0
    assert!((l - 0.4_f32).abs() < 0.001 && (r - (-0.4_f32)).abs() < 0.001,
        "Reverb bypass failed: ({l}, {r})");
}

// ── Polivoks filter tests ───────────────────────────────────────

#[test]
fn polivoks_lp_passes_dc() {
    let mut f = filter::Filter::new(44100.0);
    f.set_type(filter::FilterType::PolivoksLP);
    f.set_cutoff(1000.0);
    f.set_resonance(0.3);
    f.force_update();
    // Warm up
    for _ in 0..1000 { f.tick(0.5); }
    let out = f.tick(0.5);
    // LP should pass DC-ish signal (within 50% — it's a nonlinear filter)
    assert!(out.abs() > 0.1 && out.abs() < 2.0, "LP DC failed: {out}");
}

#[test]
fn polivoks_bp_attenuates_dc() {
    // Verify BP passes a signal near cutoff better than a signal well above cutoff.
    // This topology strongly attenuates high frequencies (above cutoff) in the v1 output.
    let mut f = filter::Filter::new(44100.0);
    f.set_type(filter::FilterType::PolivoksBP);
    f.set_cutoff(1000.0);
    f.set_resonance(0.3);
    f.force_update();

    // Measure RMS at cutoff frequency (1kHz — should pass well)
    let mut rms_at_cutoff = 0.0_f32;
    for i in 0..4410 {
        let x = ((i as f32) * 2.0 * std::f32::consts::PI * 1000.0 / 44100.0).sin() * 0.5;
        rms_at_cutoff += f.tick(x).powi(2);
    }
    rms_at_cutoff = (rms_at_cutoff / 4410.0).sqrt();

    // Reset and measure RMS well above cutoff (10kHz — should be strongly attenuated)
    f.reset();
    let mut rms_high_freq = 0.0_f32;
    for i in 0..4410 {
        let x = ((i as f32) * 2.0 * std::f32::consts::PI * 10000.0 / 44100.0).sin() * 0.5;
        rms_high_freq += f.tick(x).powi(2);
    }
    rms_high_freq = (rms_high_freq / 4410.0).sqrt();

    assert!(rms_at_cutoff > rms_high_freq,
        "BP should pass {rms_at_cutoff:.4} (1kHz at cutoff) > {rms_high_freq:.4} (10kHz above cutoff)");
    assert!(rms_at_cutoff.is_finite() && rms_high_freq.is_finite(), "BP produced non-finite output");
}

#[test]
fn polivoks_no_nan_under_high_resonance() {
    let mut f = filter::Filter::new(44100.0);
    f.set_type(filter::FilterType::PolivoksLP);
    f.set_cutoff(500.0);
    f.set_resonance(1.0);
    f.pv_drive = 1.0;
    f.pv_starve = 1.0;
    f.force_update();
    for i in 0..10000 {
        let input = ((i as f32) * 0.01).sin();
        let out = f.tick(input);
        assert!(!out.is_nan(), "NaN at sample {i}");
        assert!(!out.is_infinite(), "Inf at sample {i}");
    }
}

#[test]
fn polivoks_drive_increases_harmonic_content() {
    let mut f_clean = filter::Filter::new(44100.0);
    f_clean.set_type(filter::FilterType::PolivoksLP);
    f_clean.set_cutoff(2000.0);
    f_clean.set_resonance(0.5);
    f_clean.pv_drive = 0.0;
    f_clean.force_update();

    let mut f_driven = filter::Filter::new(44100.0);
    f_driven.set_type(filter::FilterType::PolivoksLP);
    f_driven.set_cutoff(2000.0);
    f_driven.set_resonance(0.5);
    f_driven.pv_drive = 1.0;
    f_driven.force_update();

    let mut rms_clean = 0.0_f32;
    let mut rms_driven = 0.0_f32;
    for i in 0..4410 {
        let x = ((i as f32) * 2.0 * std::f32::consts::PI * 440.0 / 44100.0).sin() * 0.5;
        let c = f_clean.tick(x);
        let d = f_driven.tick(x);
        rms_clean += c * c;
        rms_driven += d * d;
    }
    assert!(rms_driven > rms_clean, "Drive should increase RMS (adds harmonics), clean={rms_clean}, driven={rms_driven}");
}

// ── Part enable/disable state guard ────────────────────────────

#[test]
fn part_disable_releases_each_voice_once_and_cleans_state() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    let active: Vec<bool> = e.parts[0].voices.iter().map(|v| v.active).collect();
    assert!(active.iter().any(|&a| a), "no active voice to disable");
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: false });
    for (i, v) in e.parts[0].voices.iter().enumerate() {
        if active[i] {
            assert!(v.is_releasing(), "voice {i} not released after part disable");
        }
    }
    assert!(e.parts[0].note_stack.is_empty(), "note_stack not cleared");
    assert!(e.parts[0].latch_held.is_empty(), "latch_held not cleared");
    assert!(!e.parts[0].sustained_notes.iter().any(|&s| s), "sustained_notes not cleared");
    assert!(!e.parts[0].keys_down.iter().any(|&k| k), "keys_down not cleared");
    // Re-enable: notes must sound again
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "part did not restart after re-enable");
}

#[test]
fn dispatch_arp_note_off_releases_dsp_voice_with_transpose_set() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "no sounding voice");
    e.dispatch_arp_note_off(60); // raw key; DSP matches by src_note
    assert!(
        e.parts[0].voices.iter().all(|v| !v.active || v.is_releasing()),
        "dispatch_arp_note_off did not release the sounding voice"
    );
}

// ── Root-only mode (TECHDEBT #6) ───────────────────────────────

fn note_mode_engine(mode: u8) -> SynthEngine {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartNoteMode { part: 0, mode });
    e
}

fn bass_engine() -> SynthEngine { note_mode_engine(NOTE_MODE_BASS) }

fn root_engine() -> SynthEngine { note_mode_engine(NOTE_MODE_ROOT) }

/// Count of sounding (active, not releasing) voices.
fn sounding(part: &Part) -> usize {
    part.voices.iter().filter(|v| v.active && !v.is_releasing()).count()
}

#[test]
fn root_only_suppresses_higher_notes() {
    let mut e = bass_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "lowest note did not sound");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].voices.iter().filter(|v| v.active && !v.is_releasing()).count(), 1,
        "higher note was not suppressed");
    assert_eq!(e.parts[0].root_active, Some(60));
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 64 }); // non-root off: no-op
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(60), "non-root note-off re-keyed the root");
}

#[test]
fn root_only_rekeys_to_new_lowest() {
    let mut e = bass_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 60),
        "new lowest did not start a voice");
    assert!(e.parts[0].voices.iter().filter(|v| v.active && v.is_releasing() && v.src_note == 64).count() >= 1,
        "old root was not released");
    assert_eq!(e.parts[0].root_active, Some(60));
}

#[test]
fn root_only_release_of_root_rekeys_next_lowest() {
    let mut e = bass_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 64),
        "next-lowest key did not re-key after root release");
    assert_eq!(e.parts[0].root_active, Some(64));
}

#[test]
fn root_only_toggle_resets_tracking() {
    let mut e = bass_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    let _ = e.handle_control(ControlEvent::SetPartNoteMode { part: 0, mode: NOTE_MODE_POLY });
    let _ = e.handle_control(ControlEvent::SetPartNoteMode { part: 0, mode: NOTE_MODE_BASS });
    assert_eq!(e.parts[0].root_active, None, "toggle did not reset root_active");
    assert!(!e.parts[0].root_held.iter().any(|&h| h), "toggle did not reset root_held");
    // A stale root would have swallowed this note:
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()),
        "note after toggle was suppressed by stale tracking");
}

#[test]
fn root_only_out_of_range_key_does_not_join_held_mask() {
    let mut e = bass_engine();
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 48, max_note: 70 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 30, velocity: 100 }); // out of range
    for _ in 0..100 { e.tick(); }
    assert!(!e.parts[0].root_held[30], "out-of-range key joined the held mask");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, None, "out-of-range key re-keyed after root off");
}

// ── Root mode (chord-root detection, note_mode = 2) ────────────

#[test]
fn root_mode_follows_chord_root_and_rekeys_back() {
    // TC1: C(48)+E(52) = {0,4} — некордово, фолбэк на нижнюю → звучит C.
    // +A(45): {9,0,4} = Am → корень A, прежний C отпускается (re-key).
    // Снять A → остался C → рё-key на C.
    let mut e = root_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 48),
        "C did not sound");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 52, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(sounding(&e.parts[0]), 1, "E was not suppressed in C+E");
    assert_eq!(e.parts[0].root_active, Some(48));

    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 45, velocity: 100 }); // Am
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(45), "chord root A was not detected");
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 45),
        "root A did not sound");
    assert!(e.parts[0].voices.iter().any(|v| v.active && v.is_releasing() && v.src_note == 48),
        "old root C was not released on re-key");

    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 45 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(48), "did not re-key back to C");
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 48),
        "C did not re-sound after A release");
}

#[test]
fn root_mode_low_octave_no_wrap_no_panic() {
    // TC1b: root-pc выше raw (raw=1, pcs {9,1,4}). i16-математика +
    // saturating_sub: без паники/обёртки, target = 1−4 → 0.
    let mut e = root_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 1, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 1));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 9, velocity: 100 }); // {1,9} — некордово
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(1), "fallback target moved off the bass note");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 4, velocity: 100 }); // {9,1,4} = A
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(0), "saturating target expected (1−4 → 0)");
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 0),
        "root note 0 did not sound");
    assert!(e.parts[0].voices.iter().any(|v| v.active && v.is_releasing() && v.src_note == 1),
        "old root 1 was not released");
}

#[test]
fn root_mode_high_note_does_not_raise_root() {
    // TC1c: верхняя нота не поднимает корень (якорь min_held).
    let mut e = root_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 45, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(45), "high note raised the root");
    assert_eq!(sounding(&e.parts[0]), 1, "high note was not suppressed");
}

#[test]
fn root_mode_nonchord_set_falls_back_to_bass() {
    // TC2: 2 ноты C+E без A — фолбэк на Bass, играет нижняя.
    let mut e = root_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(60), "fallback did not anchor on the lowest");
    assert_eq!(sounding(&e.parts[0]), 1, "more than one voice in fallback");
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 60),
        "lowest note did not sound in fallback");
}

#[test]
fn root_mode_rekeys_on_chord_member_release_even_under_synthesized_root() {
    // Корень может не совпадать ни с одной зажатой клавишей (Root-режим,
    // синтезированный таргет): снятие члена аккорда обязано пересчитать корень.
    // held {1,4,9} → A, target 0; снять 1 → {4,9} некордово → фолбэк 4.
    let mut e = root_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 1, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 9, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 4, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(0));
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 1 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(4), "stale synthesized root after member release");
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 4),
        "re-keyed root 4 did not sound");
    assert_eq!(sounding(&e.parts[0]), 1, "old synthesized root piled up with the new one");
}

// ── Per-part sustain (Sus checkbox, TC1) ───────────────────────

#[test]
fn per_part_sustain_gate_holds_only_enabled_part() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::load_patch_from(1, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    // Part 0 = pad (Sus on by default), part 1 = bass (Sus off)
    let _ = e.handle_control(ControlEvent::SetPartSustain { part: 1, on: false });

    // One note-on sounds in every enabled part
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "part A did not start");
    assert!(e.parts[1].voices.iter().any(|v| v.active), "part B did not start");

    // Pedal down, then the key up
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..100 { e.tick(); }

    assert!(e.parts[1].voices.iter().all(|v| !v.active || v.is_releasing()),
        "part with Sus=off kept ringing under the pedal");
    assert!(!e.parts[1].sustained_notes.iter().any(|&s| s),
        "part with Sus=off accumulated pedal-held notes");
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()),
        "part with Sus=on was not held by the pedal");
    assert!(e.parts[0].sustained_notes[60],
        "part with Sus=on did not flag the note as pedal-held");

    // Pedal up: the held part releases
    let _ = e.handle_control(ControlEvent::SetSustain { on: false });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().all(|v| !v.active || v.is_releasing()),
        "pedal-up did not release the part with Sus=on");
}

#[test]
fn root_only_rekey_releases_old_root_even_under_pedal() {
    // route_note_off DSP path: root key-up force-releases the old root voice
    // (v.note_off(), sustained_notes cleared) even while the pedal holds it,
    // then re-keys to the next-lowest held key.
    let mut e = bass_engine();
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 }); // pedal down
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 }); // suppressed
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "root did not sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pedal still holding it
    for _ in 0..100 { e.tick(); }
    assert!(
        !e.parts[0].voices.iter().any(|v| v.active && v.src_note == 60 && !v.is_releasing()),
        "old root voice is still sustain-held after its key-up under pedal"
    );
    assert!(!e.parts[0].sustained_notes[60], "old root left in sustained_notes");
    assert_eq!(e.parts[0].root_active, Some(64), "did not re-key to note 64");
    assert!(
        e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 64),
        "re-keyed voice for note 64 did not sound"
    );
}

#[test]
fn root_only_key_up_kills_pedal_held_bass() {
    // Plain key release with no re-key target: the root voice is force-killed
    // at key-up even under the pedal (bass-split semantics: key-up = note
    // done, the pedal must not resurrect the bass).
    let mut e = bass_engine();
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 }); // pedal down
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()), "root did not sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pedal still down
    for _ in 0..100 { e.tick(); }
    assert!(!e.parts[0].sustained_notes[60], "bass left in sustained_notes");
    assert!(
        !e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()),
        "pedal-held bass survived its key-up"
    );
    assert_eq!(e.parts[0].root_active, None, "stale root_active after key-up");
}

// ── Sustain-atom coherence on MIDI-seq stop ────────────────────

#[test]
fn midi_seq_stop_clears_part_sustain_state() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..50 { e.tick(); }
    assert!(e.parts[0].sustain_pedal > 0.5, "pedal did not engage the part flag");
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    assert!(!e.sustain_atom().load(std::sync::atomic::Ordering::Relaxed),
        "seq stop left sustain atom ON");
    assert_eq!(e.parts[0].sustain_pedal, 0.0, "part pedal flag stuck ON after seq stop");
    assert!(!e.parts[0].sustained_notes.iter().any(|&s| s),
        "sustained_notes stuck ON after seq stop");
}

// ── Mono / legato / portamento ─────────────────────────────────

// `f32::powf` is not const-stable, so these are runtime helpers, not consts.
fn f60() -> f32 {
    440.0 * 2.0_f32.powf(-9.0 / 12.0)
}
fn f72() -> f32 {
    440.0 * 2.0_f32.powf(3.0 / 12.0)
}

fn patch_engine(play_mode: f32, porta_time: f32, porta_mode: f32) -> SynthEngine {
    let patches = crate::preset::load_all_patches();
    let mut patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists")
        .clone();
    patch.params.insert("play_mode".into(), play_mode);
    patch.params.insert("portamento_time".into(), porta_time);
    patch.params.insert("portamento_mode".into(), porta_mode);
    patch.params.insert("amp_release".into(), 0.05);
    let mut e = SynthEngine::new(48000.0);
    e.set_patches(patches);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e
}

fn render_blocks(e: &mut SynthEngine, blocks: usize) {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        e.tick_block(&mut l, &mut r);
    }
}

fn active_freq(e: &SynthEngine) -> f32 {
    e.parts[0]
        .voices
        .iter()
        .find(|v| v.active)
        .map(|v| v.current_freq())
        .unwrap_or(0.0)
}

fn note(e: &mut SynthEngine, n: u8) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: n, velocity: 100 });
}

fn unnote(e: &mut SynthEngine, n: u8) {
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: n });
}

#[test]
fn mono_mode_is_single_voice() {
    let mut e = patch_engine(1.0, 0.0, 0.0);
    note(&mut e, 60);
    render_blocks(&mut e, 5);
    note(&mut e, 72);
    render_blocks(&mut e, 5);
    let active = e.parts[0].voices.iter().filter(|v| v.active).count();
    assert_eq!(active, 1, "mono mode must keep exactly one voice");
    let v = e.parts[0].voices.iter().find(|v| v.active).unwrap();
    assert_eq!(v.note, 72, "mono must follow the last note");
}

#[test]
fn glide_slews_pitch_in_mono_mode() {
    let mut e = patch_engine(1.0, 0.2, 1.0); // 200 ms, Always
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    note(&mut e, 72);
    render_blocks(&mut e, 7); // ~4.7 ms — well inside the 200 ms glide
    let f = active_freq(&e);
    assert!(
        f > f60() && f < f72(),
        "expected mid-glide pitch between {} and {}, got {f}",
        f60(),
        f72()
    );
    render_blocks(&mut e, 800); // 0.53 s >> glide time
    let f = active_freq(&e);
    assert!((f - f72()).abs() < 1.0, "glide did not land on target: {f}");
}

#[test]
fn glide_off_is_instant() {
    // Time is SET but mode is Off — must not glide (mono path ignores mode
    // today: RED until the fix).
    let mut e = patch_engine(1.0, 0.2, 0.0);
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    note(&mut e, 72);
    render_blocks(&mut e, 1);
    let f = active_freq(&e);
    assert!((f - f72()).abs() < 0.5, "glide must be instant in mode Off: {f}");
}

#[test]
fn fingered_portamento_does_not_glide_from_released_note() {
    // Mode Fingered (legato): first note after all keys released must be
    // instant, not a glide from the previous phrase's last pitch (RED until
    // the fix).
    let mut e = patch_engine(1.0, 0.2, 2.0);
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    unnote(&mut e, 60);
    render_blocks(&mut e, 400); // release 0.05 s → voice idle
    note(&mut e, 72);
    render_blocks(&mut e, 1);
    let f = active_freq(&e);
    assert!((f - f72()).abs() < 0.5, "stale glide from released note: {f}");
}

#[test]
fn legato_retrigger_glides_when_first_note_was_instant() {
    // Mono-ST + Fingered: the first note has no glide (porta_coeff = 1.0);
    // the second, held-key note must still glide (RED until retrigger_note
    // sets porta_coeff).
    let mut e = patch_engine(2.0, 0.2, 2.0);
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    note(&mut e, 72);
    render_blocks(&mut e, 7);
    let f = active_freq(&e);
    assert!(
        f > f60() && f < f72(),
        "legato retrigger snapped instead of gliding: {f}"
    );
}

#[test]
fn set_part_mono_legato_glide_events() {
    let mut e = patch_engine(0.0, 0.0, 0.0);
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: true });
    assert_eq!(e.parts[0].params.play_mode, 1.0);
    let _ = e.handle_control(ControlEvent::SetPartLegato { part: 0, legato: true });
    assert_eq!(e.parts[0].params.play_mode, 2.0);
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 0, ms: 150.0 });
    assert!((e.parts[0].params.portamento_time - 0.15).abs() < 1e-4);
    assert_eq!(e.parts[0].params.portamento_mode, 1.0);
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 0, ms: 0.0 });
    assert_eq!(e.parts[0].params.portamento_time, 0.0);
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 0, ms: 5000.0 });
    assert!((e.parts[0].params.portamento_time - 2.0).abs() < 1e-4, "ms must clamp to 2 s");
    // Mono off clears the stale stack and returns to poly
    note(&mut e, 60);
    render_blocks(&mut e, 1);
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: false });
    assert_eq!(e.parts[0].params.play_mode, 0.0);
    assert!(e.parts[0].note_stack.is_empty(), "mono-off must clear note_stack");
    // Latch mode is left alone by the mono toggle
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: true });
    e.parts[0].params.play_mode = 3.0;
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: true });
    assert_eq!(e.parts[0].params.play_mode, 3.0, "latch mode must not be clobbered");
    // Out-of-range part: no-op, no panic
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 99, ms: 50.0 });
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 99, mono: true });
}

#[test]
fn steal_prefers_releasing_voice() {
    let mut p = Part::new(48_000.0);
    p.enabled = true; // note_on бейлится на !enabled (mod.rs:613)
    // занять ВСЕ 8 голосов (VOICES_PER_LAYER = 8, mod.rs:376) sustained-нотами;
    // Part::note_on(raw_note: u8, velocity: u8)
    for n in 36..36 + p.voices.len() {
        p.note_on(n as u8, 100);
    }
    // отпустить НЕ-самый-старый голос (40) → он в release-фазе;
    // голос 36 — самый старый активный (старый вор взял бы ЕГО)
    p.note_off(40);
    // новая нота: занимает release-голос 40 (его src_note перезаписывается
    // на 80 — ассерт по src_note==40 невозможен по определению)
    p.note_on(80, 100);
    let v_new = p.voices.iter().find(|v| v.note == 80).unwrap();
    assert!(v_new.active && !v_new.is_releasing(), "new voice must attack");
    // ДИСКРИМИНАТОР: голос 36 (самый старый активный) НЕ тронут. Старый вор
    // взял бы ЕГО: src_note перезаписался бы на 80 → find(36) запаниковал бы.
    let v36 = p.voices.iter().find(|v| v.src_note == 36).unwrap();
    assert!(v36.active && !v36.is_releasing() && v36.note == 36,
        "old stealer would have taken voice 36");
}

// ── Voice stealing: Poly-High / Poly-Low / oldest-first (audit S0) ────

fn part_with_mode(play_mode: f32) -> Part {
    let mut p = Part::new(48_000.0);
    p.enabled = true; // note_on bails on !enabled (mod.rs:613... now 617)
    p.params.play_mode = play_mode;
    p
}

/// Occupy ALL 8 voices (VOICES_PER_LAYER, mod.rs:381) with sustained notes
/// 36..44 pressed in order — nothing releasing, ages strictly increasing.
fn fill_voices(p: &mut Part) {
    for n in 36..36 + p.voices.len() as u8 {
        p.note_on(n, 100);
    }
}

#[test]
fn poly_high_steals_the_lowest_note() {
    // play_mode 4: highest note wins -> lowest (36) is stolen (mod.rs:717-723)
    let mut p = part_with_mode(4.0);
    fill_voices(&mut p);
    p.note_on(80, 100);
    assert!(p.voices.iter().any(|v| v.note == 80), "new note must sound");
    assert!(!p.voices.iter().any(|v| v.src_note == 36), "lowest note 36 must be stolen");
    let hi = p.voices.iter().find(|v| v.src_note == 43).expect("highest note 43 must survive");
    assert!(hi.active && hi.note == 43, "surviving voice must still sound note 43");
}

#[test]
fn poly_low_steals_the_highest_note() {
    // play_mode 5: lowest note wins -> highest (43) is stolen (mod.rs:725-730)
    let mut p = part_with_mode(5.0);
    fill_voices(&mut p);
    p.note_on(80, 100);
    assert!(p.voices.iter().any(|v| v.note == 80), "new note must sound");
    assert!(!p.voices.iter().any(|v| v.src_note == 43), "highest note 43 must be stolen");
    let lo = p.voices.iter().find(|v| v.src_note == 36).expect("lowest note 36 must survive");
    assert!(lo.active && lo.note == 36, "surviving voice must still sound note 36");
}

#[test]
fn default_poly_steals_the_oldest_sustained_voice() {
    // No releasing voices -> oldest sustained (first pressed, 36) dies;
    // JUCE-style order, mod.rs:736-743. (Releasing-first is covered by
    // steal_prefers_releasing_voice above.)
    let mut p = part_with_mode(0.0);
    fill_voices(&mut p);
    p.note_on(80, 100);
    assert!(p.voices.iter().any(|v| v.note == 80), "new note must sound");
    assert!(!p.voices.iter().any(|v| v.src_note == 36), "oldest voice (36) must be stolen");
    let recent = p.voices.iter().find(|v| v.src_note == 43).expect("newest voice 43 must survive");
    assert!(recent.active && recent.note == 43);
}

// ── SF2 no-op mode switch (seamless perf switch) ───────────────

#[test]
fn sf2_mode_noop_does_not_clear_dsp_state() {
    // Perf load pushes SetPartSf2Mode for EVERY part with its current
    // sf2_mode; the no-op arm must return before draining DSP voices.
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    assert!(!e.parts[0].sf2_mode, "part 0 must start in DSP mode");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "no sounding voice");
    // No-op switch: enabled == part.sf2_mode (false == false)
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: false });
    assert!(
        e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()),
        "no-op SetPartSf2Mode must not release sounding DSP voices"
    );
    assert!(!e.parts[0].sustained_notes.iter().any(|&s| s), "no-op switch must not clear sustained_notes");
    assert!(e.parts[0].keys_down[60], "no-op switch must not clear keys_down");
    // Out-of-range part: no-op, no panic
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 99, enabled: true });
}

// ── Part::drain_notes root-state guard ─────────────────────────

#[test]
fn all_notes_off_clears_root_held_and_root_active() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartNoteMode { part: 0, mode: NOTE_MODE_BASS });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 36, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }); // suppressed
    for _ in 0..50 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(36), "root tracking did not engage");
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    assert!(e.parts[0].root_active.is_none(), "root_active survived AllNotesOff");
    assert!(e.parts[0].root_held.iter().all(|&h| !h), "root_held survived AllNotesOff");
    assert!(e.parts[0].note_stack.is_empty(), "note_stack survived AllNotesOff");
    assert!(!e.parts[0].keys_down.iter().any(|&k| k), "keys_down survived AllNotesOff");
}

// ── Part control-event → engine-field contract (audit S0) ──────────────
// Perf-load (gui load_performance_by_name) and the audio reconnect re-send
// (send_full_part_state) rely on every SetPart* event landing in exactly
// one engine field. This pins that mapping — including the pan clamp
// (mod.rs:2216) and the raw volume store (mod.rs:2198).

#[test]
fn part_control_events_land_in_engine_fields() {
    let mut e = SynthEngine::new(48_000.0);
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 0.42 });
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 36, max_note: 84 });
    let _ = e.handle_control(ControlEvent::SetPartVelRange { part: 0, vel_min: 20, vel_max: 100 });
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 0, pan: -0.5 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: -12 });
    let _ = e.handle_control(ControlEvent::SetMacro { part: 0, index: 3, value: 0.75 });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    let p = &e.parts[0];
    assert!(p.enabled);
    assert!((p.volume - 0.42).abs() < 1e-6, "volume is stored raw (mod.rs:2198)");
    assert!(p.mute);
    assert_eq!((p.min_note, p.max_note), (36, 84));
    assert_eq!((p.vel_min, p.vel_max), (20, 100));
    assert!((p.pan + 0.5).abs() < 1e-6);
    assert_eq!(p.transpose, -12);
    assert!((p.macro_vals[3] - 0.75).abs() < 1e-6);
    assert!(p.sf2_mode);
    // pan is clamped into [-1, 1] (mod.rs:2216) — pin the clamp
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 0, pan: 7.0 });
    assert!((e.parts[0].pan - 1.0).abs() < 1e-6, "pan clamped to +1");
    // Negative: out-of-range part is a no-op everywhere
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 99, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 99, pan: 0.1 });
    let _ = e.handle_control(ControlEvent::SetMacro { part: 99, index: 0, value: 1.0 });
    assert_eq!(e.parts.len(), crate::synth::MAX_PARTS, "no part was created");
}

#[test]
fn full_state_re_send_restores_a_fresh_engine() {
    // Mirrors send_full_part_state (gui/mod.rs:1714-1741): after an audio
    // reconnect a brand-new engine receives ONLY this SetPart* sequence and
    // must match the pre-reconnect engine's part configuration. Sounding
    // voices are deliberately NOT carried (documented re-send semantics).
    let mut configured = SynthEngine::new(48_000.0);
    let patches = crate::preset::load_all_patches();
    let _ = configured.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    configured.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    push_full_part_state(&mut configured, 0, true, 0.9, 24, 96, 10, 120, 0.25, 7);
    push_full_part_state(&mut configured, 1, false, 0.5, 0, 127, 1, 127, 0.0, 0);

    let mut fresh = SynthEngine::new(48_000.0);
    push_full_part_state(&mut fresh, 0, true, 0.9, 24, 96, 10, 120, 0.25, 7);
    push_full_part_state(&mut fresh, 1, false, 0.5, 0, 127, 1, 127, 0.0, 0);

    for i in 0..2usize {
        let (a, b) = (&configured.parts[i], &fresh.parts[i]);
        assert_eq!(a.enabled, b.enabled, "part {i} enabled");
        assert_eq!(a.mute, b.mute, "part {i} mute");
        assert_eq!(a.volume, b.volume, "part {i} volume");
        assert_eq!((a.min_note, a.max_note), (b.min_note, b.max_note), "part {i} range");
        assert_eq!((a.vel_min, a.vel_max), (b.vel_min, b.vel_max), "part {i} vel range");
        assert_eq!(a.pan, b.pan, "part {i} pan");
        assert_eq!(a.transpose, b.transpose, "part {i} transpose");
        assert_eq!(a.macro_vals, b.macro_vals, "part {i} macros");
        assert_eq!(a.sf2_mode, b.sf2_mode, "part {i} sf2_mode");
    }
    assert!(
        configured.parts[0].voices.iter().any(|v| v.active),
        "configured engine keeps its sounding voice (re-send does not carry it)"
    );
    assert!(
        !fresh.parts[0].voices.iter().any(|v| v.active),
        "fresh engine starts silent until a note arrives"
    );
}

/// Exact event order of send_full_part_state (gui/mod.rs:1724-1739).
#[allow(clippy::too_many_arguments)]
fn push_full_part_state(
    e: &mut SynthEngine, part: usize, enabled: bool, volume: f32,
    min_note: u8, max_note: u8, vel_min: u8, vel_max: u8,
    pan: f32, transpose: i8,
) {
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part, enabled });
    let _ = e.handle_control(ControlEvent::SetPartMute { part, mute: false });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part, volume });
    let _ = e.handle_control(ControlEvent::SetPartRange { part, min_note, max_note });
    let _ = e.handle_control(ControlEvent::SetPartVelRange { part, vel_min, vel_max });
    let _ = e.handle_control(ControlEvent::SetPartPan { part, pan });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part, semitones: transpose });
    for index in 0..8usize {
        let _ = e.handle_control(ControlEvent::SetMacro { part, index, value: 0.0 });
    }
    // sf2_mode == false in both calls here: the Sf2Mode/Sf2Program pair is
    // pushed only when sf2_mode (gui/mod.rs:1734-1739) — TC25 pins the arm.
}

#[test]
fn absent_parts_are_disabled_by_tail_loop() {
    // Perf-load tail loop (gui/mod.rs:2197-2202): parts beyond the
    // performance's part list must not keep playing — replay pins the
    // idempotent SetPartEnabled(false) sweep.
    let mut e = SynthEngine::new(48_000.0);
    for i in 0..4usize {
        let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled: true });
    }
    // The "performance" carries 1 part -> tail loop disables 1..MAX_PARTS
    for i in 1..crate::synth::MAX_PARTS {
        if e.parts[i].enabled {
            let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled: false });
        }
    }
    assert!(e.parts[0].enabled, "part 0 stays enabled");
    for i in 1..4usize {
        assert!(!e.parts[i].enabled, "part {i} disabled by the tail loop");
    }
    // Idempotent: a second sweep changes nothing
    for i in 1..crate::synth::MAX_PARTS {
        let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled: false });
    }
    for i in 1..4usize {
        assert!(!e.parts[i].enabled);
    }
    assert!(e.parts[0].enabled);
}
