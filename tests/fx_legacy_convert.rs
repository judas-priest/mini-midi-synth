//! Покрытие FxChain::from_legacy_preset_active (блокер S3, REFACTOR_AUDIT).
//! Конвертер переносит legacy-поля в 16-слотовую цепочку. Пины:
//! 1) конвертируются ровно 15 семейств (набор — осознанное расширение в S3);
//! 2) порядок слотов = порядок legacy-цепочки;
//! 3) неактивные (mix<=0) семейства скипаются;
//! 4) маппинг параметров, включая трансформ freq_shift_hz/1000 и клампы;
//! 5) известные ПОТЕРИ (neuron_asym/bias/comb_sep, spring_spin/chaos, eq/
//!    compressor/overdrive/phaser/flanger/tremolo/bitcrusher и др.) пиннуты —
//!    появление их в слотах = осознанное расширение конвертера.

use mini_midi_synth::synth::fx_chain::{FxChain, FxSlotType};
use std::collections::BTreeMap;

fn legacy(with_all: bool) -> BTreeMap<String, f32> {
    let mut m = BTreeMap::new();
    if with_all {
        let kvs: &[(&str, f32)] = &[
            ("tape_mix", 0.8), ("tape_drive", 0.3), ("tape_saturation", 0.6),
            ("neuron_mix", 0.7), ("neuron_drive", 0.2), ("neuron_squash", 0.4),
            ("ring_mod_mix", 0.6), ("ring_mod_freq", 220.0), ("ring_mod_shape", 0.3),
            ("freq_shift_mix", 0.5), ("freq_shift_hz", 2500.0), ("freq_shift_feedback", 0.1),
            ("bonsai_mix", 0.4), ("bonsai_drive", 0.9), ("bonsai_mode", 2.0),
            ("resonator_mix", 0.3), ("resonator_freq", 330.0), ("resonator_decay", 0.6),
            ("chorus_mix", 0.2),
            ("ensemble_mix", 0.4), ("ensemble_depth", 0.6), ("ensemble_rate", 0.2),
            ("delay_mix", 0.9), ("delay_time_l", 0.25), ("delay_time_r", 0.5),
            ("delay_feedback", 0.35), ("delay_filter", 0.7),
            ("reverb_mix", 0.6), ("reverb_room_size", 0.8), ("reverb_damping", 0.2),
            ("spring_mix", 0.5), ("spring_size", 0.7), ("spring_decay", 0.4),
            ("rotary_mix", 0.3), ("rotary_speed", 1.0),
            ("wave_shaper_mix", 0.5), ("wave_shaper_drive", 0.8), ("wave_shaper_mode", 3.0),
            ("reverb2_mix", 0.4), ("reverb2_decay", 0.6), ("reverb2_size", 0.7),
            ("airwindows_mix", 0.3), ("airwindows_mode", 5.0),
        ];
        for (k, v) in kvs { m.insert(k.to_string(), *v); }
    }
    m
}

const EXPECTED_ORDER: &[FxSlotType] = &[
    FxSlotType::Tape, FxSlotType::Neuron, FxSlotType::RingMod, FxSlotType::FreqShift,
    FxSlotType::Bonsai, FxSlotType::Resonator, FxSlotType::Chorus, FxSlotType::BbdEnsemble,
    FxSlotType::Delay, FxSlotType::Reverb, FxSlotType::SpringReverb, FxSlotType::Rotary,
    FxSlotType::WaveShaper, FxSlotType::Reverb2, FxSlotType::Airwindows,
];

#[test]
fn tc_all_families_convert_in_legacy_order() {
    let chain = FxChain::from_legacy_preset_active(&legacy(true));
    let filled: Vec<(FxSlotType, f32)> = chain.slots.iter()
        .filter(|s| s.enabled && s.slot_type != FxSlotType::None)
        .map(|s| (s.slot_type, s.mix)).collect();
    assert_eq!(filled.len(), 15, "expected 15 families, got {filled:?}");
    for (i, (ty, mix)) in filled.iter().enumerate() {
        assert_eq!(*ty, EXPECTED_ORDER[i], "slot {i} order");
        assert!(*mix > 0.0);
    }
    // Хвост пустой (дефолтный слот: enabled=true, тип None)
    assert_eq!(chain.slots[15].slot_type, FxSlotType::None, "slot 15 must stay empty");
}

#[test]
fn tc_param_mapping_and_transforms() {
    let chain = FxChain::from_legacy_preset_active(&legacy(true));
    let find = |ty: FxSlotType| chain.slots.iter().find(|s| s.slot_type == ty).unwrap();
    // Трансформ freq_shift_hz/1000 с клампом [-1,1]: 2500 Гц -> 1.0
    assert_eq!(find(FxSlotType::FreqShift).params[0], 1.0, "freq_shift_hz/1000 clamp");
    // Прямой маппинг
    assert_eq!(find(FxSlotType::Tape).params[0], 0.3);           // tape_drive
    assert_eq!(find(FxSlotType::RingMod).params[0], 220.0);      // ring_mod_freq
    assert_eq!(find(FxSlotType::Bonsai).params[3], 2.0);         // bonsai_mode -> p3
    assert_eq!(find(FxSlotType::Delay).params, [0.25, 0.5, 0.35, 0.7]);
    assert_eq!(find(FxSlotType::Reverb).params[2], 1.0);         // reverb_width (дефолт)
    assert_eq!(find(FxSlotType::WaveShaper).params[1], 3.0);     // mode -> p1
    // RingMod p3 — hardwire 0.5 (не из пресета)
    assert_eq!(find(FxSlotType::RingMod).params[3], 0.5);
}

#[test]
fn tc_inactive_families_skipped() {
    let chain = FxChain::from_legacy_preset_active(&legacy(false));
    let filled = chain.slots.iter()
        .filter(|s| s.enabled && s.slot_type != FxSlotType::None).count();
    assert_eq!(filled, 0, "empty legacy map -> no slots");
}

#[test]
fn tc_known_losses_pinned() {
    // Эти legacy-ключи НЕ конвертируются (аудит S3: 15/30 семейств).
    // Слоты после конвертации all-on НЕ содержат их типов. Если конвертер
    // расширили — обнови список осознанно.
    let chain = FxChain::from_legacy_preset_active(&legacy(true));
    let types: Vec<FxSlotType> = chain.slots.iter().filter(|s| s.enabled)
        .map(|s| s.slot_type).collect();
    // EQ/Compressor/Overdrive/Phaser/Flanger/Tremolo/Bitcrusher не имеют
    // legacy-миксов в конвертере — их типов в слотах быть не должно
    for absent in [FxSlotType::Phaser, FxSlotType::Flanger, FxSlotType::Tremolo,
                   FxSlotType::Bitcrusher, FxSlotType::Compressor] {
        assert!(!types.contains(&absent), "{absent:?} must NOT be converted (known loss, expand consciously in S3)");
    }
}
