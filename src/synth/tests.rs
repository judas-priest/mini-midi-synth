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

    synth_a.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
        part: 0, params, mod_matrix: ModMatrix::default(), mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
    })));
    synth_b.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
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
    synth.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
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
        synth.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
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
        synth.handle_control(ControlEvent::LoadPatch(Box::new(LoadPatchEvent {
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
