//! Per-пэд velocity-оверрайд ударки (см. docs/superpowers/plans/2026-10-28-drum-pad-velocity.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE};

#[test]
fn tc1_default_passthrough() {
    let e = DrumEngine::new(48_000.0);
    assert_eq!(e.pad_velocity(40, 57), 57);
    assert_eq!(e.pad_velocity(51, 1), 1);
}

#[test]
fn tc2_override_applies() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    assert_eq!(e.pad_velocity(40, 30), 100);
}

#[test]
fn tc3_override_isolated_per_pad() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    assert_eq!(e.pad_velocity(41, 33), 33);
}

#[test]
fn tc4_zero_returns_to_passthrough() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 0;
    assert_eq!(e.pad_velocity(40, 30), 30);
}

#[test]
fn tc5_normalize_from_config() {
    let mut e = DrumEngine::new(48_000.0);
    e.apply_pad_vel_vec(&[100, 127]);
    assert_eq!(e.pad_velocity(36, 10), 100);
    assert_eq!(e.pad_velocity(37, 10), 127);
    assert_eq!(e.pad_velocity(38, 10), 10);  // незаданные = Off
    e.apply_pad_vel_vec(&[5u8; 20]);          // длиннее 16 — берутся первые 16
    assert_eq!(e.pad_velocity(36, 10), 5);
    e.apply_pad_vel_vec(&[250]);              // >127 клампится
    assert_eq!(e.pad_velocity(36, 10), 127);
}

#[test]
fn tc6_independent_of_fixed_velocity_flag() {
    // оверрайд сам по себе; caller применяет его ПОСЛЕ fixed_velocity
    let mut e = DrumEngine::new(48_000.0);
    e.fixed_velocity = true;
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 64;
    assert_eq!(e.pad_velocity(40, 30), 64);
}

#[test]
fn tc7_out_of_range_passthrough() {
    let mut e = DrumEngine::new(48_000.0);
    e.pad_vel[(40 - DRUM_NOTE_BASE) as usize] = 100;
    assert_eq!(e.pad_velocity(60, 40), 40);
    assert_eq!(e.pad_velocity(35, 40), 40);
    assert_eq!(e.pad_velocity(0, 40), 40);
}
