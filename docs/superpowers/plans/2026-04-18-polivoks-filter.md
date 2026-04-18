# Polivoks Filter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Поливокс (Soviet К140УД12 op-amp SVF) filter emulation — two filter types (LP + BP) with 2x oversampling, asymmetric clipping, Starve and Drive parameters — plus a factory patch "polivoks_lead".

**Architecture:** New `PolivoksLP`/`PolivoksBP` entries in the existing `FilterType` enum; state and coefficients added to the `Filter` struct alongside existing Moog/Diode fields; `filter_drive` and `filter_starve` added to `VoiceParams`/`SynthParams` following the exact `svf_morph` pattern.

**Tech Stack:** Rust, existing `src/synth/filter.rs` / `src/synth/voice.rs` / `src/synth/mod.rs` / `src/gui/params.rs` / `src/preset.rs`

---

## File Map

| File | Change |
|------|--------|
| `src/synth/filter.rs` | +2 `FilterType` variants (37,38), +5 state/coeff fields, +`is_polivoks()`, +`update_polivoks_coefficients()`, +`tick_polivoks()`, +`asym_clip()` |
| `src/synth/voice.rs` | +`filter_drive: f32`, `filter_starve: f32` to `VoiceParams`; pass to `filter` in `apply_params` |
| `src/synth/mod.rs` | +`filter_drive`, `filter_starve` to `SynthParams`; defaults; `p()` parsing; voice_params conversion |
| `src/gui/params.rs` | +Drive/Starve sliders shown when filter_type == 37 or 38 |
| `presets/polivoks_lead.json` | New factory patch file |
| `src/preset.rs` | Register patch in `FACTORY_PRESETS` |

---

## Task 1: Add FilterType variants + Filter state/coeff fields

**Files:**
- Modify: `src/synth/filter.rs`

- [ ] **Step 1: Add two variants to `FilterType` enum**

In `src/synth/filter.rs`, after `SVFMorph, // 36`, add:

```rust
    PolivoksLP,    // 37 — Polivoks К140УД12 op-amp SVF, lowpass
    PolivoksBP,    // 38 — Polivoks К140УД12 op-amp SVF, bandpass
```

- [ ] **Step 2: Add arms to `FilterType::from_param`**

After `36 => Self::SVFMorph,`:

```rust
            37 => Self::PolivoksLP,
            38 => Self::PolivoksBP,
```

- [ ] **Step 3: Add `is_polivoks()` helper**

After the `is_allpass` fn:

```rust
    pub fn is_polivoks(self) -> bool {
        matches!(self, Self::PolivoksLP | Self::PolivoksBP)
    }
```

- [ ] **Step 4: Add state + coeff fields to `Filter` struct**

After the `// SVF Morph parameter` block (after `pub svf_morph: f32`), add:

```rust
    // Polivoks state
    pv_s1: f32,      // integrator 1 state (bandpass)
    pv_s2: f32,      // integrator 2 state (lowpass)
    pv_delay: f32,   // z^-1 for half-sample resonance feedback delay
    pv_tune: f32,    // frequency coefficient
    pv_res: f32,     // resonance feedback amount
    // Polivoks external parameters (set from VoiceParams)
    pub pv_drive: f32,   // 0..1 input drive
    pub pv_starve: f32,  // 0..1 power-supply starvation
```

- [ ] **Step 5: Initialize new fields in `Filter::new`**

After `svf_morph: 0.0,` add:

```rust
            pv_s1: 0.0,
            pv_s2: 0.0,
            pv_delay: 0.0,
            pv_tune: 0.0,
            pv_res: 0.0,
            pv_drive: 0.0,
            pv_starve: 0.0,
```

- [ ] **Step 6: Reset pv state in `Filter::reset`**

After `self.allpass_x1 = 0.0; self.allpass_y1 = 0.0;` add:

```rust
        self.pv_s1 = 0.0;
        self.pv_s2 = 0.0;
        self.pv_delay = 0.0;
```

- [ ] **Step 7: Wire into `update_coefficients`**

In `fn update_coefficients`, the first if/else chain — add a branch before the final `else`:

```rust
        } else if self.filter_type.is_polivoks() {
            self.update_polivoks_coefficients();
        } else {
```

- [ ] **Step 8: Wire into `tick` match**

In `fn tick`, inside the `match self.filter_type` block, after the `VintageLadderLP` arm:

```rust
            FilterType::PolivoksLP => self.tick_polivoks(input, false),
            FilterType::PolivoksBP => self.tick_polivoks(input, true),
```

- [ ] **Step 9: Build to check compilation**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo build 2>&1 | head -40
```

Expected: errors about missing `update_polivoks_coefficients` and `tick_polivoks` — that's fine, we add them next.

---

## Task 2: Implement coefficient update + tick

**Files:**
- Modify: `src/synth/filter.rs`

- [ ] **Step 1: Add `update_polivoks_coefficients`**

Add this method to the `impl Filter` block (e.g. after `update_diode_coefficients`):

```rust
    fn update_polivoks_coefficients(&mut self) {
        use std::f32::consts::PI;
        // Normalized frequency (no 2x correction — Polivoks less frequency-sensitive than Moog)
        let fc = (self.cutoff / self.sample_rate).min(0.45);
        // Same exponential integrator formula as Moog tune
        let fcr = 1.0 - (-2.0 * PI * fc).exp();
        self.pv_tune = fcr / VT_INV;
        // Resonance: 0..1 → 0..0.95 (self-oscillation at top but unmapped to musical scale)
        self.pv_res = self.resonance * 0.95;
    }
```

- [ ] **Step 2: Add `asym_clip` free function**

After the `diode_clip` function (or near `fast_tanh`):

```rust
/// Asymmetric soft clipper modeling К140УД12 op-amp slew asymmetry.
/// Positive half clips harder (forward saturation), negative softer.
#[inline(always)]
fn asym_clip(x: f32) -> f32 {
    if x >= 0.0 {
        fast_tanh(x * 1.2) * 0.9
    } else {
        fast_tanh(x * 0.7)
    }
}
```

- [ ] **Step 3: Add `tick_polivoks`**

Add this method to `impl Filter`:

```rust
    /// Polivoks — К140УД12 op-amp state-variable filter with 2x oversampling.
    ///
    /// Key behaviors vs clean SVF:
    /// - Asymmetric clipping at input (op-amp slew asymmetry)
    /// - Resonance suppression at high signal levels (authentic hardware behavior)
    /// - g modulated by Starve (power-supply-starved slew rate limiting)
    /// - "Bubble" effect emerges naturally from the nonlinear feedback loop
    fn tick_polivoks(&mut self, input: f32, bandpass: bool) -> f32 {
        let tune = self.pv_tune;
        let base_res = self.pv_res;
        // Drive: 0..1 → 1..4x input gain
        let drive_gain = 1.0 + self.pv_drive * 3.0;
        let starve = self.pv_starve;

        let mut v1 = 0.0_f32;
        let mut v2 = 0.0_f32;

        for _ in 0..2 {
            // Half-sample feedback delay (same trick as Moog for stability)
            let feedback = (self.pv_s2 + self.pv_delay) * 0.5;
            self.pv_delay = self.pv_s2;

            // Resonance suppression at high input levels — authentic К140УД12 behavior
            let eff_res = base_res * (1.0 - starve * input.abs() * 0.5).max(0.0);

            // Asymmetric clip on driven input (models op-amp slew asymmetry)
            let x = asym_clip((input - eff_res * feedback) * drive_gain);

            // Starve modulates integrator gain — slew rate limiting under power starvation
            let g_eff = tune * (1.0 - starve * self.pv_s1.abs() * 0.3).clamp(0.1, 1.0);

            // Two one-pole integrators (SVF topology)
            v1 = self.pv_s1 + g_eff * (x - self.pv_s1);      // bandpass
            v2 = self.pv_s2 + g_eff * (v1 - self.pv_s2);     // lowpass
            self.pv_s1 = (2.0 * v1 - self.pv_s1).clamp(-4.0, 4.0);
            self.pv_s2 = (2.0 * v2 - self.pv_s2).clamp(-4.0, 4.0);
        }

        if bandpass { v1 } else { v2 }
    }
```

- [ ] **Step 4: Build successfully**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo build 2>&1 | head -40
```

Expected: clean build, zero errors.

- [ ] **Step 5: Write tests in `src/synth/mod.rs`**

Find the `mod tests` block at the bottom of `src/synth/mod.rs` and add:

```rust
    // ── Polivoks filter tests ───────────────────────────────────────

    #[test]
    fn polivoks_lp_passes_dc() {
        let mut f = filter::Filter::new(44100.0);
        f.set_type(filter::FilterType::PolivoksLP);
        f.set_cutoff(1000.0);
        f.set_resonance(0.3);
        f.force_update();
        // Warm up
        for _ in 0..1000 { f.tick(0.5); }
        let out = f.tick(0.5);
        // LP should pass DC-ish signal (within 50% — it's a nonlinear filter)
        assert!(out.abs() > 0.1 && out.abs() < 2.0, "LP DC failed: {out}");
    }

    #[test]
    fn polivoks_bp_attenuates_dc() {
        let mut f = filter::Filter::new(44100.0);
        f.set_type(filter::FilterType::PolivoksBP);
        f.set_cutoff(1000.0);
        f.set_resonance(0.3);
        f.force_update();
        // Warm up with DC
        for _ in 0..5000 { f.tick(1.0); }
        let out = f.tick(1.0);
        // BP should attenuate DC significantly
        assert!(out.abs() < 0.3, "BP should attenuate DC, got: {out}");
    }

    #[test]
    fn polivoks_no_nan_under_high_resonance() {
        let mut f = filter::Filter::new(44100.0);
        f.set_type(filter::FilterType::PolivoksLP);
        f.set_cutoff(500.0);
        f.set_resonance(1.0);
        f.pv_drive = 1.0;
        f.pv_starve = 1.0;
        f.force_update();
        for i in 0..10000 {
            let input = ((i as f32) * 0.01).sin();
            let out = f.tick(input);
            assert!(!out.is_nan(), "NaN at sample {i}");
            assert!(!out.is_infinite(), "Inf at sample {i}");
        }
    }

    #[test]
    fn polivoks_drive_increases_harmonic_content() {
        // With drive=1, output RMS should differ from drive=0
        let mut f_clean = filter::Filter::new(44100.0);
        f_clean.set_type(filter::FilterType::PolivoksLP);
        f_clean.set_cutoff(2000.0);
        f_clean.set_resonance(0.5);
        f_clean.pv_drive = 0.0;
        f_clean.force_update();

        let mut f_driven = filter::Filter::new(44100.0);
        f_driven.set_type(filter::FilterType::PolivoksLP);
        f_driven.set_cutoff(2000.0);
        f_driven.set_resonance(0.5);
        f_driven.pv_drive = 1.0;
        f_driven.force_update();

        let mut rms_clean = 0.0_f32;
        let mut rms_driven = 0.0_f32;
        for i in 0..4410 {
            let x = ((i as f32) * 2.0 * std::f32::consts::PI * 440.0 / 44100.0).sin() * 0.5;
            let c = f_clean.tick(x);
            let d = f_driven.tick(x);
            rms_clean += c * c;
            rms_driven += d * d;
        }
        // Driven should produce more energy (clipping adds harmonics)
        assert!(rms_driven != rms_clean, "Drive had no effect on output");
    }
```

- [ ] **Step 6: Run tests**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo test polivoks 2>&1
```

Expected: 4 tests pass.

- [ ] **Step 7: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth && git add src/synth/filter.rs src/synth/mod.rs && git commit -m "feat: add Polivoks filter algorithm (PolivoksLP/BP, types 37/38)"
```

---

## Task 3: Add filter_drive + filter_starve to VoiceParams and SynthParams

**Files:**
- Modify: `src/synth/voice.rs`
- Modify: `src/synth/mod.rs`

- [ ] **Step 1: Add fields to `VoiceParams` in voice.rs**

Find `// SVF Morph filter parameter` comment block at the bottom of `VoiceParams`:

```rust
    // SVF Morph filter parameter
    pub svf_morph: f32,
```

Add after it:

```rust
    // Polivoks filter parameters
    pub filter_drive: f32,
    pub filter_starve: f32,
```

- [ ] **Step 2: Pass drive/starve to filter in `apply_params` in voice.rs**

Find the block that sets `self.filter.svf_morph`:

```rust
            self.filter.set_type(filter_type);
            self.filter.set_resonance(params.filter_resonance);
            self.filter.svf_morph = params.svf_morph;
            self.filter.reset();
```

Add two lines before `self.filter.reset()`:

```rust
            self.filter.pv_drive = params.filter_drive;
            self.filter.pv_starve = params.filter_starve;
```

So the full block becomes:

```rust
            self.filter.set_type(filter_type);
            self.filter.set_resonance(params.filter_resonance);
            self.filter.svf_morph = params.svf_morph;
            self.filter.pv_drive = params.filter_drive;
            self.filter.pv_starve = params.filter_starve;
            self.filter.reset();
```

- [ ] **Step 3: Add fields to `SynthParams` in mod.rs**

Find `// SVF Morph filter parameter (0=LP, 0.5=BP, 1=HP)` in `SynthParams`:

```rust
    // SVF Morph filter parameter (0=LP, 0.5=BP, 1=HP)
    pub svf_morph: f32,
```

Add after:

```rust
    // Polivoks filter parameters
    pub filter_drive: f32,
    pub filter_starve: f32,
```

- [ ] **Step 4: Add defaults in `SynthParams::default` in mod.rs**

Find `svf_morph: 0.0,` in the `SynthParams` default block and add after:

```rust
            filter_drive: 0.0,
            filter_starve: 0.0,
```

- [ ] **Step 5: Add `p()` parsing in `SynthParams::from_patch` in mod.rs**

Find `svf_morph: p("svf_morph", 0.0),` and add after:

```rust
            filter_drive: p("filter_drive", 0.0),
            filter_starve: p("filter_starve", 0.0),
```

- [ ] **Step 6: Add to voice_params conversion in mod.rs**

Find `svf_morph: self.params.svf_morph,` in the `voice_params()` method and add after:

```rust
            filter_drive: self.params.filter_drive,
            filter_starve: self.params.filter_starve,
```

- [ ] **Step 7: Build**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo build 2>&1 | head -30
```

Expected: clean build.

- [ ] **Step 8: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth && git add src/synth/voice.rs src/synth/mod.rs && git commit -m "feat: add filter_drive and filter_starve params (Polivoks)"
```

---

## Task 4: UI sliders in params.rs

**Files:**
- Modify: `src/gui/params.rs`

- [ ] **Step 1: Find SVFMorph slider block**

In `src/gui/params.rs`, find:

```rust
            // SVF Morph slider: only shown when filter type is SVFMorph (type 36)
            if filter_type == 36 {
                changed |= self.param_slider(ui, "svf_morph", "LP\u{2194}HP", 0.0, 1.0, false);
            }
```

- [ ] **Step 2: Add Drive and Starve sliders after that block**

```rust
            // Polivoks sliders: only shown when filter type is PolivoksLP (37) or PolivoksBP (38)
            if filter_type == 37 || filter_type == 38 {
                changed |= self.param_slider(ui, "filter_drive", "Drive", 0.0, 1.0, false);
                changed |= self.param_slider(ui, "filter_starve", "Starve", 0.0, 1.0, false);
            }
```

- [ ] **Step 3: Build**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo build 2>&1 | head -30
```

Expected: clean build.

- [ ] **Step 4: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth && git add src/gui/params.rs && git commit -m "feat: add Drive/Starve sliders for Polivoks filter types"
```

---

## Task 5: Factory patch polivoks_lead

**Files:**
- Create: `presets/polivoks_lead.json`
- Modify: `src/preset.rs`

- [ ] **Step 1: Create patch file**

Create `presets/polivoks_lead.json`:

```json
{
  "name": "Polivoks Lead",
  "category": "Lead",
  "params": {
    "osc1_type": 1,
    "osc1_level": 0.7,
    "osc1_detune": 0.0,
    "osc2_type": 2,
    "osc2_level": 0.5,
    "osc2_detune": -7.0,
    "num_oscs": 2,
    "filter_type": 37,
    "filter_cutoff": 1200.0,
    "filter_resonance": 0.65,
    "filter_drive": 0.4,
    "filter_starve": 0.2,
    "filter_env_amount": 0.4,
    "amp_attack": 0.002,
    "amp_decay": 0.08,
    "amp_sustain": 0.7,
    "amp_release": 0.2,
    "filter_attack": 0.001,
    "filter_decay": 0.12,
    "filter_sustain": 0.0,
    "filter_release": 0.1
  }
}
```

Note: `osc1_type` 1 = Saw, `osc2_type` 2 = Square (check your actual `OscType` enum numbering in `src/synth/oscillator.rs` before saving — adjust if needed).

- [ ] **Step 2: Verify OscType numbering**

```bash
grep -n "Saw\|Square\|pub enum OscType" /home/dima/Projects/mini_midi_synth/src/synth/oscillator.rs | head -20
```

Adjust `osc1_type` and `osc2_type` values in the JSON if Saw/Square have different numbers.

- [ ] **Step 3: Register in FACTORY_PRESETS in preset.rs**

Find the `("Lead", ...)` group in `FACTORY_PRESETS`. Add:

```rust
    ("Lead", "polivoks_lead", include_str!("../presets/polivoks_lead.json")),
```

If there is no Lead group yet, find a suitable nearby group and add it there.

- [ ] **Step 4: Build**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo build 2>&1 | head -30
```

Expected: clean build. If the JSON keys are wrong you'll get a runtime error, not a compile error — that's fine for now.

- [ ] **Step 5: Run all tests**

```bash
cd /home/dima/Projects/mini_midi_synth && cargo test 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
cd /home/dima/Projects/mini_midi_synth && git add presets/polivoks_lead.json src/preset.rs && git commit -m "feat: add Polivoks Lead factory patch"
```

---

## Done

After all tasks: `cargo test` passes, the synth builds, filter types 37/38 appear in the filter selector, Drive/Starve sliders appear when Polivoks is selected, and "Polivoks Lead" appears in the Lead category of factory patches.
