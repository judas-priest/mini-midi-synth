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

/// MasterStop ПОСЛЕ закрытия capture не должен трогать готовый луп:
/// looper_capture_quantized остаётся true (гейтит стирание свежего захвата),
/// но looper_rec_cycles == 0 — без guard'а trim_after(0) стирает все события
/// (regression: review fix trim guard).
#[test]
fn tc13_stop_after_close_keeps_loop() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16)); // граница: Playing
    assert!(matches!(e.looper.state, LooperState::Playing));
    let events_before = e.looper.len();
    assert!(events_before > 0);

    let _ = e.handle_control(ControlEvent::MasterStop);
    assert!(e.looper.len() == events_before, "loop events intact after stop");
    assert!(
        (e.looper.loop_len_ticks() as i64 - 96000).abs() <= 64,
        "loop length unchanged: {}", e.looper.loop_len_ticks()
    );
}

/// MasterStop в середине ЗАХВАТА (не закрытия) обрезает луп до записанных
/// циклов — audit rhythm #2.
#[test]
fn tc14_stop_mid_capture_trims() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват, cycle 1
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    run(&mut e, steps(16)); // граница: cycle 2 (looper_rec_cycles = 2)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 67 });
    let _ = e.handle_control(ControlEvent::MasterStop); // посреди захвата
    // master_stop останавливает всё: Recording -> Playing -> Idle
    assert!(matches!(e.looper.state, LooperState::Idle));
    // Цикл ~96000 тиков: петля = 2 цикла, cycle-2 нота жива, cycle-3 тишины нет
    assert!(
        (e.looper.loop_len_ticks() as i64 - 2 * 96000).abs() <= 64,
        "loop trimmed to 2 cycles: got {}", e.looper.loop_len_ticks()
    );
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 60), "cycle-1 note kept");
        assert!(ons.iter().any(|ev| ev.note == 67), "cycle-2 note kept");
    }
}

/// Clock-death fallback: часы умерли посреди quantized capture (sequencer
/// остановлен напрямую, field poke — сценарий не driven атомами) → fallback
/// в tick_block закрывает capture СЕЙЧАС: атом гасится, лупер уходит в
/// Playing, записанное живо. Длину петли fallback НЕ перекраивает
/// (нечётный кусок) — фиксируем как есть.
#[test]
fn tc15_clock_death_fallback_closes_capture() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват, cycle 1
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 2, "capturing");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    // Клок умирает посреди записи
    e.drum_engine.sequencer.playing = false;
    run(&mut e, 2); // >= 1 блок: fallback закрывает capture
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 0, "rec atom reset");
    assert!(matches!(e.looper.state, LooperState::Playing), "closed by fallback");
    // Петля не перекроена: осталась ёмкостью 64 цикла (безостановочный захват)
    assert!(
        (e.looper.loop_len_ticks() as i64 - 64 * 96000).abs() <= 64,
        "fallback keeps loop length (no reshape): got {}", e.looper.loop_len_ticks()
    );
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 60), "captured note kept");
    }
}

/// Негатив: смерть клока при НЕ-capturing — no-op (fallback-арм не должен
/// трогать Idle-лупер и атомы).
#[test]
fn tc16_clock_death_no_capture_noop() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    e.drum_engine.sequencer.playing = false;
    run(&mut e, 4);
    assert_eq!(e.master_rec_state_atom().load(Ordering::Relaxed), 0, "atom untouched");
    assert!(matches!(e.looper.state, LooperState::Idle), "no capture started");
    assert_eq!(e.looper.len(), 0, "no events recorded");
}

/// CaptureCycleAdd (плюс-такт) во время quantized capture: логическая длина
/// растёт, закрытие отдаёт петлю = rec_cycles циклов (здесь 3 цикла:
/// 2 записанных и 1 добавленный с тишиной). Хендлер живёт в handle_control
/// ВНЕ машины — тест фиксирует семантику rec_cycles и охраняет вырезку
/// косвенно: машина читает looper_rec_cycles на границе закрытия.
#[test]
fn tc17_cycle_add_extends_loop() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват, cycle 1 (rec_cycles = 1)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    run(&mut e, steps(16)); // граница: cycle 2 (rec_cycles = 2)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 67 });
    let _ = e.handle_control(ControlEvent::CaptureCycleAdd); // +1: rec_cycles = 3
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16)); // граница: закрылось, петля = 3 цикла
    assert!(matches!(e.looper.state, LooperState::Playing));
    assert!(
        (e.looper.loop_len_ticks() as i64 - 3 * 96000).abs() <= 64,
        "loop = 3 cycles (2 recorded + 1 added): got {}", e.looper.loop_len_ticks()
    );
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 60 && ev.position < 0.2), "cycle-1 note in first third");
        assert!(
            ons.iter().any(|ev| ev.note == 67 && ev.position > 0.25 && ev.position < 0.45),
            "cycle-2 note in second third"
        );
        assert!(!ons.iter().any(|ev| ev.position > 0.6), "added cycle 3 is silence");
    }
}

/// LooperSetSource в процессе capture: в Part-режиме (mix=false) пишется
/// ТОЛЬКО активная часть (default 0); возврат в Mix снова пишет все части.
/// Behavior lock по display()-контенту.
#[test]
fn tc18_source_part_gates_capture() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват
    // Part source: part 1 (не активная) не пишется
    let _ = e.handle_control(ControlEvent::LooperSetSource { mix: false });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 40, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 40 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    run(&mut e, 94);
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 40), "active part (0) recorded");
        assert!(!ons.iter().any(|ev| ev.note == 72), "inactive part (1) gated in Part source");
    }
    // Mix source: part 1 снова пишется
    let _ = e.handle_control(ControlEvent::LooperSetSource { mix: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    run(&mut e, 94);
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16)); // граница: Playing
    assert!(matches!(e.looper.state, LooperState::Playing));
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 72), "part 1 recorded after Mix toggle");
        assert!(ons.iter().any(|ev| ev.note == 40), "part 0 still there");
    }
}

#[test]
fn tc11_overdub_layer_boundary_aligned() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // старт
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    // Закрыть в ПЕРВОМ цикле: Rec до его границы → петля = 1 цикл
    run(&mut e, steps(10));
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(6)); // граница: Playing, петля = 1 цикл
    let len_before = e.looper.len();
    assert!(matches!(e.looper.state, LooperState::Playing));

    // Слой: Ovr → на границе Overdubbing
    let _ = e.handle_control(ControlEvent::MasterOverdub);
    assert!(matches!(e.looper.state, LooperState::Playing), "waits for boundary");
    run(&mut e, steps(16)); // граница: overdub started
    assert!(matches!(e.looper.state, LooperState::Overdubbing));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 65, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 65 });
    let _ = e.handle_control(ControlEvent::MasterOverdub); // закрыть слой
    run(&mut e, steps(16)); // граница: Playing
    assert!(matches!(e.looper.state, LooperState::Playing));
    // Длина петли не изменилась; слой записан
    assert!(
        (e.looper.loop_len_ticks() as i64 - 96000).abs() <= 64,
        "loop length unchanged: {}", e.looper.loop_len_ticks()
    );
    assert!(e.looper.len() > len_before, "layer captured");
}

#[test]
fn tc12_cycle_remove_cuts_last_cycle() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват, cycle 1
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    run(&mut e, steps(16)); // граница: cycle 2
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 67, velocity: 100 }); // плохая нота в cycle 2
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 67 });
    let _ = e.handle_control(ControlEvent::CaptureCycleRemove); // −1: убрать cycle 2
    run(&mut e, steps(8)); // доигрываем до границы (уже в урезанном cycle 1→2)
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Playing));
    {
        let disp = e.looper.display();
        let disp = disp.lock().unwrap();
        let ons: Vec<_> = disp.events.iter().filter(|ev| ev.velocity > 0).collect();
        assert!(ons.iter().any(|ev| ev.note == 60), "cycle-1 note kept");
        assert!(!ons.iter().any(|ev| ev.note == 67), "cycle-2 note removed");
    }
}
