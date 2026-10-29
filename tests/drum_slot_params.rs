//! Per-slot L/T/D/P (DrumSetParam → DrumEngine.params) are DSP-only by design
//! (TECHDEBT #33): in SF2 mode the sequencer trigger path returns the hit to
//! the caller for sampler.drum_note_on (synth/mod.rs tick_block) and never
//! runs trigger_slot or the level/pan mix, so SF2 kits ignore these params.

use mini_midi_synth::synth::drum::DrumEngine;
use mini_midi_synth::synth::{ControlEvent, DrumParam, SynthEngine};

const SR: f32 = 48_000.0;
const KICK: usize = 0;
/// 120 BPM, 16ths: samples per sequencer step. The step fires at the END of
/// its window (phase_acc >= samples_per_step), so measure two windows.
const STEP_SAMPLES: usize = 6000;

#[test]
fn tc1_drum_set_param_updates_engine_state() {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetParam { slot: KICK as u8, param: DrumParam::Level(0.25) });
    let _ = e.handle_control(ControlEvent::DrumSetParam { slot: KICK as u8, param: DrumParam::Tune(7.0) });
    let _ = e.handle_control(ControlEvent::DrumSetParam { slot: KICK as u8, param: DrumParam::Decay(2.5) });
    let _ = e.handle_control(ControlEvent::DrumSetParam { slot: KICK as u8, param: DrumParam::Pan(-0.5) });
    let p = &e.drum_engine.params[KICK];
    assert_eq!((p.level, p.tune, p.decay, p.pan), (0.25, 7.0, 2.5, -0.5));
}

fn arm_kick_step0(e: &mut DrumEngine) {
    e.sequencer.patterns[0].steps[KICK][0].velocity = 100;
    e.sequencer.playing = true;
}

fn rms(e: &mut DrumEngine, samples: usize) -> f32 {
    let mut acc = 0.0_f32;
    for _ in 0..samples {
        let ((l, r), _) = e.tick();
        acc += l * l + r * r;
    }
    acc.sqrt()
}

#[test]
fn tc2_dsp_mode_level_scales_output() {
    let mut loud = DrumEngine::new(SR);
    arm_kick_step0(&mut loud);
    loud.params[KICK].level = 1.0;
    let loud_rms = rms(&mut loud, 2 * STEP_SAMPLES);

    let mut quiet = DrumEngine::new(SR);
    arm_kick_step0(&mut quiet);
    quiet.params[KICK].level = 0.1;
    let quiet_rms = rms(&mut quiet, 2 * STEP_SAMPLES);

    assert!(loud_rms > 0.01, "kick should sound in DSP mode");
    assert!(loud_rms > quiet_rms * 5.0, "level slider must scale DSP mix: {loud_rms} vs {quiet_rms}");
}

#[test]
fn tc3_sf2_mode_no_dsp_sound_and_trigger_routed_out() {
    let mut e = DrumEngine::new(SR);
    arm_kick_step0(&mut e);
    e.params[KICK].level = 1.0;
    e.sf2_mode = true;

    let mut fired = false;
    let mut acc = 0.0_f32;
    for _ in 0..2 * STEP_SAMPLES {
        let ((l, r), tr) = e.tick();
        acc += l * l + r * r;
        if tr.count > 0 {
            fired = true;
            assert!(tr.triggers[..tr.count].contains(&(KICK as u8, 100u8)));
        }
    }
    assert!(fired, "SF2 mode must hand the hit to the sampler via DrumTriggers");
    assert!(acc.sqrt() < 1e-3, "SF2 mode must produce no DSP voice audio, got {}", acc.sqrt());
}
