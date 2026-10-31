//! Quantized master recording: looper starts at the pattern boundary,
//! петля = РОВНО 1 цикл паттерна, закрывается сама на его конце (#30 sync).

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
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
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 1, "armed");
    assert!(matches!(e.looper.state, LooperState::Idle), "not capturing until boundary");
    run(&mut e, steps(12)); // crosses the boundary
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 2, "capturing");
    assert_eq!(e.master_rec_cycle_atom().load(Ordering::Relaxed), 1);
    assert!(
        matches!(e.looper.state, LooperState::Recording | LooperState::Overdubbing),
        "capture started at boundary"
    );
}

#[test]
fn tc2_multi_cycle_capture() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // до границы: старт (играется цикл 1)
    assert!(matches!(e.looper.state, LooperState::Recording));
    // цикл 1: нота 60 (part 0)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    run(&mut e, steps(16)); // граница цикла 1 — не закрывает (pending ещё нет)
    let _ = e.handle_control(ControlEvent::MasterRecord); // попросили закрыть
    // цикл 2 (закрывающий): нота 67 в его начале
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 67 });
    run(&mut e, steps(16)); // граница: закрылось
    assert!(matches!(e.looper.state, LooperState::Playing), "closed at the boundary");
    // Петля = ровно 2 цикла
    assert!(
        (e.looper.loop_len_ticks() as i64 - 2 * 96000).abs() <= 64,
        "loop = 2 cycles: got {}", e.looper.loop_len_ticks()
    );
    assert!(e.looper.len() >= 4, "notes from both cycles captured");
    // TC-B: нота цикла 2 живёт во второй половине петли
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let second_half = disp.events.iter()
            .filter(|ev| ev.velocity > 0 && ev.position > 0.4)
            .count();
        assert!(second_half >= 1, "cycle-2 note lives in the second half");
        let _guard = disp; // держим guard до конца блока
    }
}

#[test]
fn tc3_no_autoclose_without_second_press() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord);
    run(&mut e, steps(13)); // старт
    run(&mut e, steps(16) * 3); // 3 цикла без нажатий
    assert!(
        matches!(e.looper.state, LooperState::Recording | LooperState::Overdubbing),
        "no second press = still capturing"
    );
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16)); // закрывающая граница
    assert!(matches!(e.looper.state, LooperState::Playing));
    // 4 цикла сыграно (стартовый + 3): total = cycles+1
    assert!(
        (e.looper.loop_len_ticks() as i64 - 4 * 96000).abs() <= 64,
        "4 cycles captured: got {}", e.looper.loop_len_ticks()
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
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 0);
    assert!(matches!(e.looper.state, LooperState::Idle));
}

#[test]
fn tc8_rearm_cancel() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(6));
    let _ = e.handle_control(ControlEvent::MasterRecord); // armed
    let _ = e.handle_control(ControlEvent::MasterRecord); // cancel before boundary
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 0, "disarmed");
    run(&mut e, steps(10));
    assert!(matches!(e.looper.state, LooperState::Idle), "never started");
}

#[test]
fn tc9_multitrack_records_only_sounding_parts() {
    let mut e = engine();
    // Сплит: part 0 = 0..59 (transpose +12), part 1 = 60..127
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват идёт
    // Ноты с обеих рук
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 }); // part 0
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 }); // part 1
    run(&mut e, 94);
    for n in [40u8, 72u8] {
        e.handle_event(MidiEvent::NoteOff { channel: 0, note: n });
    }
    run(&mut e, 94);
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 40 && ev.part_id == 0), "40 -> part 0");
        assert!(ons.iter().any(|ev| ev.note == 72 && ev.part_id == 1), "72 -> part 1");
        assert!(!ons.iter().any(|ev| ev.note == 40 && ev.part_id == 1), "40 must NOT tag part 1");
        let offs = disp.events.iter().filter(|ev| ev.velocity == 0).count();
        assert!(offs <= 4, "no 15-per-note garbage offs: got {offs}");
    }
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Playing));
}

#[test]
fn tc10_rec_after_close_starts_fresh() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // старт
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    run(&mut e, steps(16)); // цикл 1 завершён
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16)); // граница: Playing, петля = 1 цикл
    assert!(matches!(e.looper.state, LooperState::Playing));
    let old_len = e.looper.len();
    assert!(old_len > 0);

    // Rec после закрытия = НОВАЯ запись: петля стёрта, свежий арм
    let _ = e.handle_control(ControlEvent::MasterRecord);
    assert!(matches!(e.looper.state, LooperState::Idle), "old loop cleared");
    assert_eq!(e.looper.len(), 0, "old events wiped");
    run(&mut e, steps(16)); // граница: свежий захват
    assert!(matches!(e.looper.state, LooperState::Recording));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть свежий
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Playing));
    // В петле только новая нота — старой 60 нет
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 72), "fresh capture recorded");
        assert!(!ons.iter().any(|ev| ev.note == 60), "old loop content must be gone");
    }
}
