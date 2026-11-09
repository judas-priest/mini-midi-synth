//! Golden-signal characterization harness (REFACTOR_AUDIT S0).
//! Pins bit-exact deterministic output of FX modules, the 16-slot fx_chain
//! (engine level) and every factory preset.
//!
//! Run:        cargo test --test golden_fx
//! Regenerate: MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx
//!             (commit the regenerated tests/golden/*.txt deliberately —
//!              a golden change IS an audible-behavior change)
//!
//! Goldens are platform/toolchain specific (float codegen differs between
//! x86_64 and aarch64). The determinism tests below are platform-independent.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{
    bonsai, chorus, combulator, delay, exciter, neuron, nimbus, resonator, reverb,
    spring_reverb, vocoder, ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE,
};
use mini_midi_synth::synth::fx_chain;

const SR: f32 = 48_000.0;
/// 0.25 s per preset — matches the runtime budget of all_presets_no_nan_no_silence.
const BLOCKS_PER_RENDER: usize = 48_000 / BLOCK_SIZE / 4; // 375
const FX_SAMPLES: usize = 4096;

// ── hashing ──────────────────────────────────────────────────────────────────

fn fnv1a_update(h: &mut u64, sample: f32) {
    for b in sample.to_bits().to_le_bytes() {
        *h ^= b as u64;
        *h = h.wrapping_mul(0x100000001b3);
    }
}

/// Deterministic xorshift32 input signal (fixed seed — no wall-clock).
fn xorshift_input(seed: u32) -> impl Iterator<Item = f32> {
    let mut st = seed;
    std::iter::from_fn(move || {
        st ^= st << 13;
        st ^= st >> 17;
        st ^= st << 5;
        Some((st as f32 / u32::MAX as f32) * 2.0 - 1.0)
    })
}

// ── golden files ─────────────────────────────────────────────────────────────

fn golden_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join(name)
}

/// Factory presets + the committed fixture dir (tests/golden/fixtures/external)
/// — never the live user/external preset directories. Golden tests must stay
/// machine-independent: a user editing their own presets must not flip them
/// (QW3 incident #106).
fn hermetic_patches() -> Vec<preset::Patch> {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/fixtures/external");
    preset::load_all_patches_from(Some(fixture.as_path()), None)
}

/// Compares `entries` against tests/golden/<file>, or (re)writes the file
/// when MINI_MIDI_GOLDEN_UPDATE is set.
fn check_golden(file: &str, entries: &[(String, u64)]) {
    let path = golden_path(file);
    if std::env::var("MINI_MIDI_GOLDEN_UPDATE").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut body = String::new();
        for (k, v) in entries {
            body.push_str(&format!("{k}\t{v:016x}\n"));
        }
        std::fs::write(&path, body).unwrap();
        eprintln!("goldens written: {}", path.display());
        return;
    }
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "golden file {} missing ({e}); run MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx",
            path.display()
        )
    });
    let map: std::collections::HashMap<&str, u64> = text
        .lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('\t')?;
            Some((k, u64::from_str_radix(v, 16).ok()?))
        })
        .collect();
    let mut bad = Vec::new();
    for (k, v) in entries {
        match map.get(k.as_str()) {
            Some(g) if *g == *v => {}
            Some(g) => bad.push(format!("{k}: golden {g:016x} != actual {v:016x}")),
            None => bad.push(format!("{k}: no golden entry (regenerate goldens)")),
        }
    }
    assert!(
        bad.is_empty(),
        "golden mismatches — behavior changed. Intentional? Regenerate with \
         MINI_MIDI_GOLDEN_UPDATE=1 and commit deliberately:\n{}",
        bad.join("\n")
    );
}

// ── FX module table ──────────────────────────────────────────────────────────

type StereoFx = Box<dyn FnMut(f32, f32) -> (f32, f32)>;

/// The FX families named by the audit S0 gap list. Two parameter sets each.
type FxFactory = Box<dyn Fn() -> StereoFx>;
fn fx_modules() -> Vec<(&'static str, Vec<FxFactory>)> {
    vec![
        ("chorus", vec![
            Box::new(|| Box::new({ let mut m = chorus::Chorus::new(SR); move |l, _r| m.tick(l, 0.5) })),
            Box::new(|| Box::new({ let mut m = chorus::Chorus::new(SR); move |l, _r| m.tick(l * 0.5 + 0.1, 0.9) })),
        ]),
        ("delay", vec![
            Box::new(|| Box::new({ let mut m = delay::StereoDelay::new(SR); move |l, r| m.tick(l, r, 0.3, 0.4, 0.4, 0.5, false, 0.4) })),
            Box::new(|| Box::new({ let mut m = delay::StereoDelay::new(SR); move |l, r| m.tick(l, r, 0.6, 0.2, 0.7, 0.3, true, 0.8) })),
        ]),
        ("reverb", vec![
            Box::new(|| Box::new({ let mut m = reverb::Reverb::new(SR); move |l, r| m.tick(l, r, 0.6, 0.4, 0.8, 0.1, 0.4) })),
            Box::new(|| Box::new({ let mut m = reverb::Reverb::new(SR); move |l, r| m.tick(l, r, 0.9, 0.2, 1.0, 0.3, 0.9) })),
        ]),
        ("spring_reverb", vec![
            Box::new(|| Box::new({ let mut m = spring_reverb::SpringReverb::new(SR); move |l, r| m.tick(l, r, 0.7, 0.6, 0.5, 0.3, 0.2, 0.0, 0.4) })),
            Box::new(|| Box::new({ let mut m = spring_reverb::SpringReverb::new(SR); move |l, r| m.tick(l, r, 0.3, 0.9, 0.8, 0.1, 0.8, 0.5, 0.7) })),
        ]),
        ("vocoder", vec![
            Box::new(|| Box::new({ let mut m = vocoder::Vocoder::new(SR); move |l, r| m.tick(l, r, (l + r) * 0.5, 0.5, 0.1, 0.6) })),
            Box::new(|| Box::new({ let mut m = vocoder::Vocoder::new(SR); move |l, r| m.tick(l, r, l.abs(), 0.9, 0.4, 0.9) })),
        ]),
        ("nimbus", vec![
            Box::new(|| Box::new({ let mut m = nimbus::Nimbus::new(SR); move |l, r| m.tick(l, r, 0.3, 0.4, 0.5, 0.5, 0.3, 0.2, 0.5) })),
            Box::new(|| Box::new({ let mut m = nimbus::Nimbus::new(SR); move |l, r| m.tick(l, r, 0.8, 0.9, 0.2, 0.9, 0.7, 0.6, 0.9) })),
        ]),
        ("combulator", vec![
            Box::new(|| Box::new({ let mut m = combulator::Combulator::new(SR); move |l, r| m.tick(l, r, 0.4, 0.5, 0.5, 0.3, 0.6, 0.5) })),
            Box::new(|| Box::new({ let mut m = combulator::Combulator::new(SR); move |l, r| m.tick(l, r, 0.8, 0.2, 0.7, 0.8, 0.3, 0.9) })),
        ]),
        ("resonator", vec![
            Box::new(|| Box::new({ let mut m = resonator::Resonator::new(SR); move |l, r| m.tick((l + r) * 0.5, 0.6) })),
            Box::new(|| Box::new({ let mut m = resonator::Resonator::new(SR); move |l, r| m.tick((l - r) * 0.5, 0.95) })),
        ]),
        ("exciter", vec![
            Box::new(|| Box::new({ let mut m = exciter::Exciter::new(SR); move |l, r| m.tick(l, r, 0.5, 0.5, 0.4, 0.4) })),
            Box::new(|| Box::new({ let mut m = exciter::Exciter::new(SR); move |l, r| m.tick(l, r, 0.9, 0.8, 0.9, 0.8) })),
        ]),
        ("bonsai", vec![
            Box::new(|| Box::new({ let mut m = bonsai::Bonsai::new(SR); move |l, r| m.tick(l, r, 0.5, 0.5, 0.5, bonsai::BonsaiMode::from_param(0.3), 0.5) })),
            Box::new(|| Box::new({ let mut m = bonsai::Bonsai::new(SR); move |l, r| m.tick(l, r, 0.9, 0.2, 0.8, bonsai::BonsaiMode::from_param(0.8), 0.9) })),
        ]),
        ("neuron", vec![
            Box::new(|| Box::new({ let mut m = neuron::Neuron::new(SR); move |l, r| m.tick(l, r, 0.5, 0.4, 0.3, 0.0, 0.5, 0.6, 0.5, 0.4) })),
            Box::new(|| Box::new({ let mut m = neuron::Neuron::new(SR); move |l, r| m.tick(l, r, 0.9, 0.8, 0.7, 0.2, 0.1, 0.3, 0.9, 0.8) })),
        ]),
    ]
}

fn render_fx(fx: &mut StereoFx) -> (u64, f32) {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut peak = 0.0_f32;
    for s in xorshift_input(0x1234567).take(FX_SAMPLES) {
        let (l, r) = fx(s, s * 0.7);
        fnv1a_update(&mut h, l);
        fnv1a_update(&mut h, r);
        peak = peak.max(l.abs()).max(r.abs());
    }
    (h, peak)
}

// ── TC1: determinism (platform-independent) ──────────────────────────────────

#[test]
fn fx_modules_render_bit_deterministic() {
    // ВАЖНО: все FX stateful (delay lines, LFO phase, reverb buffers) —
    // детерминизм проверяется на ДВУХ СВЕЖИХ инстансах одного param-set:
    // fx_modules() отдаёт фабрики, каждая создаёт новый Box<dyn FnMut>.
    // Рендерить один и тот же инстанс дважды НЕЛЬЗЯ — второй проход получит
    // другое состояние (delay lines не пустые) и тест был бы ложным RED.
    for (name, variants) in fx_modules() {
        for (set, factory) in variants.iter().enumerate() {
            let mut fx = factory();
            let (a, peak_a) = render_fx(&mut fx);
            let mut fx2 = factory();
            let (b, _) = render_fx(&mut fx2);
            assert_eq!(a, b, "fx '{name}' set {set}: not bit-deterministic across two fresh instances");
            assert!(peak_a > 1e-4, "fx '{name}' set {set}: output silent (peak {peak_a})");
            assert!(peak_a.is_finite(), "fx '{name}' set {set}: non-finite output");
        }
    }
}

// ── TC4: fx module goldens ───────────────────────────────────────────────────

#[test]
fn golden_fx_module_hashes_match() {
    let mut entries = Vec::new();
    for (name, variants) in fx_modules() {
        for (set, factory) in variants.iter().enumerate() {
            // fresh instance, state capture point = start of render
            let mut fx = factory();
            let (h, _) = render_fx(&mut fx);
            entries.push((format!("{name}_set{set}"), h));
        }
    }
    check_golden("fx_module_hashes.txt", &entries);
}

// ── engine helpers ───────────────────────────────────────────────────────────

fn load_init_patch(engine: &mut SynthEngine, patch: &preset::Patch) {
    let spent = engine.handle_control(ControlEvent::load_patch_from(0, patch));
    drop(spent);
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = engine.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
}

/// Renders 0.25 s of a preset through tick_block under no_denormals (the
/// production wrapper, audio.rs:199) and returns the FNV-1a hash of the
/// interleaved output bits.
fn render_preset_hash(patch: &preset::Patch) -> u64 {
    let mut engine = SynthEngine::new(SR);
    engine.set_patches(vec![patch.clone()]);
    load_init_patch(&mut engine, patch);
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });

    let mut h: u64 = 0xcbf29ce484222325;
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    no_denormals::no_denormals(|| {
        for _ in 0..BLOCKS_PER_RENDER {
            engine.tick_block(&mut l, &mut r);
            for i in 0..BLOCK_SIZE {
                fnv1a_update(&mut h, l[i]);
                fnv1a_update(&mut h, r[i]);
                assert!(l[i].is_finite() && r[i].is_finite(), "non-finite output");
            }
        }
    });
    h
}

// ── TC2: engine determinism (platform-independent) ───────────────────────────

#[test]
fn engine_preset_render_bit_deterministic() {
    let patches = hermetic_patches();
    let keys = ["epiano", "piano", "chorus", "neuron", "reverb", "vocoder", "nimbus",
        "combulator", "resonator", "exciter", "spring", "bonsai", "organ", "fm", "bell", "saw"];
    let mut subset: Vec<&preset::Patch> = patches
        .iter()
        .filter(|p| keys.iter().any(|k| p.name.to_lowercase().contains(k)))
        .take(16)
        .collect();
    if subset.is_empty() {
        subset = patches.iter().take(8).collect();
    }
    assert!(!subset.is_empty(), "no factory presets compiled in");
    for p in &subset {
        let a = render_preset_hash(p);
        let b = render_preset_hash(p);
        assert_eq!(a, b, "preset '{}' not bit-deterministic across two runs", p.name);
    }
}

// ── TC3: golden hash for EVERY factory preset ────────────────────────────────

#[test]
fn golden_preset_hashes_match() {
    let patches = hermetic_patches();
    let mut entries = Vec::new();
    // hermetic_patches merges factory + the committed fixture dir; the fixture
    // "Chorus Pad" deliberately duplicates the factory name, so names collide
    // across sources. Keys must stay unique or the golden map collapses the
    // duplicates and every second entry is a false RED.
    let mut seen = std::collections::HashSet::new();
    for p in &patches {
        let mut key = p.name.clone();
        let mut n = 2;
        while !seen.insert(key.clone()) {
            key = format!("{}#{}", p.name, n);
            n += 1;
        }
        entries.push((key, render_preset_hash(p)));
    }
    check_golden("preset_hashes.txt", &entries);
}

// ── TC5: engine-level 16-slot fx_chain goldens ───────────────────────────────

/// (slot type number, FxSlotType name) — fx_chain.rs:11-51 (repr u8).
/// All 31 non-None variants, canonical by type number.
const CHAIN_SLOTS: &[(f32, &str)] = &[
    (1.0, "Overdrive"), (2.0, "Tape"), (3.0, "Neuron"), (4.0, "Bonsai"),
    (5.0, "WaveShaper"), (6.0, "Airwindows"), (7.0, "Chorus"),
    (8.0, "BbdEnsemble"), (9.0, "Flanger"), (10.0, "Phaser"),
    (11.0, "Tremolo"), (12.0, "Rotary"), (13.0, "Delay"),
    (14.0, "FloatyDelay"), (15.0, "Reverb"), (16.0, "Reverb2"),
    (17.0, "SpringReverb"), (18.0, "ConvReverb"), (19.0, "Nimbus"),
    (20.0, "RingMod"), (21.0, "FreqShift"), (22.0, "Bitcrusher"),
    (23.0, "Resonator"), (24.0, "Combulator"), (25.0, "Treemonster"),
    (26.0, "Vocoder"), (27.0, "GraphicEq"), (28.0, "Compressor"),
    (29.0, "MsTool"), (30.0, "Conditioner"), (31.0, "Exciter"),
];

#[test]
fn golden_engine_fx_chain_slots() {
    let base = hermetic_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists");
    let mut entries = Vec::new();
    for (ty, name) in CHAIN_SLOTS {
        let mut patch = base.clone();
        // FxChain becomes active iff the fx0_type key exists (patch_params.rs:464)
        patch.params.insert("fx0_type".into(), *ty);
        patch.params.insert("fx0_en".into(), 1.0);
        patch.params.insert("fx0_mix".into(), 0.5);
        let h = render_preset_hash(&patch);
        entries.push((format!("fx_chain_{name}"), h));
    }
    check_golden("fx_chain_engine_hashes.txt", &entries);
}

// ── S2 pre-pin: parameter sweep golden (2026-11-07) ─────────────────────
// Свип полного набора параметров (~420 ключей) в КОНТЕКСТАХ:
//  - каждый из 30 движков osc_type (движко-специфичные ключи);
//  - «fx_all» — все _mix/_en FX-ключей = 1 (legacy + слоты): масштабы
//    spring/neuron/tape/bonsai/exciter/... через CachedFxParams;
//  - «lfo_open» — LFO-глубины = 1: темповые параметры модуляции.
// В golden попадают только ЖИВЫЕ ключи (hash != baseline) — набор
// самоподдерживающийся. Пинит build_voice_params и CachedFxParams —
// ровно поверхность реестра S2.
// Расхождение = реестр изменил семантику масштаба. Осознанное обновление
// goldens — только с diff-ревью звучания (MINI_MIDI_GOLDEN_UPDATE=1).

const SWEEP_ENGINES: u32 = 30; // OscType 0..=29 (oscillator.rs:104-133)

fn render_sweep(
    name: &str, overlays: &[(&str, f32)],
    key_filter: impl Fn(&str) -> bool,
) -> Vec<(String, u64)> {
    let patches = hermetic_patches();
    let patch = patches.iter().find(|p| p.name.eq_ignore_ascii_case("Init"))
        .expect("Init preset not found").clone();
    // Полный дефолтный набор ключей (JSON пресетов sparse) + оверлей
    let mut base = mini_midi_synth::synth::PatchParams::default().to_map();
    for (k, v) in &patch.params {
        base.insert(k.clone(), *v);
    }
    for (k, v) in overlays {
        base.insert(k.to_string(), *v);
    }
    let mut p0 = patch.clone();
    p0.params = base.clone();
    let baseline = render_preset_hash(&p0);
    let mut entries = Vec::new();
    for (k, v) in &base {
        if !key_filter(k) { continue; }
        let mut m = base.clone();
        m.insert(k.clone(), if *v == 0.5 { 0.25 } else { 0.5 });
        let mut p = patch.clone();
        p.params = m;
        let h = render_preset_hash(&p);
        if h != baseline {
            entries.push((format!("{name}|{k}"), h));
        }
    }
    entries
}

type SweepCtx = (String, Vec<(&'static str, f32)>, fn(&str) -> bool);

fn sweep_contexts() -> Vec<SweepCtx> {
    let all: fn(&str) -> bool = |_| true;
    let mut c: Vec<SweepCtx> = (0..SWEEP_ENGINES)
        .map(|t| (format!("e{t}"), vec![("osc_type", t as f32)], all))
        .collect();
    // FX-семейства: у каждого свой флаг активации (spring требует
    // reverb_type>0.5 — mod.rs:3871; свипается только семейство —
    // рантайм ~секунды на контекст)
    let fam: Vec<SweepCtx> = vec![
        ("spring".to_string(), vec![("reverb_type", 1.0), ("spring_mix", 1.0)],
            |k| k.starts_with("spring_")),
        ("tape".to_string(), vec![("tape_mix", 1.0)], |k| k.starts_with("tape_")),
        ("neuron".to_string(), vec![("neuron_mix", 1.0)], |k| k.starts_with("neuron_")),
        ("ring_mod".to_string(), vec![("ring_mod_mix", 1.0)], |k| k.starts_with("ring_mod_")),
        ("freq_shift".to_string(), vec![("freq_shift_mix", 1.0)], |k| k.starts_with("freq_shift_")),
        ("wave_shaper".to_string(), vec![("wave_shaper_mix", 1.0)], |k| k.starts_with("wave_shaper_")),
        ("inter_ws".to_string(), vec![("inter_ws_mix", 1.0)], |k| k.starts_with("inter_ws_")),
        ("delay".to_string(), vec![("delay_mix", 1.0)],
            |k| k.starts_with("delay_")),
        ("env".to_string(), vec![("filter_env_amount", 0.5), ("noise_level", 0.3)],
            |k| k.ends_with("_shape") || k == "filter_hold"),
        // LFO2 с открытыми глубинами: rate/waveform/deform становятся слышимыми
        ("lfo2".to_string(), vec![
            ("lfo2_pitch_depth", 1.0), ("lfo2_filter_depth", 1.0), ("lfo2_amp_depth", 1.0),
        ], |k| k.starts_with("lfo2_")),
        ("lfo1".to_string(), vec![
            ("lfo_pitch_depth", 1.0), ("lfo_filter_depth", 1.0), ("lfo_amp_depth", 1.0),
        ], |k| k.starts_with("lfo1_") || k.starts_with("lfo_") && !k.starts_with("lfo2")),
    ];
    c.extend(fam);
    // Слот-FX (Task 9, S3 prereq): каждый тип с ≥1 помеченным параметром.
    // 31 не-None тип, без меток 6 (Chorus, Flanger, Phaser, Tremolo,
    // Compressor, Bitcrusher) → 25 контекстов (план говорил «24 из 31» —
    // пересчёт param_labels даёт 25; проверено assert'ом ниже).
    // Имя контекста = FxSlotType::name() — ТОТ ЖЕ источник имён, что и в
    // slot_dead_scalings_stay_dead (не ручной ввод).
    let mut slots_with_params = 0usize;
    for t in 1..=31u8 {
        let ty = fx_chain::FxSlotType::from_u8(t);
        if !ty.param_labels().iter().any(|l| l.is_some()) { continue; }
        slots_with_params += 1;
        c.push((
            ty.name().to_string(),
            vec![("fx0_type", t as f32), ("fx0_en", 1.0), ("fx0_mix", 1.0)],
            |k| k.starts_with("fx0_p"),
        ));
    }
    assert_eq!(slots_with_params, 25, "GUI-labeled slot params changed — resync slot sweep contexts");
    c
}

#[test]
fn golden_param_sweep_hashes_match() {
    let mut entries = Vec::new();
    for (name, overlays, pred) in sweep_contexts() {
        entries.extend(render_sweep(&name, &overlays, pred));
    }
    assert!(entries.len() > 600, "sweep too small: {}", entries.len());
    check_golden("param_sweep_hashes.txt", &entries);
}

/// Мёртвые масштабы слотов (аудит 2026-11-08): эти (тип, pN) НЕ должны
/// появляться в свипе (hash == baseline). Если появился — масштаб починили
/// → осознанная регенерация + прослушивание.
/// Имена — из FxSlotType::name(), тот же источник, что и контексты свипа
/// (ключ НЕ набивается руками).
#[test]
fn slot_dead_scalings_stay_dead() {
    let dead: [(fx_chain::FxSlotType, usize); 6] = [
        (fx_chain::FxSlotType::WaveShaper, 0),
        (fx_chain::FxSlotType::Exciter,    1),
        (fx_chain::FxSlotType::Neuron,     3),
        (fx_chain::FxSlotType::MsTool,     0),
        (fx_chain::FxSlotType::MsTool,     1),
        (fx_chain::FxSlotType::MsTool,     2),
    ];
    let file = std::fs::read_to_string(golden_path("param_sweep_hashes.txt")).unwrap();
    for (ty, p) in dead {
        let key = format!("{}|fx0_p{}", ty.name(), p);
        assert!(
            !file.contains(&key),
            "dead scaling now live: {key} — regen goldens + listening review"
        );
    }
}
