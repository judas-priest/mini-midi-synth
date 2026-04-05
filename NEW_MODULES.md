# New DSP Modules

Added in commits `2edf259` / `1643941`. All modules live in `src/synth/`.

---

## Filters (`filter.rs`)

19 new `FilterType` variants (indices 16–34).

### BP24 / Notch24 (16, 17)
Cascaded SVF — two 12dB stages in series.
- **BP24**: two bandpass stages → 24dB/oct bandpass. Narrow, resonant.
- **Notch24**: two notch stages → deep 24dB notch.

### OB-Xd 2-pole (18–21)
Saturating SVF: `tanh()` applied to state variables on every update, giving warm harmonic distortion near self-oscillation.
- `OBXd2LP` (18) — lowpass
- `OBXd2HP` (19) — highpass
- `OBXd2BP` (20) — bandpass
- `OBXd2Notch` (21) — notch

Resonance `k = (2.0 - res * 1.98).max(0.01)` — reaches zero at full resonance (self-oscillates).

### OB-Xd 4-pole (22)
Two cascaded OBXd2 LP stages. 24dB/oct, warmest filter in the set.

### Tripole (23)
Three cascaded 1-pole LP stages with resonance feedback. 18dB/oct slope — character between 12 and 24dB. Odd-order rolloff gives a slightly different phase response.

### Sample & Hold / ZOH (24)
Zero-order hold: samples input at `cutoff_hz`, holds value between samples. Produces stepped/robotic artifacts, rate-controlled by cutoff.

### Cutoff Warp (25–29)
Standard SVF with `fast_tanh(v3)` applied **before** the integrators. Saturates the high-frequency drive, adding harmonics as cutoff increases.
- LP (25), HP (26), BP (27), Notch (28), AP (29)

### Resonance Warp (30–34)
Standard SVF with `fast_tanh(k * v1)` replacing raw resonance feedback. Smooth self-oscillation — resonance saturates instead of going unstable.
- LP (30), HP (31), BP (32), Notch (33), AP (34)

---

## Effects

### `wave_shaper.rs` — WaveShaper
Multi-mode waveshaper with DC blocker.

```
tick(in_l, in_r, drive, mode: u32, bias, mix) -> (f32, f32)
```

| Mode | Name | Character |
|------|------|-----------|
| 0 | Tanh | Smooth soft clip |
| 1 | HardClip | Brick-wall clip |
| 2 | Asymmetric | Tanh + bias offset |
| 3 | SinFold | Sine wavefolder |
| 4 | TriFold | Triangle wavefolder |
| 5 | Digital | Quantize (stepped) |
| 6 | Diode | Half-wave rectifier |
| 7 | Rectify | Full-wave rectifier |

DC blocker at 6 Hz removes offset from asymmetric modes.

---

### `ms_tool.rs` — MsTool
M/S encode → independent gain → stereo rotation → decode.

```
tick(in_l, in_r, mid_gain, side_gain, rotation, mix) -> (f32, f32)
```

- `mid_gain` / `side_gain`: 0..2, unity at 1.0
- `rotation`: ±π/2 rotates the M/S plane
- `mix`: wet/dry blend

---

### `graphic_eq.rs` — GraphicEq
11-band RBJ biquad EQ with lazy coefficient recompute.

```
tick(in_l, in_r, gains: &[f32; 11], output_gain) -> (f32, f32)
```

| Band | Freq | Type |
|------|------|------|
| 0 | 31 Hz | Low shelf |
| 1–9 | 62–8k Hz | Peak (Q=1.41) |
| 10 | 16 kHz | High shelf |

Gains in dB, clamped ±12 dB. Coefficients rebuilt only when gains change.

---

### `conditioner.rs` — Conditioner
3-stage mastering tool: HP filter → M/S width → tanh limiter.

```
tick(in_l, in_r, bass_cut, width, limit_threshold, mix) -> (f32, f32)
```

- `bass_cut`: HP cutoff Hz (0 = bypass)
- `width`: 0=mono, 1=unity, 2=expanded
- `limit_threshold`: 0..1 maps to threshold 0.5..1.0
- Output is tanh soft-clipped at threshold

---

### `exciter.rs` — Exciter
Adds presence/air by distorting high-frequency content and mixing it back in.

```
tick(in_l, in_r, drive, freq, presence, mix) -> (f32, f32)
```

- Two cascaded HP filters isolate content above `freq`
- Isolated band is tanh-distorted by `drive`
- `presence` shelf boosts the mixed-in harmonics

---

### `floaty_delay.rs` — FloatyDelay
Stereo delay with LFO-modulated time (pitch wobble/flutter). Quadrature LFO — L and R modulate 90° out of phase for width.

```
tick(in_l, in_r, time, feedback, wobble, rate, damp, mix) -> (f32, f32)
```

- `time`: 0..1 → 0..2 s delay
- `wobble`: LFO depth (0..1)
- `rate`: LFO frequency Hz
- `damp`: 1-pole LP in feedback path (darker echoes)
- Buffer: 88200 samples (~2s at 44.1 kHz)

---

### `reverb2.rs` — Reverb2
8-line Feedback Delay Network with Hadamard mixing matrix. More diffuse and complex than the plate reverb.

```
tick(in_l, in_r, decay, damping, size, mix) -> (f32, f32)
```

- 8 prime-length delay lines (scaled by sample rate)
- Hadamard butterfly mix (normalized by 1/√8)
- Per-line 1-pole LP for frequency-dependent decay
- T60 feedback: `exp(-7 * delay_time / decay_time)`
- Stereo: even lines → L, odd lines → R

---

### `combulator.rs` — Combulator
3 tuned comb filters with semitone offsets and tone control.

```
tick(in_l, in_r, freq, offset2, offset3, feedback, tone, mix) -> (f32, f32)
```

- Comb 1 at `freq` Hz (center pan)
- Comb 2 at `freq * 2^(offset2/12)` (left pan)
- Comb 3 at `freq * 2^(offset3/12)` (right pan)
- `tone`: 1-pole LP in feedback (0=dark, 1=bright)
- Output clamped ±2.0

---

### `treemonster.rs` — Treemonster
Zero-crossing pitch detector → sine oscillator → ring modulation.

```
tick(in_l, in_r, threshold, shift, ring_mix, mix) -> (f32, f32)
```

- Detects positive-going zero crossings above `threshold`
- Detected pitch shifted by `shift` semitones
- `ring_mix`: 0=dry signal, 1=full ring mod
- Smoother τ ≈ 0.45s (blend coeff 0.05 per ZC event)

---

### `nimbus.rs` — Nimbus
16-grain granular cloud with Hann envelope and constant-power panning.

```
tick(in_l, in_r, position, size, pitch, density, spread, texture, mix) -> (f32, f32)
```

- Records live input into 2s ring buffer
- `position`: read head position in buffer (0=now, 1=oldest)
- `size`: grain duration 0..0.5s
- `pitch`: playback speed (semitones, ±24)
- `density`: grain spawn rate (grains/sec)
- `spread`: random position scatter
- `texture`: crossfade shape (0=Hann, 1=rectangular)
- LCG PRNG for position/pan randomization
- Output normalized by 1/√(active_grains)

---

### `vocoder.rs` — Vocoder
16-band analysis/synthesis vocoder. Modulator envelope drives carrier amplitude per band.

```
tick(car_l, car_r, mod_in, env_follow, gate, mix) -> (f32, f32)
```

- 16 bands log-spaced 100 Hz – 8 kHz
- Independent SVF BPF for modulator and carrier (L/R carrier processed separately)
- Asymmetric envelope follower: attack = `env_follow/4`, release = `env_follow`
- `gate`: minimum modulator energy to pass (noise gate)
- Band gain normalized by 1/√16

---

### `convolution_reverb.rs` — ConvolutionReverb
Direct convolution with a procedurally generated 512-sample IR.

```
tick(in_l, in_r, room_size, damping, pre_delay, mix) -> (f32, f32)
```

- IR generated once at `new()`: LCG noise + exponential decay + early reflections + HP/LP shaping
- `room_size`: number of IR taps used (64–512)
- `damping`: per-tap exponential rolloff — **cached**, not computed per sample
- `pre_delay`: 0..1 → 0..40 ms pre-delay line
- IR rebuilt only when `damping` or `ir_len` changes

---

## Preset Parameters

All effects are controlled via `PresetParams` and load from JSON.

### WaveShaper
`wave_shaper_drive`, `wave_shaper_mode` (0–7), `wave_shaper_bias`, `wave_shaper_mix`

### MS Tool
`ms_mid_gain`, `ms_side_gain`, `ms_rotation`, `ms_mix`

### Graphic EQ
`geq_0`..`geq_10` (dB), `graphic_eq_output`

### Conditioner
`conditioner_bass_cut`, `conditioner_width`, `conditioner_threshold`, `conditioner_mix`

### Exciter
`exciter_drive`, `exciter_freq`, `exciter_presence`, `exciter_mix`

### Floaty Delay
`floaty_time`, `floaty_feedback`, `floaty_wobble`, `floaty_rate`, `floaty_damp`, `floaty_mix`

### Reverb2
`reverb2_decay`, `reverb2_damping`, `reverb2_size`, `reverb2_mix`

### Combulator
`combulator_freq`, `combulator_offset2`, `combulator_offset3`, `combulator_feedback`, `combulator_tone`, `combulator_mix`

### Treemonster
`treemonster_threshold`, `treemonster_shift`, `treemonster_ring_mix`, `treemonster_mix`

### Nimbus
`nimbus_position`, `nimbus_size`, `nimbus_pitch`, `nimbus_density`, `nimbus_spread`, `nimbus_texture`, `nimbus_mix`

### Vocoder
`vocoder_env_follow`, `vocoder_gate`, `vocoder_mix`

### Convolution Reverb
`conv_reverb_room`, `conv_reverb_damping`, `conv_reverb_predelay`, `conv_reverb_mix`

---

## Modulation Sources (new)

| Source | Range | Description |
|--------|-------|-------------|
| `RandomBipolar` | −1..+1 | New random value on each note-on |
| `RandomUnipolar` | 0..1 | Same, unipolar |
| `AltBipolar` | ±1 | Alternates sign each note-on |
| `AltUnipolar` | 0/1 | Alternates 0/1 each note-on |
| `ReleaseVel` | 0..1 | Note-off velocity |
| `PitchBend` | −1..+1 | MIDI pitch bend (normalized) |
| `Cc1`..`Cc4` | 0..1 | Assignable MIDI CCs (default: CC1–4) |

## Modulation Destinations (new)

`Lfo3Rate`, `Lfo4Rate`, `WaveShaperDrive`, `WaveShaperMix`, `RotaryMix`, `EnsembleMix`, `ResonatorMix`, `BonsaiDrive`

---

## Voice Modes (`play_mode`)

| Value | Mode | Behaviour |
|-------|------|-----------|
| 0 | Poly | Normal polyphonic (default) |
| 1 | Mono | Single voice, retriggered; note stack for legato |
| 2 | Mono-ST | Single trigger — retrigger pitch only, envelopes held |
| 3 | Latch | Note-on toggles notes on/off; note-off ignored |
