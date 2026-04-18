# Polivoks Filter — Design Spec
Date: 2026-04-18

## Summary

Add a Polivoks (Soviet К140УД12 op-amp SVF) filter emulation to the synth, plus a factory patch "Polivoks Lead".

## Algorithm

**Approach:** Full nonlinear model with 2x oversampling (same pattern as Moog/Diode ladders).

**New FilterTypes:** `PolivoksLP` (37), `PolivoksBP` (38)

**New state fields in Filter:**
- `pv_s1: f32` — integrator 1 (bandpass)
- `pv_s2: f32` — integrator 2 (lowpass)
- `pv_delay: f32` — z⁻¹ for half-sample resonance feedback delay

**New coefficient fields:**
- `pv_tune: f32` — frequency coefficient (same derivation as moog_tune)
- `pv_res: f32` — resonance feedback amount (0..~0.95 before self-osc)

**tick_polivoks — 2x oversampled loop:**
```
for _ in 0..2:
    feedback = (pv_s2 + pv_delay) * 0.5
    pv_delay = pv_s2

    // Resonance suppression at high input levels (authentic К140УД12 behavior)
    eff_res = pv_res * (1.0 - starve * |input| * 0.5).max(0)

    // Asymmetric clip on input (models op-amp slew asymmetry)
    x = asym_clip((input - eff_res * feedback) * drive_gain)

    // Two integrators with g modulated by Starve (slew rate limiting)
    g_eff = pv_tune * (1.0 - starve * |pv_s1| * 0.3).clamp(0.1, 1.0)
    v1 = pv_s1 + g_eff * (x - pv_s1)      // bandpass
    v2 = pv_s2 + g_eff * (v1 - pv_s2)     // lowpass
    pv_s1 = 2*v1 - pv_s1
    pv_s2 = 2*v2 - pv_s2

output = v2 (LP) or v1 (BP)
```

**Asymmetric clipper:**
```rust
fn asym_clip(x: f32) -> f32 {
    if x >= 0.0 { fast_tanh(x * 1.2) * 0.9 }
    else        { fast_tanh(x * 0.7) }
}
```

**Coefficient update (update_polivoks_coefficients):**
- `pv_tune`: same formula as moog_tune but without 2x oversampling correction (Polivoks is less frequency-sensitive)
- `pv_res`: `resonance * 0.95`

## New Parameters

Added to `VoiceParams` and passed to `Filter` — same pattern as `svf_morph`:

| Field | Range | Default | Meaning |
|-------|-------|---------|---------|
| `filter_drive` | 0..1 | 0.0 | Input gain into nonlinearity (1.0 = 4x gain) |
| `filter_starve` | 0..1 | 0.0 | Power-supply starvation: reduces g and res at high signal levels |

Both are stored as `"filter_drive"` and `"filter_starve"` keys in the patch BTreeMap.

## Factory Patch: polivoks_lead

- Osc1: Saw, Osc2: Square, detune -7 cents
- Filter: PolivoksLP, cutoff 1200 Hz, resonance 0.65
- filter_drive 0.4, filter_starve 0.2
- Amp env: A=2ms D=80ms S=0.7 R=200ms
- Filter env: A=1ms D=120ms amount +40%
- FX: overdrive (light) + short reverb

## Files Changed

| File | Change |
|------|--------|
| `src/synth/filter.rs` | +2 FilterTypes, +5 state/coeff fields, +`tick_polivoks`, +`update_polivoks_coefficients`, +`asym_clip`, +`is_polivoks()` helper |
| `src/synth/voice.rs` | +`filter_drive`, `filter_starve` in VoiceParams; pass to filter in apply_params |
| `src/gui/params.rs` | UI sliders for Drive and Starve (visible for all filter types, same as svf_morph) |
| `presets/polivoks_lead.json` | new factory patch |
| `src/preset.rs` | register patch in FACTORY_PRESETS |
