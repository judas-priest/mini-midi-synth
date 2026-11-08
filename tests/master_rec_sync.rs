//! Quantized master recording: looper starts at the pattern boundary,
//! петля = РОВНО 1 цикл паттерна, закрывается сама на его конце (#30 sync).

use mini_midi_synth::synth::looper::LooperState;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine};
use std::sync::atomic::Ordering;

// Sustain-capture characterization (plan fix/looper-sustain-capture, Task 1).
//
// Цель: пока педаль (CC64/SetSustain) ВНИЗУ, note-off ОТКЛАДЫВАЕТСЯ — в лупер
// он пишется в момент pedal-UP, а не отпускания клавиши. Сейчас отложенной
// записи нет: off пишется сразу при key-release — таргет-тесты (TC1/5/6/7/8/9)
// RED до Task 2, регрессионные замки (TC2/3/4/10) GREEN.
//
// Все тесты атомарные (handle_control + tick_block), как tc13/tc14.
// Позиции в display() нормированы на ДЛИНУ закрытой петли: во время
// безостановочного захвата тики непрерывны (mod.rs:2246), поэтому assert'ы
// по позициям делаются после закрытия петли (Rec -> граница -> Playing).

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

// ---------------------------------------------------------------------------
// Sustain capture (Task 1, TC1-TC10)
// ---------------------------------------------------------------------------

/// (position, layer_id) of every recorded event for `note`;
/// `off = true` -> note-offs (velocity 0), `off = false` -> note-ons.
fn note_events(e: &SynthEngine, note: u8, off: bool) -> Vec<(f32, u8)> {
    let disp = e.looper.display();
    let disp = disp.lock().unwrap();
    disp.events
        .iter()
        .filter(|ev| {
            ev.note == note && if off { ev.velocity == 0 } else { ev.velocity > 0 }
        })
        .map(|ev| (ev.position, ev.layer_id))
        .collect()
}

/// Close a capturing loop the tc13 way: Rec now, boundary next -> Playing,
/// петля = ровно 1 цикл (96000 тиков), позиции display() нормированы.
fn close_one_cycle(e: &mut SynthEngine) {
    let _ = e.handle_control(ControlEvent::MasterRecord); // закрыть
    run(e, steps(16)); // граница: Playing
    assert!(matches!(e.looper.state, LooperState::Playing), "loop closed");
    assert!(
        (e.looper.loop_len_ticks() as i64 - 96000).abs() <= 64,
        "loop = 1 cycle: got {}", e.looper.loop_len_ticks()
    );
}

/// Start a quantized capture at the pattern boundary (tc13 pattern).
fn capture_at_boundary(e: &mut SynthEngine) {
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(e, steps(13)); // граница: захват, playback_tick ~0 (±блок)
}

/// TC1 (sustain_captured): педаль внизу, клавиша отпущена под педалью —
/// TARGET: off пишется в момент pedal-UP (~позиция 0.25), а НЕ при
/// key-release (~0.0625). RED до Task 2 (отложенной записи нет — off
/// записывается сразу при key-release).
#[test]
fn sustain_captured() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1)); // key-release ~тик 6000 (позиция ~0.0625)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // клавиша вверх
    run(&mut e, steps(3)); // pedal-UP ~тик 24000 (позиция ~0.25)
    let _ = e.handle_control(ControlEvent::SetSustain { on: false }); // TARGET: off здесь
    close_one_cycle(&mut e);
    let offs = note_events(&e, 60, true);
    assert_eq!(offs.len(), 1, "ровно один off для ноты 60");
    assert!(
        offs[0].0 > 0.2 && offs[0].0 < 0.31,
        "off должен сидеть в pedal-up (~0.25), а не в key-release (~0.0625): {}",
        offs[0].0
    );
    let ons = note_events(&e, 60, false);
    assert!(!ons.is_empty() && ons[0].0 < 0.1, "on у старта захвата");
}

/// TC2 (регрессионный замок, GREEN и до Task 2): педаль отпущена ДО
/// key-release — off пишется немедленно в момент note-off (~0.125),
/// никаких артефактов отложенной записи.
#[test]
fn sustain_off_before_key_release_immediate() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1)); // ~тик 6000
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: false }); // педаль вверх, клавиши не трогали
    run(&mut e, steps(1)); // key-release ~тик 12000 (позиция ~0.125)
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    close_one_cycle(&mut e);
    let offs = note_events(&e, 62, true);
    assert_eq!(offs.len(), 1, "ровно один off");
    assert!(
        offs[0].0 > 0.08 && offs[0].0 < 0.17,
        "off при key-release (~0.125): {}", offs[0].0
    );
}

/// TC3 (GREEN сейчас; flush-vs-clear при стопе — дизайн Task 2): педаль
/// внизу с ожидающим off, MasterStop посреди захвата — без паники, петля
/// вменяемая (обрезана до записанных циклов, контент жив).
#[test]
fn sustain_pending_master_stop_sane() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pending в TARGET
    let _ = e.handle_control(ControlEvent::MasterStop); // посреди захвата
    assert!(matches!(e.looper.state, LooperState::Idle), "стоп = Idle");
    assert!(
        (e.looper.loop_len_ticks() as i64 - 96000).abs() <= 64,
        "петля обрезана до 1 цикла: {}", e.looper.loop_len_ticks()
    );
    let ons = note_events(&e, 60, false);
    assert!(!ons.is_empty(), "нота цикла 1 сохранена");
    // Task 2 strengthening: MasterStop ФЛУШИТ отложенный off в петлю
    // (до trim_after) — off в display после стопа, на момент стопа
    // (~0.0625), а не потерян и не в pedal-up.
    let offs = note_events(&e, 60, true);
    assert_eq!(offs.len(), 1, "pending off флушится в петлю при MasterStop: {:?}", offs);
    assert!(
        offs[0].0 < 0.2,
        "off в момент стопа (~0.0625), не потерян: {}", offs[0].0
    );
}

/// TC4 (регрессионный замок, GREEN): pedal up/down БЕЗ записи — set_sustain
/// ведёт себя как раньше: без паники, лупер не тронут.
#[test]
fn sustain_not_recording_untouched() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 55, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 55 });
    run(&mut e, steps(2));
    let _ = e.handle_control(ControlEvent::SetSustain { on: false });
    run(&mut e, steps(2));
    assert!(matches!(e.looper.state, LooperState::Idle));
    assert_eq!(e.looper.len(), 0, "без захвата лупер пуст");
}

/// TC5: аккорд (3 ноты) под педалью — TARGET: все offs отложены к pedal-UP
/// (~0.25). RED до Task 2 (сейчас все offs при key-release ~0.0625).
#[test]
fn sustain_chord_all_deferred() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1)); // ~тик 6000
    for n in [60u8, 64, 67] {
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: n, velocity: 100 });
    }
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    for n in [60u8, 64, 67] {
        e.handle_event(MidiEvent::NoteOff { channel: 0, note: n }); // key-release
    }
    run(&mut e, steps(3)); // pedal-UP ~тик 24000 (позиция ~0.25)
    let _ = e.handle_control(ControlEvent::SetSustain { on: false });
    close_one_cycle(&mut e);
    for n in [60u8, 64, 67] {
        let offs = note_events(&e, n, true);
        assert_eq!(offs.len(), 1, "один off для {n}");
        assert!(
            offs[0].0 > 0.2 && offs[0].0 < 0.31,
            "off {n} в pedal-up (~0.25): {}", offs[0].0
        );
    }
}

/// TC6: рестирай той же ноты под педалью при уже отложенном off — TARGET:
/// flush пишет off РОВНО ОДИН раз (второй отсекается guard'ом active_notes),
/// позиция = pedal-UP. RED до Task 2 (сейчас пишутся оба off при key-release).
#[test]
fn sustain_restrike_duplicate_pending_one_flush() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1)); // ~тик 6000
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pending #1
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }); // рестирай
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pending #2 (дубль)
    run(&mut e, steps(3)); // pedal-UP ~тик 24000
    let _ = e.handle_control(ControlEvent::SetSustain { on: false }); // flush: один off
    close_one_cycle(&mut e);
    let offs = note_events(&e, 60, true);
    assert_eq!(offs.len(), 1, "flush пишет off один раз: {:?}", offs);
    assert!(
        offs[0].0 > 0.2 && offs[0].0 < 0.31,
        "единственный off в pedal-up (~0.25): {}", offs[0].0
    );
}

/// TC7: педаль держится ЧЕРЕЗ wrap замкнутой петли (overdub-слой): на wrap
/// forced-off чистит active_notes, поздний flush отсекается guard'ом —
/// TARGET: off в лупе НЕ появляется вовсе (звук слышен до wrap, обрезка —
/// playback-side). RED до Task 2 (сейчас off пишется при key-release).
/// Wrap с forced-off бывает только на замкнутой петле (Во время захвата тики
/// непрерывны, mod.rs:2246), поэтому сценарий строится на overdub-слое.
#[test]
fn sustain_pending_across_wrap_guard_dropped() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    close_one_cycle(&mut e); // базовая петля = 1 цикл
    // Overdub-слой: Ovr -> граница -> Overdubbing
    let _ = e.handle_control(ControlEvent::MasterOverdub);
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Overdubbing));
    run(&mut e, steps(1)); // ~тик 6000
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 70, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 70 }); // pending
    run(&mut e, steps(15)); // через wrap (тик 96000): forced-off чистит active
    let _ = e.handle_control(ControlEvent::SetSustain { on: false }); // поздний flush — мимо
    let _ = e.handle_control(ControlEvent::MasterOverdub); // закрыть слой
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Playing), "слой закрыт");
    assert_eq!(note_events(&e, 70, true).len(), 0, "off через wrap не пишется");
    assert!(!note_events(&e, 70, false).is_empty(), "on слоя записан");
}

/// TC8: отложенная запись работает и в Overdubbing — TARGET: off слоя в
/// pedal-UP (on + 2 шага), а не в key-release. RED до Task 2.
#[test]
fn sustain_overdub_deferred() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    close_one_cycle(&mut e);
    let _ = e.handle_control(ControlEvent::MasterOverdub);
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Overdubbing));
    run(&mut e, steps(1)); // key-release ~тик 6000 (позиция ~0.0625)
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 70, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 70 });
    run(&mut e, steps(2)); // pedal-UP = key-release + 2 шага
    let _ = e.handle_control(ControlEvent::SetSustain { on: false });
    let _ = e.handle_control(ControlEvent::MasterOverdub);
    run(&mut e, steps(16));
    assert!(matches!(e.looper.state, LooperState::Playing));
    let offs = note_events(&e, 70, true);
    assert_eq!(offs.len(), 1, "один off слоя");
    // Оверdub-граница пересекается СЕРЕДИНЕ run(steps(16)): key-release
    // реально ~0.13, pedal-up = +2 шага (~0.25). Ось — позиция ON-события.
    let ons = note_events(&e, 70, false);
    assert_eq!(ons.len(), 1, "один on слоя");
    assert!(
        offs[0].0 > ons[0].0 + 0.10 && offs[0].0 < ons[0].0 + 0.16,
        "off слоя в pedal-up (on + 2 шага = +0.125): off {} on {}",
        offs[0].0, ons[0].0
    );
}

/// TC9: AllNotesOff (CC123) при захвате с ожидающим off — TARGET:
/// set_sustain(0) сначала flush'ит отложенный off (позиция ~0.1875 = момент
/// AllNotesOff), потом глушит. RED до Task 2 (off сейчас в key-release ~0.0625).
#[test]
fn sustain_all_notes_off_flushes_pending() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    run(&mut e, steps(1)); // ~тик 6000
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pending
    run(&mut e, steps(2)); // AllNotesOff ~тик 18000 (позиция ~0.1875)
    let _ = e.handle_control(ControlEvent::AllNotesOff); // set_sustain(0) -> flush
    close_one_cycle(&mut e);
    let offs = note_events(&e, 60, true);
    assert_eq!(offs.len(), 1, "один off для ноты 60");
    assert!(
        offs[0].0 > 0.14 && offs[0].0 < 0.24,
        "off должен попасть в момент AllNotesOff (~0.1875): {}", offs[0].0
    );
}

/// TC10 (регрессионный замок, GREEN; лок для Task 2): парт с
/// sustain_enabled=false — его off пишется НЕМЕДЛЕННО при key-release (~0.0625),
/// отложение педали его не касается.
#[test]
fn sustain_part_disabled_records_immediately() {
    let mut e = engine();
    capture_at_boundary(&mut e);
    let _ = e.handle_control(ControlEvent::SetPartSustain { part: 0, on: false });
    run(&mut e, steps(1)); // ~тик 6000
    let _ = e.handle_control(ControlEvent::SetSustain { on: true }); // педаль глобально внизу
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 63, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 63 }); // key-release: сразу
    run(&mut e, steps(3)); // pedal-UP ~тик 24000
    let _ = e.handle_control(ControlEvent::SetSustain { on: false });
    close_one_cycle(&mut e);
    let offs = note_events(&e, 63, true);
    assert_eq!(offs.len(), 1, "один off");
    assert!(
        offs[0].0 > 0.03 && offs[0].0 < 0.12,
        "парт без sustain: off при key-release (~0.0625), НЕ в pedal-up: {}", offs[0].0
    );
}

/// Слоёный сетап (регрессия на review b510437): пары 0 и 2 включены на одном
/// диапазоне — note-on записывается в ОБА парта, active_notes[60] хранит
/// ПОСЛЕДНИЙ (part 2). Отложенный off должен сесть в петлю с part_id
/// держателя (2), иначе flush-guard его отбросит и нота зависнет на каждом
/// проходе петли.
#[test]
fn sustain_layered_holder_off_recorded() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 2, enabled: true });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват
    run(&mut e, steps(1)); // ~тик 6000
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }); // пары 0+2, holder = 2
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pending
    run(&mut e, steps(3)); // pedal-UP ~тик 24000
    let _ = e.handle_control(ControlEvent::SetSustain { on: false }); // flush
    close_one_cycle(&mut e);
    let disp = e.looper.display();
    let disp = disp.lock().unwrap();
    let offs: Vec<_> = disp.events.iter()
        .filter(|ev| ev.note == 60 && ev.velocity == 0)
        .collect();
    assert_eq!(offs.len(), 1, "ровно один off в петле: {:?}", offs.iter().map(|ev| (ev.part_id, ev.position)).collect::<Vec<_>>());
    assert_eq!(
        offs[0].part_id, 2,
        "off принадлежит держателю ноты (последний прозвучавший парт)"
    );
}

/// Сплит (регрессия на final review a11582f): part0 = 0..60, part1 = 61..127.
/// Нота 60 звучит ТОЛЬКО в part0 (part1 вне диапазона, route_note_on не
/// прозвучал — active_notes[60] = 0). Конфиг-гейты (enabled/sustain_enabled
/// part1) пропускали запись part1 → overwrote → flush-guard отбрасывал off →
/// зависшая нота. Гейт должен быть по ДЕРЖАТЕЛЮ.
#[test]
fn sustain_split_holder_off_recorded() {
    let mut e = engine();
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 60 });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 61, max_note: 127 });
    let _ = e.handle_control(ControlEvent::MasterPlay);
    run(&mut e, steps(3));
    let _ = e.handle_control(ControlEvent::MasterRecord); // arm
    run(&mut e, steps(13)); // граница: захват
    run(&mut e, steps(1)); // ~тик 6000
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 }); // holder = part 0
    let _ = e.handle_control(ControlEvent::SetSustain { on: true });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 }); // pending
    run(&mut e, steps(3)); // pedal-UP ~тик 24000
    let _ = e.handle_control(ControlEvent::SetSustain { on: false }); // flush
    close_one_cycle(&mut e);
    let disp = e.looper.display();
    let disp = disp.lock().unwrap();
    let offs: Vec<_> = disp.events.iter()
        .filter(|ev| ev.note == 60 && ev.velocity == 0)
        .collect();
    assert_eq!(offs.len(), 1, "ровно один off в петле: {:?}", offs.iter().map(|ev| (ev.part_id, ev.position)).collect::<Vec<_>>());
    assert_eq!(
        offs[0].part_id, 0,
        "off принадлежит держателю ноты (единственный парт в диапазоне)"
    );
}
