//! Quantized master recording: looper starts at the pattern boundary,
//! auto-closes after `bars` bars, early stop is boundary-quantized (#30 sync).

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, SynthEngine};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

fn engine() -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    e
}

/// Advance `blocks` audio blocks (tick_block takes two buffers, len = slice).
fn run(e: &mut SynthEngine, blocks: usize) {
    let mut l = [0.0f32; BLOCK];
    let mut r = [0.0f32; BLOCK];
    for _ in 0..blocks {
        e.tick_block(&mut l, &mut r);
    }
}

/// Blocks per 16th step at 120 BPM: 48000*60/(120*4) = 6000 samples/step;
/// 6000/64 = 93.75 -> 94.
fn steps(blocks: usize) -> usize { blocks * 94 }

#[test]
fn tc1_start_at_boundary() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay); // drums playing from step 0
    run(&mut e, steps(5)); // playhead ~step 5
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(e.master_rec_pending_atom().load(Ordering::Relaxed), "armed");
    assert!(matches!(e.looper.state, LooperState::Idle), "not capturing until boundary");
    run(&mut e, steps(12)); // crosses the boundary
    assert!(!e.master_rec_pending_atom().load(Ordering::Relaxed));
    assert!(matches!(e.looper.state, LooperState::Recording), "capture started at boundary");
}

#[test]
fn tc2_auto_close_after_one_bar() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    let _ = e.handle_control(ControlEvent::LooperSetBars { bars: 1 });
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(16)); // to the boundary: start
    assert!(matches!(e.looper.state, LooperState::Recording));
    run(&mut e, steps(16) + steps(16)); // one full bar after start + margin
    assert!(matches!(e.looper.state, LooperState::Playing), "auto-closed after `bars` bars");
}

#[test]
fn tc3_early_stop_quantized() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    let _ = e.handle_control(ControlEvent::LooperSetBars { bars: 4 });
    run(&mut e, steps(2));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(14)); // boundary: recording
    run(&mut e, steps(24)); // 1.5 bars into the capture
    let _ = e.handle_control(ControlEvent::MasterRecord); // early stop request
    assert!(matches!(e.looper.state, LooperState::Recording), "not closed immediately");
    run(&mut e, steps(10)); // next boundary
    assert!(matches!(e.looper.state, LooperState::Playing), "closed at the boundary");
    // Досрочное закрытие ОБРЕЗАЕТ петлю до записанных целых тактов (2 из 4):
    // эталон — полная 4-тактная петля второго движка.
    let mut e2 = engine();
    let _ = e2.handle_control(ControlEvent::MasterPlay);
    let _ = e2.handle_control(ControlEvent::LooperSetBars { bars: 4 });
    run(&mut e2, steps(2));
    let _ = e2.handle_control(ControlEvent::MasterRecord);
    run(&mut e2, steps(14) + steps(16) * 4); // start + full 4 bars auto-close
    assert!(matches!(e2.looper.state, LooperState::Playing));
    assert_eq!(
        e.looper.loop_len_ticks() * 2,
        e2.looper.loop_len_ticks(),
        "early-closed loop must be exactly half the full-bars loop"
    );
}

#[test]
fn tc4_free_mode_immediate() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterRecord); // drums stopped
    assert!(e.drum_engine.sequencer.playing, "auto-play");
    assert!(matches!(e.looper.state, LooperState::Recording), "free mode: raw immediate capture");
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(matches!(e.looper.state, LooperState::Playing), "immediate finish");
}

#[test]
fn tc5_master_stop_clears_pending() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(4));
    let _ = e.handle_control(ControlEvent::MasterRecord); // armed
    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(!e.master_rec_pending_atom().load(Ordering::Relaxed));
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn tc8_rearm_cancel() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(6));
    let _ = e.handle_control(ControlEvent::MasterRecord); // armed
    let _ = e.handle_control(ControlEvent::MasterRecord); // cancel before boundary
    assert!(!e.master_rec_pending_atom().load(Ordering::Relaxed), "disarmed");
    run(&mut e, steps(10));
    assert!(matches!(e.looper.state, LooperState::Idle), "never started");
}
