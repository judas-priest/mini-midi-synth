//! Drum note remap: входная нота -> внутренний слот (см. docs/superpowers/plans/2026-10-28-drum-note-remap.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE, NUM_DRUM_SLOTS};

#[test]
fn tc1_identity_by_default() {
    let eng = DrumEngine::new(48_000.0);
    assert_eq!(eng.remapped_note(40), 40);
}

#[test]
fn tc2_remap_translates_to_other_slot() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[(40 - DRUM_NOTE_BASE) as usize] = 0; // pad note 40 -> kick slot
    assert_eq!(eng.remapped_note(40), DRUM_NOTE_BASE); // 36
}

#[test]
fn tc3_full_range() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[0] = (NUM_DRUM_SLOTS - 1) as u8; // note 36 -> slot 15
    assert_eq!(eng.remapped_note(36), DRUM_NOTE_BASE + 15); // 51
}

#[test]
fn tc4_out_of_range_passthrough_no_panic() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[0] = 3;
    assert_eq!(eng.remapped_note(60), 60);
    assert_eq!(eng.remapped_note(35), 35);
    assert_eq!(eng.remapped_note(0), 0);
    assert_eq!(eng.remapped_note(52), 52);
}

#[test]
fn tc7_normalize_short_vec() {
    // нормализация вектора из конфига: короткий/длинный не паникует, короткий дополняется identity
    let mut eng = DrumEngine::new(48_000.0);
    eng.apply_remap_vec(&[7, 1]);
    assert_eq!(eng.remapped_note(36), 36 + 7);
    assert_eq!(eng.remapped_note(38), 38); // незаданные позиции = identity
    eng.apply_remap_vec(&[0u8; 20]); // длиннее 16 — берутся первые 16
    assert_eq!(eng.remapped_note(36), 36);
    eng.apply_remap_vec(&[200]); // значение > 15 клампится
    assert_eq!(eng.remapped_note(36), DRUM_NOTE_BASE + 15);
}

#[test]
fn tc6_config_without_remaps_is_default() {
    let s: mini_midi_synth::config::Sf2Settings = serde_json::from_str("{}").unwrap();
    assert!(s.drums_remaps.is_empty());
}

#[test]
fn tc5_translation_is_stateless_and_stable() {
    // переведённая нота для off обязана совпадать с on (иначе SF2-войс зависает);
    // note_on не должен мутировать таблицу
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[(40 - DRUM_NOTE_BASE) as usize] = 0;
    let on = eng.remapped_note(40);
    eng.note_on(40, 100);
    eng.note_off(40);
    let off = eng.remapped_note(40);
    assert_eq!(on, off);
    assert_eq!(on, DRUM_NOTE_BASE);
}
