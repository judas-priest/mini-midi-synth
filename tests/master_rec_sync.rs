//! Quantized master recording: looper starts at the pattern boundary,
//! петля = РОВНО 1 цикл паттерна, закрывается сама на его конце (#30 sync).

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
    assert!(
        matches!(e.looper.state, LooperState::Recording | LooperState::Overdubbing),
        "capture started at boundary"
    );
}

#[test]
fn tc2_auto_close_after_one_cycle() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(16)); // to the boundary: start
    assert!(matches!(
        e.looper.state,
        LooperState::Recording | LooperState::Overdubbing
    ));
    run(&mut e, steps(16) + steps(16)); // one full pattern cycle + margin
    assert!(matches!(e.looper.state, LooperState::Playing), "auto-closed after one pattern cycle");
}

#[test]
fn tc3_loop_is_one_pattern_cycle() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(2));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(14)); // boundary: capture starts
    run(&mut e, steps(16) * 2); // full cycle + margin -> auto-closed
    assert!(matches!(e.looper.state, LooperState::Playing), "closed after one cycle");
    // Петля = ровно 1 цикл паттерна: 16 шагов * 6000 сэмплов/шаг.
    // (лупер тикает раньше детекта границы — допуск на 1 блок)
    let expected = 16 * 6000;
    let got = e.looper.loop_len_ticks();
    assert!(
        got >= expected - 64 && got <= expected + 64,
        "loop must equal one pattern cycle: got {got}, want {expected}±64"
    );
    // Rec после закрытия = overdub поверх петли (Boss-семантика)
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(matches!(e.looper.state, LooperState::Overdubbing), "post-close Rec overdubs");
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
