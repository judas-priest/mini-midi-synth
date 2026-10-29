//! Drum note remap: входная нота -> внутренний слот (см. docs/superpowers/plans/2026-10-28-drum-note-remap.md)
use mini_midi_synth::synth::drum::{DrumEngine, DRUM_NOTE_BASE};

#[test]
fn tc1_identity_by_default() {
    let eng = DrumEngine::new(48_000.0);
    assert_eq!(eng.remapped_note(40), 40);
}

#[test]
fn tc2_remap_translates_to_other_note() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[(40 - DRUM_NOTE_BASE) as usize] = 46; // pad note 40 -> kit note 46 (Open HH)
    assert_eq!(eng.remapped_note(40), 46);
}

#[test]
fn tc3_full_range() {
    let mut eng = DrumEngine::new(48_000.0);
    eng.remap[0] = 51; // note 36 -> kit note 51 (Closed Hat)
    assert_eq!(eng.remapped_note(36), 51);
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
    eng.apply_remap_vec(&[46, 51]);
    assert_eq!(eng.remapped_note(36), 46);
    assert_eq!(eng.remapped_note(37), 51);
    assert_eq!(eng.remapped_note(38), 38); // незаданные позиции = identity
    eng.apply_remap_vec(&[36u8; 20]); // длиннее 16 — берутся первые 16
    assert_eq!(eng.remapped_note(36), 36);
    eng.apply_remap_vec(&[200]); // значение вне 30..=55 клампится
    assert_eq!(eng.remapped_note(36), 55);
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
    eng.remap[(40 - DRUM_NOTE_BASE) as usize] = 46;
    let on = eng.remapped_note(40);
    eng.note_on(40, 100);
    eng.note_off(40);
    let off = eng.remapped_note(40);
    assert_eq!(on, off);
    assert_eq!(on, 46);
}

#[test]
fn identity_remap_is_full_notes_36_to_51() {
    // D1 contract: GUI identity = 36+i (full notes), not slots 0..15.
    let mut e = DrumEngine::new(48_000.0);
    let identity: Vec<u8> = (0..16).map(|i| 36 + i as u8).collect();
    e.apply_remap_vec(&identity);
    for i in 0..16u8 {
        assert_eq!(e.remapped_note(36 + i), 36 + i, "pad slot {i} not identity");
    }
}

#[test]
fn legacy_slot_encoding_migrates_to_notes() {
    let mut e = DrumEngine::new(48_000.0);
    let legacy: Vec<u8> = (0..16).collect();
    let migrated: Vec<u8> = legacy.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
    e.apply_remap_vec(&migrated);
    assert_eq!(e.remapped_note(36), 36);
    assert_eq!(e.remapped_note(51), 51);
}

#[test]
fn sample_rate_change_preserves_user_state() {
    let mut e = DrumEngine::new(48_000.0);
    let remap: Vec<u8> = (0..16).map(|i| (36 + i) as u8).collect();
    e.apply_remap_vec(&remap);
    e.apply_pad_vel_vec(&[100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 90]);
    let mut choke = [0u8; 26];
    choke[0] = 1;
    e.set_choke_groups(&choke);
    e.sf2_mode = true;
    e.fixed_velocity = true;
    e.volume = 0.42;
    e.set_sample_rate(44_100.0);
    assert_eq!(e.remapped_note(36), 36, "remap lost");
    assert_eq!(e.pad_velocity(36, 5), 100, "pad_vel lost");
    assert_eq!(e.choke_groups[0], 1, "choke table lost");
    assert!(e.sf2_mode, "sf2_mode lost");
    assert!(e.fixed_velocity, "fixed_velocity lost");
    assert_eq!(e.volume, 0.42, "volume lost");
}

#[test]
fn sample_rate_change_twice_still_preserves() {
    let mut e = DrumEngine::new(48_000.0);
    let remap: Vec<u8> = (0..16).map(|i| (40 + i) as u8).collect();
    e.apply_remap_vec(&remap);
    e.set_sample_rate(44_100.0);
    e.set_sample_rate(48_000.0);
    assert_eq!(e.remapped_note(36), 40, "remap lost on double SR change");
}

#[test]
fn undo_atom_tracks_snapshot_lifecycle() {
    let mut e = DrumEngine::new(48_000.0);
    assert!(!e.undo_available(), "no snapshot at start");
    e.sequencer.save_snapshot();
    assert!(e.undo_available());
    e.sequencer.undo();
    assert!(!e.undo_available(), "snapshot consumed by undo");
    e.sequencer.clear_pattern();
    assert!(e.undo_available());
}

#[test]
fn sample_rate_change_resets_choke_state() {
    let mut e = DrumEngine::new(48_000.0);
    let mut choke = [0u8; 26];
    choke[0] = 1;
    e.set_choke_groups(&choke);
    assert_eq!(e.sf2_choke_take(30), 0);
    e.set_sample_rate(44_100.0);
    let prev = e.sf2_choke_take(30);
    assert_eq!(prev, 0, "stale choke holder survived SR change");
}
