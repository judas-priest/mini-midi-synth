//! SF2 hihat choke: закрытый/педальный хэт обрывает хвост открытого
//! (см. docs/superpowers/plans/2026-10-28-sf2-hihat-choke.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE};

#[test]
fn tc1_first_hat_nothing_to_choke() {
    let mut e = DrumEngine::new(48_000.0);
    assert_eq!(e.sf2_choke_take(46), 0);
}

#[test]
fn tc2_closed_hat_chokes_open() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(42), 46);
}

#[test]
fn tc3_pedal_hat_chokes_open() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(44), 46);
}

#[test]
fn tc4_retrigger_same_note_cuts_own_tail() {
    // rustysynth не убивает same-key войс на повторном note-on — явный note-off
    // предыдущего триггера предотвращает накопление войсов
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(46), 46);
}

#[test]
fn tc5_non_choke_notes_pass_through() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    assert_eq!(e.sf2_choke_take(38), 0); // снейр не трогает хэт-группу
    assert_eq!(e.sf2_choke_take(42), 46); // состояние группы сохранено
}

#[test]
fn tc6_reset_clears_state() {
    let mut e = DrumEngine::new(48_000.0);
    e.sf2_choke_take(46);
    e.reset_choke_state();
    assert_eq!(e.sf2_choke_take(42), 0);
}

#[test]
fn tc7_choke_uses_remapped_note() {
    // контракт caller'а: сначала remapped_note(), потом sf2_choke_take()
    let mut e = DrumEngine::new(48_000.0);
    e.remap[(40 - DRUM_NOTE_BASE) as usize] = 10; // pad note 40 -> slot 10 (note 46, Open HH)
    let n = e.remapped_note(40);
    e.sf2_choke_take(n);
    assert_eq!(e.sf2_choke_take(42), n);
}
