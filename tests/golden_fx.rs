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
    let patches = preset::load_all_patches();
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
    let patches = preset::load_all_patches();
    let mut entries = Vec::new();
    // load_all_patches merges factory + external + user dirs; names collide
    // across sources ("Clarinet" exists both as a factory preset and as a
    // converted Surge preset). Keys must stay unique or the golden map
    // collapses the duplicates and every second entry is a false RED.
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
const CHAIN_SLOTS: &[(f32, &str)] = &[
    (13.0, "Delay"), (7.0, "Chorus"), (17.0, "SpringReverb"),
    (19.0, "Nimbus"), (26.0, "Vocoder"),
];

#[test]
fn golden_engine_fx_chain_slots() {
    let base = preset::load_all_patches()
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
