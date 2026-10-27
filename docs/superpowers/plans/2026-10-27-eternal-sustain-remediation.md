# Eternal Sustain Remediation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate every path where notes ring forever after sustain/preset/part/fader/SF2 operations.

**Architecture:** The engine has two voice owners — DSP `Part.voices` and the rustysynth `Sampler` (channels 0-7 = parts, 9 = drums). rustysynth gates every note-off behind its per-channel hold pedal (`voice.rs:285`: `RELEASE_REQUESTED` does not release while `hold_pedal`), so every "all off" path must lift CC64 first. A second root cause: several patch/part mutation paths and note-off routes never release voices at all.

**Tech Stack:** Rust, existing `SynthEngine` test harness (`tests/end_to_end.rs` pattern), cargo test.

**Verified facts (audit, 3 subagents):**
- rustysynth: `voice.end()` only sets `RELEASE_REQUESTED`; release deferred while `channel.get_hold_pedal()` (vendor/rustysynth/src/voice.rs:285). CC123 → `note_off_all_channel(ch,false)` → pedal-gated. `Synthesizer::note_off_all_channel` exists (synthesizer.rs:269). Program change 0xC0 kills nothing.
- `sustained_notes` flags are secondary; the hold mechanism is *not calling* `note_off`.
- Recently fixed (already in tree, keep): `set_sustain` releases all voices + forwards CC64 to sampler channels 0-7; `LoadPatch` and `SetPartEnabled{false}` clean DSP voices + `Sampler::part_all_notes_off`; F1 GUI toggle.

**Out of scope (do not do here):** mute not applied in the mix (only gates note-ons); seq/part program cache desync in `seq_program_set`; `skip_samples` time distortion.

---

## Test Cases (from the requirement)

Requirement: "Sustain must never stick: after any sustain pedal action, preset/program change, part enable/disable/mute, SF2 mode/program switch, fader move, or panic (AllNotesOff / CC120 / CC123), no note may keep sounding, in both DSP and SF2 engines."

Written BEFORE implementation. Task 1 is executed by a FRESH subagent that receives ONLY this plan document.

1. `pedal_up_releases_held_notes` — pedal down, note on, note off (held by pedal), pedal up → silence.
2. `all_notes_off_releases_pedal_held_notes` — pedal down, note held by pedal, `ControlEvent::AllNotesOff` → silence AND sustain atom reads false.
3. `cc123_panic_releases_pedal_held_notes` — pedal down, note held, MIDI CC123 → silence.
4. `cc120_panic_releases_pedal_held_notes` — same with CC120.
5. `patch_switch_releases_pedal_held_notes` — pedal down, note held, `LoadPatch` with a different patch → silence.
6. `program_change_releases_pedal_held_notes` — pedal down, note held, `MidiEvent::ProgramChange` → silence.
7. `transposed_part_note_off_stops_sound` — `SetPartTranspose{12}`, note on 60, note off 60 → silence.
8. `arp_transposed_note_off_stops_sound` — arpeggiator on + part transpose 12 → no hanging voice after arp stops (peak → silence after gate).
9. `mono_pedal_up_clears_note_stack` — mono mode, pedal down, A on/off (held), pedal up, B on/off → silence after B (stale stack must not resurrect A/B).
10. `part_disable_releases_pedal_held_notes` — pedal down, note held, `SetPartEnabled{false}` → silence.
11. `mute_releases_sounding_notes` — note sounding, `SetPartMute{true}` → silence (SF2 channel can't be tested without a soundfont; DSP only).
12. `midi_learn_rejects_cc64` — `CcMap::from_config` with a saved binding on CC64 must NOT produce a binding at index 64.
13. `performance_part_config_roundtrip` — serde roundtrip of `PartConfig` preserves pan/transpose/mute/vel_min/vel_max.
14. `repeated_cc123_is_idempotent` — CC123 sent twice in a row: silence, no panic.
15. `program_change_with_empty_patch_cache_cleans_voices` — engine without `set_patches` (empty cache): pedal-held note, `ProgramChange` → silence, no panic.

SF2-only behaviors (pedal state across SF2 reload, `part_all_notes_off` on SF2 program switch, MIDI-player mute passing note-offs) cannot be driven in tests without a real .sf2 — they are verified by code review against the audit facts and a manual checklist (Task 12).

## Negative Matrix

- **Missing field:** old performance JSON without pan/transpose/mute/vel → serde defaults (Task 13 already coded; test 13 guards regression).
- **Null:** `keys.synth == None` (no SF2 loaded) → `part_all_notes_off` must be a no-op, not a panic (guard with `if let Some`).
- **Zero/empty:** no patches loaded (`preset_params_cache` empty) → `ProgramChange` handler must still clean voices before bailing.
- **Clock skew:** n/a (no independent clocks involved) — n/a, single audio-thread event ordering.
- **Duplicate/concurrent:** repeated CC123 / repeated pedal-up while already up → must be idempotent, no panic, no double-release artifacts.
- **External-call timeout:** ring-buffer push failure in GUI (`CountingProducer` drops) → engine panic path must not depend on any GUI reply.

---

### Task 1: Regression tests (fresh subagent, plan-only context)

**Files:**
- Create: `tests/sustain_audit.rs`
- Reuse (copy helpers, do not modify): `tests/end_to_end.rs:12-49` (`render_peak`, `engine_with_init_patch`, `settle`)

- [ ] **Step 1: Create `tests/sustain_audit.rs` with the helpers and all 15 tests**

```rust
//! Sustain-stick regression: no note may outlive its pedal/panic/preset-switch.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;

fn render_peak(engine: &mut SynthEngine, blocks: usize) -> f32 {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
    }
    peak
}

fn settle(engine: &mut SynthEngine) {
    let _ = render_peak(engine, 6000);
}

fn engine_with_init_patch() -> SynthEngine {
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();
    let mut engine = SynthEngine::new(SR);
    engine.set_patches(patches);
    let _ = engine.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = engine.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = engine.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    engine
}

fn second_patch() -> preset::Patch {
    let patches = preset::load_all_patches();
    patches
        .iter()
        .find(|p| !p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone()
}

fn init_patch_mut() -> preset::Patch {
    preset::load_all_patches()
        .into_iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists")
}

fn pedal_down(e: &mut SynthEngine) {
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 });
}
fn pedal_up(e: &mut SynthEngine) {
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 0 });
}

#[test]
fn pedal_up_releases_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "pedal did not hold the note");
    pedal_up(&mut e);
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "pedal-up left sound ringing (peak {after})");
}

#[test]
fn all_notes_off_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held by pedal");
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "AllNotesOff left pedal note ringing (peak {after})");
    assert_eq!(e.sustain_atom().load(std::sync::atomic::Ordering::Relaxed), false,
        "AllNotesOff left sustain atom ON");
}

#[test]
fn cc123_panic_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC123 left pedal note ringing (peak {after})");
}

#[test]
fn cc120_panic_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC120 left pedal note ringing (peak {after})");
}

#[test]
fn patch_switch_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "patch switch left pedal note ringing (peak {after})");
}

#[test]
fn program_change_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 1 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "ProgramChange left pedal note ringing (peak {after})");
}

#[test]
fn transposed_part_note_off_stops_sound() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to stop");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "transposed note-off did not match note-on (peak {after})");
}

#[test]
fn arp_transposed_note_off_stops_sound() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    // Patch params live in a BTreeMap<String, f32>; PatchParams::from_map
    // reads exactly these keys.
    let mut patch = init_patch_mut();
    patch.params.insert("arp_enabled".to_string(), 1.0);
    patch.params.insert("arp_rate".to_string(), 0.0); // 1/4 note; first step fires at tick_counter==0
    patch.params.insert("arp_gate".to_string(), 0.5);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "arp produced no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "arp left a voice hanging (peak {after})");
}

#[test]
fn mono_pedal_up_clears_note_stack() {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("play_mode".to_string(), 1.0); // mono
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    pedal_up(&mut e);
    settle(&mut e);
    // Stale mono stack would resurrect A (60) when B (62) is pressed/released
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "stale mono stack resurrected a note (peak {after})");
}

#[test]
fn part_disable_releases_pedal_held_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: false });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "part disable left pedal note ringing (peak {after})");
}

#[test]
fn mute_releases_sounding_notes() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to mute");
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "mute left notes sounding (peak {after})");
}

#[test]
fn midi_learn_rejects_cc64() {
    use mini_midi_synth::cc_map::{CcBindingRaw, CcMap, CcMapRaw};
    use std::collections::BTreeMap;
    let mut bindings = BTreeMap::new();
    bindings.insert(64u8, CcBindingRaw {
        param_key: "master_volume".to_string(),
        min_val: 0.0, max_val: 1.0, logarithmic: false,
        scope: mini_midi_synth::cc_map::ParamScope::Global,
    });
    let config = mini_midi_synth::config::Config::default();
    let mut config = config;
    config.cc_map = Some(CcMapRaw { bindings });
    let map = CcMap::from_config(&config);
    assert!(map.bindings[64].is_none(), "CC64 must never be a parameter binding");
}

#[test]
fn repeated_cc123_is_idempotent() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    pedal_up(&mut e); // idempotent too: already up via the handler
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "repeated CC123 left sound (peak {after})");
}

#[test]
fn program_change_with_empty_patch_cache_cleans_voices() {
    let mut e = SynthEngine::new(SR); // no set_patches: preset_params_cache is empty
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 200 }); // out of range
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "empty-cache ProgramChange left sound (peak {after})");
}

#[test]
fn performance_part_config_roundtrip() {
    use mini_midi_synth::preset::PartConfig;
    let orig = PartConfig {
        patch_name: "init".to_string(),
        enabled: true,
        volume: 0.75,
        key_low: 0,
        key_high: 127,
        param_overrides: Default::default(),
        sf2_mode: false,
        sf2_program: 0,
        pan: -0.5,
        transpose: -12,
        mute: true,
        vel_min: 40,
        vel_max: 100,
    };
    let json = serde_json::to_string(&orig).expect("serialize");
    let back: PartConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.pan, -0.5);
    assert_eq!(back.transpose, -12);
    assert!(back.mute);
    assert_eq!(back.vel_min, 40);
    assert_eq!(back.vel_max, 100);
    // Old file without the new fields must deserialize to neutral defaults
    let old_json = r#"{"patch_name":"init","enabled":true,"volume":0.8,"key_low":0,"key_high":127,"param_overrides":{},"sf2_mode":false,"sf2_program":0}"#;
    let legacy: PartConfig = serde_json::from_str(old_json).expect("legacy deserialize");
    assert_eq!(legacy.pan, 0.0);
    assert_eq!(legacy.transpose, 0);
    assert!(!legacy.mute);
    assert_eq!(legacy.vel_min, 1);
    assert_eq!(legacy.vel_max, 127);
}
```

- [ ] **Step 2: Run and record failures**

Run: `cargo test --test sustain_audit 2>&1 | tail -30`
Expected pre-fix state — do NOT "fix" tests that already pass:
- PASS (the tree already covers them): `pedal_up_releases_held_notes`, `all_notes_off_releases_pedal_held_notes`, `cc120_panic_releases_pedal_held_notes`, `cc123_panic_releases_pedal_held_notes` (the DSP panic handler already force-releases voices; the pedal-gated hang is rustysynth-only), `patch_switch_releases_pedal_held_notes`, `part_disable_releases_pedal_held_notes`, `mute_releases_sounding_notes` (DSP mute already releases voices; Task 7 adds the SF2 drain), `mono_pedal_up_clears_note_stack` (mono note_off bypasses the pedal-hold branch, so nothing is actually pedal-held), `performance_part_config_roundtrip`.
- FAIL (real bugs, fixed by Tasks 2-9): `program_change_releases_pedal_held_notes`, `transposed_part_note_off_stops_sound`, `arp_transposed_note_off_stops_sound`, `program_change_with_empty_patch_cache_cleans_voices`, `midi_learn_rejects_cc64`.
- PASS (idempotency already holds): `repeated_cc123_is_idempotent`.
Note: `performance_part_config_roundtrip` requires the PartConfig fields that already exist in the uncommitted tree (src/preset.rs:708-736). If it fails to compile, verify those fields first.

- [ ] **Step 3: Commit**

```bash
git add tests/sustain_audit.rs
git commit -m "test: sustain-stick regression matrix (15 cases)"
```

---

### Task 2: `Sampler::all_notes_off` lifts the hold pedal; pedal state survives SF2 rebuilds

**Files:**
- Modify: `src/synth/sampler.rs` — struct `SamplerEngine` (line ~188): add field; `sustain_pedal` (~353); `load_keys_soundfont` (~225), `set_sample_rate` (~287), `set_block_size` (~263); `all_notes_off` (~424-429)

- [ ] **Step 1: Add a pedal-state field to `SamplerEngine`**

In the `SamplerEngine` struct add:

```rust
    /// Last sustain state, re-applied after SF2 rebuilds (fresh rustysynth
    /// channels start with hold_pedal=false).
    sustain: bool,
```

Initialize with `sustain: false` in the constructor.

- [ ] **Step 2: `sustain_pedal` remembers; `apply_sustain` re-applies**

```rust
    pub fn sustain_pedal(&mut self, on: bool) {
        self.sustain = on;
        let val = if on { 127 } else { 0 };
        for ch in 0..8usize {
            self.keys.midi(ch as i32, 0xB0, 64, val);
        }
    }

    /// Re-send CC64 to the part channels after a rustysynth rebuild.
    fn apply_sustain(&mut self) {
        let on = self.sustain;
        let val = if on { 127 } else { 0 };
        for ch in 0..8usize {
            self.keys.midi(ch as i32, 0xB0, 64, val);
        }
    }
```

- [ ] **Step 3: Call `apply_sustain()` at the end of `load_keys_soundfont`, `set_sample_rate`, `set_block_size`**

In each of the three methods, after the `rebuild` call and before the `return`, add:

```rust
        self.apply_sustain();
```

(`set_sample_rate` routes through `load_keys_soundfont`, so it fires twice there — harmless, idempotent.)

- [ ] **Step 4: `all_notes_off` lifts the pedal first (rustysynth gates CC123 on it)**

```rust
    pub fn all_notes_off(&mut self) {
        // rustysynth defers note-offs while a channel hold pedal is down
        // (voice.rs RELEASE_REQUESTED); lift CC64 everywhere before CC123.
        self.sustain = false;
        for ch in 0..16 {
            self.keys.midi(ch, 0xB0, 64, 0);
            self.keys.midi(ch, 0xB0, 123, 0);
            self.drums.midi(ch, 0xB0, 123, 0);
        }
    }
```

- [ ] **Step 5: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -20`
Expected: no new failures (the panic-path DSP tests already pass; Task 3 makes the panic path lift the sampler pedal for SF2, which tests can't see). Run `cargo test --features rt-assert --test rt_safety` — must PASS.

- [ ] **Step 6: Commit**

```bash
git add src/synth/sampler.rs
git commit -m "fix: all_notes_off lifts rustysynth hold pedal; pedal survives SF2 rebuild"
```

---

### Task 3: Panic paths (CC120/CC123 and `AllNotesOff`) route through `set_sustain(0)`

**Files:**
- Modify: `src/synth/mod.rs` CC120/123 block (~1652-1675), `ControlEvent::AllNotesOff` (~1966-1988)

- [ ] **Step 1: CC120/123 handler — clear sustain state first**

The block currently loops parts clearing voices/flags directly. Replace its beginning so it reads:

```rust
                if cc == 120 || cc == 123 {
                    // Lift the sustain state everywhere first: rustysynth
                    // gates CC123 on the channel hold pedal, and the GUI
                    // sustain atom must not stay stuck ON.
                    self.set_sustain(0);
                    for part in &mut self.parts {
                        for v in &mut part.voices {
                            if v.active { v.note_off(); }
                        }
                        part.note_stack.clear();
                        part.latch_held.clear();
                        part.sustain_pedal = 0.0;
                        part.sustained_notes = [false; 128];
                    }
```

(the rest of the block — `self.sampler.all_notes_off();`, arpeggiator flush, `return;` — stays unchanged)

- [ ] **Step 2: `ControlEvent::AllNotesOff` — same treatment**

At the top of the `AllNotesOff` arm, replace

```rust
                self.sustain.store(false, std::sync::atomic::Ordering::Relaxed);
```

with

```rust
                self.set_sustain(0);
```

(`set_sustain(0)` stores the atom, sends CC64=0 to the sampler and releases DSP voices; the existing loop that follows re-kills voices idempotently.)

- [ ] **Step 3: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -20`
Expected: `cc120_panic…`, `cc123_panic…`, `all_notes_off_releases_pedal_held_notes` PASS (they pass on DSP already; this task adds the sampler pedal-lift the tests cannot observe — verified by review against voice.rs:285).
Run: `cargo test --features rt-assert --test rt_safety` — PASS.

- [ ] **Step 4: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: panic paths (CC120/123, AllNotesOff) clear sustain and lift SF2 hold pedal"
```

---

### Task 4: Note-off must use the transposed note (DSP parts, 3 sites)

**Files:**
- Modify: `src/synth/mod.rs` NoteOn vel==0 branch (~1539-1547), NoteOff branch (~1567-1586), arpeggiator arp_off branch (~2460-2468)

- [ ] **Step 1: Fix the vel==0 NoteOn branch**

Replace

```rust
                } else if velocity == 0 {
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        if !part.enabled { continue; }
                        if part.sf2_mode {
                            self.sampler.note_off(i, note);
                        } else {
                            part.note_off(note);
                        }
                    }
```

with

```rust
                } else if velocity == 0 {
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        if !part.enabled { continue; }
                        let transposed = (note as i16 + part.transpose as i16).clamp(0, 127) as u8;
                        if part.sf2_mode {
                            self.sampler.note_off(i, transposed);
                        } else {
                            part.note_off(transposed);
                        }
                    }
```

- [ ] **Step 2: Fix the NoteOff branch**

Replace

```rust
                } else {
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        if part.sf2_mode {
                            self.sampler.note_off(i, note);
                        } else {
                            part.note_off(note);
                        }
                    }
                    let ap = self.active_part.load(std::sync::atomic::Ordering::Relaxed);
                    self.looper.record_event(note, 0, ap);
                }
```

with

```rust
                } else {
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        let transposed = (note as i16 + part.transpose as i16).clamp(0, 127) as u8;
                        if part.sf2_mode {
                            self.sampler.note_off(i, transposed);
                        } else {
                            part.note_off(transposed);
                        }
                    }
                    let ap = self.active_part.load(std::sync::atomic::Ordering::Relaxed);
                    self.looper.record_event(note, 0, ap);
                }
```

(Note: this loop no longer skips disabled parts — a part disabled mid-note must still receive the off; `part.note_off` on inactive voices is a no-op. `sf2_mode` parts keep getting the off for the same reason.)

- [ ] **Step 3: Fix the arpeggiator off branch**

Replace

```rust
                if let Some(off_note) = arp_off {
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        if !part.enabled { continue; }
                        if part.sf2_mode { self.sampler.note_off(i, off_note); }
                        else { part.note_off(off_note); }
                    }
```

with

```rust
                if let Some(off_note) = arp_off {
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        if !part.enabled { continue; }
                        let transposed = (off_note as i16 + part.transpose as i16).clamp(0, 127) as u8;
                        if part.sf2_mode { self.sampler.note_off(i, transposed); }
                        else { part.note_off(transposed); }
                    }
```

- [ ] **Step 3b: Flush the arp gate-off when the last held key is released (both note-off routes)**

`Arpeggiator::note_off` resets `pattern_len = 0` when the last held note goes (src/synth/arpeggiator.rs:101-105), so `tick()` never dispatches the pending `current_note` gate-off — the sounding arp voice hangs forever. There are TWO live note-off routes that must flush: `MidiEvent::NoteOn { velocity: 0 }` and `MidiEvent::NoteOff`.

Add an accessor to `Arpeggiator` (src/synth/arpeggiator.rs, near `flush_current_note` ~152):

```rust
    pub fn has_held_notes(&self) -> bool {
        self.held_count > 0
    }
```

Add a helper to `SynthEngine` (near `set_sustain`):

```rust
    /// Last held arp key released: the arpeggiator resets pattern_len, so
    /// tick() will never dispatch the pending gate-off — the sounding voice
    /// would hang forever. Dispatch the off here (same routing as tick_block).
    fn flush_arp_note_off(&mut self) {
        if self.arpeggiator.has_held_notes() {
            return; // tick() still owns gate-offs
        }
        if let Some(off) = self.arpeggiator.flush_current_note() {
            for (i, part) in self.parts.iter_mut().enumerate() {
                if !part.enabled { continue; }
                let transposed = (off as i16 + part.transpose as i16).clamp(0, 127) as u8;
                if part.sf2_mode { self.sampler.note_off(i, transposed); }
                else { part.note_off(transposed); }
            }
            let ap = self.active_part.load(std::sync::atomic::Ordering::Relaxed);
            self.looper.record_event(off, 0, ap);
        }
    }
```

In `handle_event`, the live `NoteOn` arp routing arm (~1532-1539), replace

```rust
                } else if self.arpeggiator.enabled {
                    // Route through arpeggiator — it generates notes in tick_block
                    if velocity == 0 {
                        self.arpeggiator.note_off(note);
                    } else {
                        self.arpeggiator.note_on(note, velocity);
                    }
                } else if velocity == 0 {
```

with

```rust
                } else if self.arpeggiator.enabled {
                    // Route through arpeggiator — it generates notes in tick_block
                    if velocity == 0 {
                        self.arpeggiator.note_off(note);
                        self.flush_arp_note_off();
                    } else {
                        self.arpeggiator.note_on(note, velocity);
                    }
                } else if velocity == 0 {
```

and the `MidiEvent::NoteOff` arp arm (~1574-1575), replace

```rust
                } else if self.arpeggiator.enabled {
                    self.arpeggiator.note_off(note);
                } else {
```

with

```rust
                } else if self.arpeggiator.enabled {
                    self.arpeggiator.note_off(note);
                    self.flush_arp_note_off();
                } else {
```

(No double-dispatch: when held notes remain, the helper returns early and `tick()` keeps owning gate-offs; when none remain, `pattern_len` is already 0 and `tick()` early-returns.)

- [ ] **Step 4: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -20`
Expected: `transposed_part_note_off_stops_sound`, `arp_transposed_note_off_stops_sound` PASS.
Run: `cargo test 2>&1 | grep -E 'FAILED|failures:' | head -5` — no failures anywhere.

- [ ] **Step 5: Commit**

```bash
git add src/synth/mod.rs src/synth/arpeggiator.rs
git commit -m "fix: note-offs use the transposed note; arp flushes its gate-off when the last key is released"
```

---

### Task 5: Hardware `ProgramChange` cleans up like `LoadPatch`

**Files:**
- Modify: `src/synth/mod.rs` `MidiEvent::ProgramChange` (~1618-1637)

- [ ] **Step 1: Add cleanup before state swap (hoisted above the cache lookup — the empty-cache case must still clean)**

Replace the whole arm so it reads:

```rust
            MidiEvent::ProgramChange { program, .. } => {
                // Same hang-prevention as LoadPatch: release voices the old
                // patch held (pedal holds, mono/latch holds) BEFORE anything
                // else — including when the program is out of range and the
                // cache lookup below bails.
                self.sampler.part_all_notes_off(0);
                if let Some(l) = self.parts.get_mut(0) {
                    for voice in &mut l.voices {
                        if voice.active { voice.note_off(); }
                    }
                    l.sustained_notes = [false; 128];
                    l.note_stack.clear();
                    l.latch_held.clear();
                }
                // In SF2 mode, change the SF2 program; in synth mode, change the synth patch
                if self.parts.first().map(|l| l.sf2_mode).unwrap_or(false) {
                    self.sampler.set_part_program(0, program, 0);
                } else if let Some(&params) = self.preset_params_cache.get(program as usize) {
                    if let Some(l) = self.parts.get_mut(0) {
                        l.params = params;
                        l.min_note = params.min_note as u8;
                        l.max_note = params.max_note as u8;
                        if let Some(patch) = self.patches.get(program as usize) {
                            l.mod_matrix.load_from_params(&patch.params);
                            l.pitch_seq.load_from_params(&patch.params);
                        }
                        l.update_routing_cache();
                    }
                    self.reset_preset_pickups();
                }
```

(The `program_change` atom + feedback tail of the arm stays unchanged.)

- [ ] **Step 2: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -20`
Expected: `program_change_releases_pedal_held_notes` PASS.
Run: `cargo test 2>&1 | grep -E 'FAILED|failures:' | head -5` — none.

- [ ] **Step 3: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: hardware ProgramChange releases voices and syncs key range like LoadPatch"
```

---

### Task 6: Pedal-up clears mono/latch stacks

**Files:**
- Modify: `src/synth/mod.rs` `set_sustain` (~1353-1369)

- [ ] **Step 1: Extend the pedal-up branch**

Replace `set_sustain` with:

```rust
    fn set_sustain(&mut self, value: u8) {
        let on = value >= 64;
        self.sustain.store(on, std::sync::atomic::Ordering::Relaxed);
        // SF2 voices: rustysynth applies hold-pedal per channel itself
        self.sampler.sustain_pedal(on);
        for part in &mut self.parts {
            part.sustain_pedal = if on { 1.0 } else { 0.0 };
            if !on {
                for voice in &mut part.voices {
                    if voice.active && !voice.is_releasing() {
                        voice.note_off();
                    }
                }
                part.sustained_notes = [false; 128];
                // All pedal-held voices are gone; stale mono/latch entries
                // would resurrect them on the next key press (mono retrigger).
                part.note_stack.clear();
                part.latch_held.clear();
            }
        }
    }
```

- [ ] **Step 2: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -20`
Expected: `mono_pedal_up_clears_note_stack` PASS; no regressions (`cargo test 2>&1 | grep -E 'FAILED|failures:' | head -5` — none).

- [ ] **Step 3: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: pedal-up clears mono note stack and latch holds"
```

---

### Task 7: Mute, SF2-mode and SF2-program switches release voices

**Files:**
- Modify: `src/synth/mod.rs` `ControlEvent::SetPartMute` (~1824), `ControlEvent::SetPartSf2Mode` (~2012), `ControlEvent::SetPartSf2Program` (~2018)

- [ ] **Step 1: `SetPartMute`**

Wrap the existing mute flag assignment:

```rust
            ControlEvent::SetPartMute { part, mute } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.mute = mute;
                    if mute {
                        // A muted part must not keep sounding; SF2 voices live
                        // in the sampler, not in l.voices.
                        self.sampler.part_all_notes_off(part);
                        for voice in &mut l.voices {
                            if voice.active { voice.note_off(); }
                        }
                        l.sustained_notes = [false; 128];
                        l.note_stack.clear();
                        l.latch_held.clear();
                    }
                }
            }
```

(Keep whatever the arm currently does with `l.mute`; only ensure the `mute` branch adds the release block.)

- [ ] **Step 2: `SetPartSf2Mode`**

```rust
            ControlEvent::SetPartSf2Mode { part, enabled } => {
                // Engine switch: both voice owners must be drained
                self.sampler.part_all_notes_off(part);
                if let Some(l) = self.parts.get_mut(part) {
                    for voice in &mut l.voices {
                        if voice.active { voice.note_off(); }
                    }
                    l.sustained_notes = [false; 128];
                    l.note_stack.clear();
                    l.latch_held.clear();
                    l.sf2_mode = enabled;
                }
                self.sampler.set_part_mode(part, enabled);
            }
```

- [ ] **Step 3: `SetPartSf2Program`**

```rust
            ControlEvent::SetPartSf2Program { part, program, bank } => {
                // 0xC0 kills nothing in rustysynth: drain the channel first
                self.sampler.part_all_notes_off(part);
                self.sampler.set_part_program(part, program, bank);
            }
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -20` — `mute_releases_sounding_notes` PASS, no regressions.
Run: `cargo test --features rt-assert --test rt_safety 2>&1 | grep 'test result'` — PASS.

- [ ] **Step 5: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: mute and SF2 mode/program switches release voices"
```

---

### Task 8: MIDI player: mute passes note-offs, load drains voices, note-offs follow the note-on route

**Files:**
- Modify: `src/synth/midi_player.rs:204-208` (`is_muted` filter), `src/synth/mod.rs` `ControlEvent::MidiSeqLoad` (~2058-2072) and the seq event dispatch (~2415-2452)

- [ ] **Step 1: Mute must not swallow note-offs**

In `midi_player.rs` the drain loop pushes `(ch, data)` only when `!self.is_muted(ch)`. Change it to always pass `NoteOff` through:

```rust
                d => {
                    // Note-offs must never be muted: a muted track's sounding
                    // voice would otherwise hang forever.
                    let is_note_off = matches!(d, SeqEventData::NoteOff { .. });
                    if is_note_off || !self.is_muted(ch) {
                        out.push((ch, d.clone()));
                    }
                }
```

- [ ] **Step 2: `MidiSeqLoad` drains the old file's notes**

At the top of the `MidiSeqLoad` arm (before `self.midi_player.load(data)`), add:

```rust
                // The old file's sounding notes will never see their note-offs
                // from the new event stream.
                self.sampler.all_notes_off();
                for part in &mut self.parts {
                    for voice in &mut part.voices {
                        if voice.active { voice.note_off(); }
                    }
                }
```

- [ ] **Step 3: Note-off routed by the instrument that took the note-on**

Add a field to `SynthEngine` (near `midi_seq_buf`):

```rust
    /// Per MIDI-file channel: 0 = DSP layer 0 took the last note-on,
    /// 1 = SF2 took it, 2 = drums. Note-offs must follow the note-on's
    /// route, even if the track instrument changed mid-note.
    seq_route: [u8; 16],
```

Initialize `seq_route: [0u8; 16]` in `SynthEngine::new`.

In the seq dispatch in `tick_block` (the `for i in 0..self.midi_seq_buf.len()` loop, mod.rs ~2406-2452): keep the existing `NoteOn` arm exactly as it is, and add ONE line to each branch so the route is recorded:

```rust
                    midi_player::SeqEventData::NoteOn { note, velocity } => match instr {
                        midi_player::TrackInstrument::Drums => {
                            self.seq_route[ch as usize] = 2;               // add
                            let sf2 = self.drum_engine.sf2_mode;
                            if sf2 {
                                self.sampler.drum_note_on(note, velocity);
                            } else {
                                self.drum_engine.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::DspLayer0 => {
                            self.seq_route[ch as usize] = 0;               // add
                            if let Some(part) = self.parts.get_mut(0) {
                                part.note_on(note, velocity);
                            }
                        }
                        midi_player::TrackInstrument::Sf2 { program } => {
                            self.seq_route[ch as usize] = 1;               // add
                            self.sampler.seq_program_set(ch, program);     // keep!
                            self.sampler.seq_note_on(ch, note, velocity);
                        }
                    },
```

Replace the `NoteOff` arm (mod.rs:2430-2443) with dispatch by the recorded route; only route 2 (drums) falls back to the live `instr`, because drum route depends on `sf2_mode` at off-time:

```rust
                    midi_player::SeqEventData::NoteOff { note } => match self.seq_route[ch as usize] {
                        0 => {
                            if let Some(part) = self.parts.get_mut(0) {
                                part.note_off(note);
                            }
                        }
                        1 => self.sampler.seq_note_off(ch, note),
                        _ => match instr {
                            midi_player::TrackInstrument::Drums => {
                                let sf2 = self.drum_engine.sf2_mode;
                                if sf2 { self.sampler.drum_note_off(note); }
                            }
                            _ => {}
                        },
                    },
```

- [ ] **Step 3b: Reset `seq_route` in `MidiSeqLoad`**

Inside the `MidiSeqLoad` arm (after the Step 2 drain block), add:

```rust
                self.seq_route = [0u8; 16];
```

- [ ] **Step 4: Run tests**

Run: `cargo test 2>&1 | grep -E 'FAILED|failures:' | head -5` — none. Run `cargo clippy --all-targets -- -D warnings 2>&1 | tail -3` — clean.

- [ ] **Step 5: Commit**

```bash
git add src/synth/mod.rs src/synth/midi_player.rs
git commit -m "fix: MIDI player never swallows note-offs; note-offs follow note-on routing"
```

---

### Task 9: MIDI Learn can never bind CC64

**Files:**
- Modify: `src/cc_map.rs` `from_config` (~160-181), `src/synth/mod.rs` binding application (~1695), `src/gui/settings.rs` learned-CC application (~350-361)

- [ ] **Step 1: `from_config` skips index 64**

Inside the `for (cc_str, raw_binding) in &raw.bindings` loop, before writing the binding:

```rust
            if cc == 64 {
                // CC64 is the sustain pedal, never a parameter binding
                continue;
            }
```

(This makes the Task 1 test `midi_learn_rejects_cc64` pass.)

- [ ] **Step 2: Engine-side guard**

Change

```rust
                if let Some(binding) = self.cc_map.bindings[cc as usize] {
```

to

```rust
                // Defense in depth: even a hand-edited config binding on CC64
                // must not steal the sustain pedal.
                let binding = if cc == 64 { None } else { self.cc_map.bindings[cc as usize] };
                if let Some(binding) = binding {
```

- [ ] **Step 3: GUI MIDI Learn rejects CC64**

In `drain_feedback` (settings.rs), replace

```rust
        if let Some((cc, target)) = learned_cc {
```

with

```rust
        if let Some((64, _)) = learned_cc {
            self.show_toast("CC64 is the sustain pedal — not assignable".to_string(), ToastKind::Error);
        }
        let learned_cc = learned_cc.filter(|&(cc, _)| cc != 64);
        if let Some((cc, target)) = learned_cc {
```

(The existing body of the `if let Some((cc, target))` block — binding construction and `save_cc_map()` — stays unchanged.)

- [ ] **Step 4: Run tests**

Run: `cargo test --test sustain_audit 2>&1 | tail -5` — `midi_learn_rejects_cc64` PASS.
Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -3` — clean.

- [ ] **Step 5: Commit**

```bash
git add src/cc_map.rs src/synth/mod.rs src/gui/settings.rs
git commit -m "fix: CC64 can never be bound as a parameter (MIDI Learn, config, engine)"
```

---

### Task 10: Pad-performance loader disables absent parts

**Files:**
- Modify: `src/gui/mod.rs` `load_performance_by_name` (~1518-1553)

- [ ] **Step 1: Disable parts the performance does not define**

After the `for (i, part) in perf.parts.iter().enumerate()` loop inside the load branch (the function ends with a bare `true`, there is no toast), add:

```rust
                    // Parts absent from the performance must not keep playing
                    // the previous patch with their old voices.
                    for i in perf.parts.len()..self.parts.len() {
                        if self.parts[i].enabled {
                            self.parts[i].enabled = false;
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
                        }
                    }
```

- [ ] **Step 2: Verify compile and commit**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -3` — clean.

```bash
git add src/gui/mod.rs
git commit -m "fix: pad-performance load disables parts absent from the performance"
```

---

### Task 11: Full gate

- [ ] **Step 1: Run everything**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo test 2>&1 | grep -E 'test result'
cargo test --features rt-assert --test rt_safety 2>&1 | grep 'test result'
```

Expected: clippy clean; all suites `ok`; rt_safety `ok` (all new code is allocation-free voice/flag manipulation).

Do NOT run `cargo fmt` on the whole tree (project rule).

- [ ] **Step 2: Commit any leftovers**

```bash
git status --short
git add -A src tests
git commit -m "fix: eternal-sustain remediation complete"
```

---

### Task 12: Manual SF2 verification checklist (human in the loop)

Cannot be automated without a soundfont fixture. Steps for the operator:

1. Load keys SF2, set a part to SF2 mode. Hold F1 (sustain ON), play a note, release key (note held), click F1 OFF → note must release.
2. Pedal held + SF2 note → press GUI panic / send CC123 from keyboard → silence.
3. While a long SF2 note sounds, switch SF2 program → old voice must stop.
4. Toggle part SF2↔DSP while a note sounds → no hang in either engine.
5. Start MIDI file playback on SF2 tracks, press Stop → silence. Mute a track mid-note → note eventually releases.
6. Load a different MIDI file while playing → no stuck note.
7. Performance save/load: set Tr/Pan/Vel on parts, save perf, fiddle, reload → values restored (Tr now persisted).

---

## Self-Review Notes

- Spec coverage: all 13 audit findings map to Tasks 2-10; findings explicitly out of scope are listed in the header.
- Type consistency: `SamplerEngine.sustain` (bool field), `SynthEngine.seq_route: [u8; 16]`, `PartConfig.pan/transpose/mute/vel_min/vel_max` — used consistently.
- Tests come from the requirement section; negative matrix present; matrix rows 3/5/6 are covered by tests 14/15 and the idempotency inherent in voice.note_off.
- Review round 1 (subagent): 2 Critical + 5 Important fixed inline — test params via BTreeMap keys; drums route preserved in seq NoteOff; seq_program_set kept; SamplerEngine name/anchors; ProgramChange cleanup hoisted above cache lookup; pre-fix expected outcomes corrected; MIDI Learn step single-form.
