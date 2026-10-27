# SF2 Release Scale, Pad Transport, Layer Switching Fix — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Three features: (1) optional custom sustain-release length for both DSP and SF2 engines, off by default; (2) transport control (play/pause/record/undo) from pad-bank-2 notes on MIDI channel 10; (3) root-cause fix for SF2 parts going silent after performance/part switching.

**Architecture:**
- Release: DSP gets an envelope release-time override applied on pedal-up note-offs; SF2 gets a vendor patch to rustysynth adding `Synthesizer::set_release_time_scale(f32)` (GM2's CC72 semantics, global scale). GUI exposes both, default = "default" (no override / scale 1.0).
- Transport: `MidiEvent::NoteOn` on channel 9 (MIDI channel 10) with notes 82 (A#5, play/pause), 81 (A5, pause), 83 (B5, record), 64 (E4, undo) route to the drum step sequencer transport instead of drum triggers. All four notes are outside the drum-slot range 36–51, so no trigger conflict. TARGET: drum sequencer (user's "программный секвенсор"); if the looper is meant instead, only the 4 dispatch lines change.
- Layer fix: unknown root cause — Task 1 is a diagnosis task (engine test with the real SC-55.sf2 + targeted logging), Tasks 2-3 fix whatever it finds.

**Tech Stack:** Rust, vendor rustysynth patch (project rule: vendor edits stay), cargo test, real SoundFont at `/home/dima/.local/share/mini_midi_synth/sf2/SC-55.sf2` for local engine tests.

**Research facts:**
- GM2 assigns CC72 to Release Time (Wikipedia, General MIDI Level 2). rustysynth implements only CC 0/1/7/11/64/65/66/67/120/123 subset (synthesizer.rs:150-165) — no sound controllers.
- rustysynth release seconds come from `region_ex.rs` (`get_release_volume_envelope` → timecents) consumed in `voice.rs` `start()`; a single multiplier at that point scales every release.
- Envelope (DSP) release stage: `Envelope::note_off` starts Release from current output (`envelope.rs:154-158`); adding a duration-override variant is a small change.
- Pad-transport notes 81/82/83/64 all outside 36–51 drum-slot range (drum.rs `DRUM_NOTE_BASE=36`, 16 slots) — no trigger conflict.

---

## Test Cases (from the requirement)

Written BEFORE implementation. Task dispatches use a fresh subagent per task receiving ONLY the task text.

1. `pedal_release_default_unchanged` — with the feature OFF (default), pedal-up release length equals the patch amp_release (existing test `pedal_up_releases_held_notes` must keep passing untouched).
2. `dsp_sustain_release_override` — feature ON with 0.05 s: pedal-up held note tail at ~100 ms must be far below the patch's natural tail (0.7·exp(-100/300) ≈ 0.5), threshold 0.15.
3. `key_release_uses_patch_release` — feature ON: note released by KEY (pedal up) still fades at the patch amp_release, NOT the override.
4. `sf2_release_scale_applied` — engine test with SC-55.sf2: release scale 0.05 → SF2 voice decays to silence visibly faster than scale 1.0 (peak comparison over fixed windows).
5. `sf2_release_scale_default_is_identity` — scale 1.0 (default) must sound identical to pre-patch behavior (peak ratio ≈ 1).
6. `pad_transport_play_record` — NoteOn ch9 note 83 starts drum-sequencer recording (rec atom 1); note 82 starts playback (play atom 1).
7. `pad_transport_pause_undo` — note 81 stops playback (play atom 0); note 64 undoes last record (verify via pattern content or undo event effect).
8. `pad_transport_notes_do_not_trigger_drums` — notes 81/82/83/64 on ch9 produce no drum audio (peak < SILENCE with drum kit playing).
9. `layer_switch_sf2_parts_speak` — regression for Task 3 diagnosis: load a two-SF2-part split (like halloween.json), play upper keys → part 4 voice audible (this test is written in Task 1 and MUST fail before the fix).

## Negative Matrix

- **Missing/null:** no SF2 loaded → release scale is a stored no-op; transport notes with no drum engine → still update atoms, no panic.
- **Zero/empty:** release scale 0 → clamped to minimum (0.02, near-instant); override seconds 0 → treated as "use default" only via the OFF switch, never silently.
- **Duplicate/concurrent:** transport note repeated (pad held, repeat fires) → idempotent (record stays recording, play stays playing).
- **Clock skew:** n/a — single-threaded event ordering (strike out).
- **External-call timeout:** n/a — no external calls (strike out).

---

### Task 1: Diagnose silent SF2 layer after performance load (research + failing test)

**Files:**
- Create: `tests/layer_switch_debug.rs` (temporary diagnosis test; may become the permanent regression test)
- Read: `src/gui/mod.rs:1521-1560` (`load_performance_by_name` event order), `src/synth/mod.rs` handlers (`LoadPatch`, `SetPartEnabled`, `SetPartSf2Mode`, `SetPartSf2Program`, `SetPartRange`, `SetPartVolume`, `MidiEvent::NoteOn` routing), `src/synth/sampler.rs` (`set_part_program`, `part_all_notes_off`, `load_keys_soundfont` restore loop, channel map)

**Symptom:** performance `halloween.json` (part 0 = Grand Piano SF2 keys 0-59 transpose -12; part 4 = Warm Upright Piano SF2 keys 60-127 transpose +12, both program 0) — after pad load the upper part reportedly stays silent.

**Hypotheses to check in order (each with concrete evidence):**
1. **Event-order clobber:** `send_edited_params(i)` (→ LoadPatch, sets `l.min_note/max_note` from patch params) runs BEFORE `send_part_range(i)` — ring order should preserve the range. The engine repro below does NOT cover this (it omits LoadPatch) — if the repro passes clean, add `log::info!` tracing in `load_performance_by_name` and the NoteOn skip branches and rerun the app manually (`RUST_LOG=info`), pressing the pad + upper keys.
2. **Zone-B gating:** NoteOn routing skips `!part.enabled || part.mute`; perf part 4 enabled=true — confirm `SetPartEnabled{4,true}` reaches the engine and part 4's `min/max` are 60/127.
3. **Sampler channel/program:** part 4 → rustysynth ch 4; confirm `set_part_program` ran for ch 4 AFTER any `load_keys_soundfont` (which resets channels and restores programs only for `part_sf2[i]` set at that moment). If `SetPartSf2Mode{4,true}` arrives after the font load, `part_sf2[4]` restore is skipped — then only the explicit `SetPartSf2Program` in the perf load sets the program; verify it does.
4. **Transpose clamp:** part 4 transpose +12; note 72 → 84, no clamp issue — the repro includes the transpose events so this is covered.

**Steps:**

- [ ] **Step 1: Write `tests/layer_switch_debug.rs`** replicating the GUI event sequence for halloween.json against a real engine + SC-55.sf2, asserting both part 0 (note 48) and part 4 (note 72) produce sound:

```rust
//! Diagnosis: SF2 layer silent after performance load (halloween.json repro).
//! Requires the local soundfont; skipped automatically if it is absent.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;
const SF2_PATH: &str = "/home/dima/.local/share/mini_midi_synth/sf2/SC-55.sf2";

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

fn load_font() -> Option<std::sync::Arc<rustysynth::SoundFont>> {
    let mut file = std::fs::File::open(SF2_PATH).ok()?;
    rustysynth::SoundFont::new(&mut file).ok().map(std::sync::Arc::new)
}

#[test]
fn perf_load_both_sf2_layers_speak() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };

    let patches = preset::load_all_patches();
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches);
    // App startup sequence: font load, then per-part SF2 mode
    let _ = e.handle_control(ControlEvent::LoadKeysSoundFont { soundfont: sf.clone() });
    // Reproduce load_performance_by_name for parts 0 and 4 exactly:
    // (order matters: enabled, volume, range, sf2 mode, sf2 program, LoadPatch params)
    let perf_part0 = ControlEvent::SetPartEnabled { part: 0, enabled: true };
    let _ = e.handle_control(perf_part0);
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: 0, program: 0, bank: 0 });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 4, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 4, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 4, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: 4, program: 0, bank: 0 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 4, semitones: 12 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: -12 });

    // Part 0: note 48 in range 0-59, transpose -12
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 127 });
    let p0 = render_peak(&mut e, 200);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 48 });

    // Part 4: note 72 in range 60-127, transpose +12
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let p4 = render_peak(&mut e, 200);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });

    println!("part0 peak {p0}, part4 peak {p4}");
    assert!(p0 > SILENCE, "part 0 silent (peak {p0})");
    assert!(p4 > SILENCE, "part 4 silent (peak {p4})");
}
```

- [ ] **Step 2: Run it** — `cargo test --test layer_switch_debug -- --nocapture`. If it FAILS on part 4: the bug is engine-side; bisect the event sequence (drop events one at a time, e.g. skip `SetPartSf2Program`, skip `SetPartRange`, reorder LoadPatch) and record which minimal sequence reproduces. If it PASSES: the bug is GUI-side (event NOT sent, or a different GUI action intervenes) — add `log::info!` trace to `load_performance_by_name` and to the NoteOn routing skip branches, run the app with `RUST_LOG=info`, press the pad + upper keys, read the log.

- [ ] **Step 3: Record root cause** in a comment block at the top of the test file (which hypothesis, exact minimal repro sequence). Do NOT fix yet — report findings.

- [ ] **Step 4: Commit** `git add tests/layer_switch_debug.rs && git commit -m "test: SF2 layer-switch diagnosis repro (root cause noted)"`

---

### Task 2: Fix the silent SF2 layer (whatever Task 1 found)

**Files:** `src/synth/mod.rs` and/or `src/gui/mod.rs` (depends on Task 1 findings; the test from Task 1 must go green).

- [ ] **Step 1:** Apply the minimal fix for the recorded root cause. Known-likely candidates: (a) `load_performance_by_name` must send `SetPartSf2Mode`/`SetPartSf2Program` AFTER any event that can reset sampler channel state, and `send_part_range` must stay last; (b) if engine-side: NoteOn routing skip must not gate SF2 parts on stale `min/max` — correct the ordering instead of the gate.
- [ ] **Step 2:** Run `cargo test --test layer_switch_debug -- --nocapture` — both peaks > SILENCE.
- [ ] **Step 3:** Run the whole suite — `cargo test 2>&1 | grep -E 'FAILED|failures:' | head -5` (expect none).
- [ ] **Step 4:** Commit (stage exactly the files changed): `git add <files-from-step-1> && git commit -m "fix: <root cause from Task 1>"`.

---

### Task 3: Vendor patch — `Synthesizer::set_release_time_scale` for rustysynth

**Files:**
- Modify: `vendor/rustysynth/src/synthesizer.rs` (field + setter + use), `vendor/rustysynth/src/voice.rs` (thread scale through `start`), `vendor/rustysynth/src/region_ex.rs` (scale before the 10 ms clamp), `src/synth/sampler.rs` (forward API), `src/synth/mod.rs` (ControlEvent + wiring)

**Step 1: rustysynth — field + setter.** In `Synthesizer` struct add `release_time_scale: f32` (init `1.0`), plus:

```rust
    /// Global multiplier applied to every volume-envelope release time
    /// (GM2 CC72 semantics, global form). 1.0 = SoundFont defaults.
    pub fn set_release_time_scale(&mut self, scale: f32) {
        self.release_time_scale = scale.clamp(0.02, 20.0);
    }
```

**Step 2: rustysynth — apply the scale at release-time computation.** The release seconds are computed in `RegionEx::start_volume_envelope` (vendor/rustysynth/src/region_ex.rs:62), a static fn called from `Voice::start` (voice.rs:162), which is called from `synthesizer.rs:240` (`value.start(...)`). Voices hold no synthesizer reference, so thread the scale as an argument:

1. `Synthesizer` reads its field into a local at the call site (synthesizer.rs:240): `let release_scale = self.release_time_scale;` and passes it as a new last parameter of `value.start(...)`.
2. `Voice::start(region, channel, key, velocity, release_scale: f32)` — add the parameter (check the real signature first) and pass through to `RegionEx::start_volume_envelope`.
3. In `region_ex.rs:62`, scale BEFORE the 10 ms pop-guard clamp (scaling after it defeats the floor: 0.02 × 10 ms = 0.2 ms):

```rust
        let release = release * release_scale;
        let release = SoundFontMath::max(release, 0.01); // existing pop guard stays after
```

Do NOT change timecents math. Voices already sounding are not affected (scale applies at note start — document this in the setter comment).

**Step 3: Sampler forward.** In `src/synth/sampler.rs`:

```rust
    /// Global SF2 release multiplier (GM2 CC72 semantics). 1.0 = file defaults.
    pub fn set_release_time_scale(&mut self, scale: f32) {
        if let Some(synth) = &mut self.keys.synth {
            synth.set_release_time_scale(scale);
        }
    }
```

**Step 4: Engine wiring.** New `ControlEvent::SetSf2ReleaseScale { scale: f32 }` handler in `src/synth/mod.rs`: clamp + `self.sampler.set_release_time_scale(scale)`. Also re-apply after `LoadKeysSoundFont` (rebuild creates a fresh Synthesizer with scale 1.0 — store the current scale in `SamplerEngine` next to `sustain: bool` and re-send in `apply_sustain()`'s sibling, or simply re-call the setter in `load_keys_soundfont` from stored state — same pattern as `apply_sustain`).

**Step 5: Tests.** Extend `tests/layer_switch_debug.rs` (it already loads SC-55.sf2):

```rust
#[test]
fn sf2_release_scale_shortens_tail() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };

    // Tail = peak over a fixed window after note-off, measured twice with
    // the same notes: scale 1.0 vs scale 0.05. Comparison, not absolutes —
    // SC-55 piano samples have natural decay on top of the release.
    // (Caveat: if a program's native release were already ≤ the 10 ms
    // pop-guard floor, both runs clamp identically and the assert fails —
    // piano programs are well above it.)
    fn tail_peak(scale: Option<f32>, sf: &std::sync::Arc<rustysynth::SoundFont>) -> f32 {
        let mut e = SynthEngine::new(SR);
        e.set_patches(preset::load_all_patches());
        let _ = e.handle_control(ControlEvent::LoadKeysSoundFont { soundfont: sf.clone() });
        let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
        let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
        let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: 0, program: 0, bank: 0 });
        if let Some(s) = scale {
            let _ = e.handle_control(ControlEvent::SetSf2ReleaseScale { scale: s });
        }
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
        let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
        for _ in 0..100 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // attack
        e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
        for _ in 0..40 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~27ms into release
        let mut peak = 0.0f32;
        for _ in 0..20 {
            l.fill(0.0); r.fill(0.0);
            e.tick_block(&mut l, &mut r);
            for i in 0..BLOCK_SIZE { peak = peak.max(l[i].abs()).max(r[i].abs()); }
        }
        peak
    }

    let default_tail = tail_peak(None, &sf);
    let fast_tail = tail_peak(Some(0.05), &sf);
    println!("sf2 tail default {default_tail}, scaled {fast_tail}");
    assert!(fast_tail < default_tail * 0.5,
        "release scale 0.05 did not halve the early tail ({fast_tail} vs {default_tail})");
}
```

**Step 6:** `cargo test --test layer_switch_debug -- --nocapture`, rt_safety, clippy. Commit:

```bash
git add vendor/rustysynth/src/synthesizer.rs vendor/rustysynth/src/voice.rs vendor/rustysynth/src/region_ex.rs src/synth/sampler.rs src/synth/mod.rs tests/layer_switch_debug.rs
git commit -m "feat: SF2 release-time scale (GM2 CC72 semantics) via vendor patch"
```

---

### Task 4: DSP sustain-release override (optional, off by default)

**Files:**
- Modify: `src/synth/envelope.rs` (`Envelope::note_off` — add duration-override variant), `src/synth/voice.rs` (pass-through), `src/synth/mod.rs` (Part + engine: store `sustain_release: Option<f32>` global param, apply on pedal-up offs), `src/config.rs` (persist `sustain_release: Option<f32>`, default None), `src/gui/mod.rs` (global bar: toggle + slider, hidden/disabled when OFF)

**Step 1: Envelope override.** In `envelope.rs` (real names: private enum `EnvStage` with variant `Idle`; IIR shapes use `release_coeff`, phase shapes use `phase_step_r`; helper `time_to_coeff(secs, sr)` at envelope.rs:70 — READ the file and adapt):

```rust
    /// Note-off with an explicit release duration (seconds), overriding the
    /// configured release rate. Used for the optional sustain-release setting.
    pub fn note_off_for(&mut self, seconds: f32) {
        if self.stage == EnvStage::Idle { return; }
        self.stage = EnvStage::Release;
        self.phase = 0.0; // Decay leaves phase at 1.0; without the reset a
                          // sustained note under a Linear/Quadratic shape
                          // would cut to zero instantly.
        self.release_start = self.output;
        let t = seconds.max(0.02);
        // BOTH release representations must be set: IIR shapes read
        // release_coeff, Linear/Quadratic shapes read phase_step_r
        // (user-selectable via env_release_shape). Missing one silently
        // keeps the patch release for that shape.
        self.release_coeff = time_to_coeff(t, self.sample_rate);
        self.phase_step_r = 1.0 / (t * self.sample_rate);
        // NOTE: do NOT write a raw 1/(t*sr) into release_coeff — the IIR
        // coefficient is exp(-1/(t*sr)); time_to_coeff already does this.
    }
```

(Verify `time_to_coeff` visibility/signature in envelope.rs:70 — it is `exp(-1/(t*sr))`; if private to the module that's fine, same module.)

**Step 2: Voice + Part passthrough.** `Voice::note_off_for(seconds)` calling `envelope.note_off_for` for the amp env only (filter env keeps normal release). `Part::note_off_for(note, seconds)` — same voice-matching as `Part::note_off`.

**Step 3: Engine plumbing.** `ControlEvent::SetSustainRelease { seconds: Option<f32> }` → store on SynthEngine; in `set_sustain(0)` (pedal-up), when `Some(secs)`, release pedal-held voices with `note_off_for(secs)` instead of `note_off()`. Key-release paths (`MidiEvent::NoteOff` etc.) are NOT touched — they keep the patch release (Test Case 3).

**Step 4: Config + GUI.** `config.ui.sustain_release: Option<f32>` (serde default None). GUI: in the global bar after F1 Sustain, a small toggle "SR" + slider 0.05–8.0 s (visible only when ON), sending `SetSustainRelease`; persisted via `save_global_to_config`. Default OFF — zero behavior change (Test Case 1).

**Step 5: Tests** — in `tests/sustain_audit.rs` add:

```rust
#[test]
fn dsp_sustain_release_override() {
    // Init patch: amp_release 0.3s, amp_sustain 0.7, IIR sqrt release.
    // Override 0.05s: after ~100ms the tail must be far below the patch's
    // natural decay (0.7 * exp(-100/300) ≈ 0.50), so use 0.05s override and
    // a generous threshold that the patch tail provably exceeds.
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { seconds: Some(0.05) });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    pedal_up(&mut e); // pedal-held note released with the 0.05s override
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..150 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~100ms
    let peak = render_peak(&mut e, 10);
    assert!(peak < 0.15, "override release did not shorten the tail (peak {peak})");
}

#[test]
fn key_release_uses_patch_release() {
    // Override 0.05s vs patch release 0.3s. At ~250ms post-key-off the
    // override envelope is fully idle (< SILENCE, ~180ms to Idle) while the
    // patch tail still holds ≈ 0.7 * exp(-250/300) ≈ 0.30 — the assert
    // discriminates (a plain > 0.0 would pass even if the override leaked).
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { seconds: Some(0.05) });
    pedal_up(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..375 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~250ms
    let peak = render_peak(&mut e, 10);
    assert!(peak > 0.15, "key release appears overridden by sustain-release (peak {peak})");
}
```

(Also: existing `pedal_up_releases_held_notes` must keep passing with the feature OFF — no test changes.)

**Step 6:** Full gate + `git commit -m "feat: optional DSP sustain-release override (off by default)"` staging `src/synth/envelope.rs src/synth/voice.rs src/synth/mod.rs src/config.rs src/gui/mod.rs tests/sustain_audit.rs`.

---

### Task 5: Pad-bank-2 transport (channel 10): play/pause/record/undo

**Files:**
- Modify: `src/synth/mod.rs` — drum routing in `MidiEvent::NoteOn` (`channel == 9` branch ~1495-1531): intercept transport notes BEFORE the drum trigger path; `MidiEvent::NoteOff` ch9 branch — ignore offs for transport notes (they are one-shot)

**Step 1: DrumEngine setters (atoms are private; the engine's tick re-syncs atoms only while `enabled`, so an explicit store is required).** In `src/synth/drum.rs`, `impl DrumEngine`:

```rust
    /// Pad-transport record toggle: snapshots before enabling so undo
    /// reverts exactly what this recording added (same as DrumSeqRecord).
    pub fn set_recording(&mut self, on: bool) {
        if on && !self.sequencer.recording {
            self.sequencer.save_snapshot();
        }
        self.sequencer.recording = on;
        self.rec_atom.store(on as u8, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_playing(&mut self, on: bool) {
        self.sequencer.playing = on;
        self.play_atom.store(on as u8, std::sync::atomic::Ordering::Relaxed);
    }
```

(Read the real field names in `StepSequencer` first — `recording`/`playing`/`save_snapshot()` per drum.rs ~715-747; the GUI's `DrumSeqRecord{true}` handler at mod.rs ~1960 does the same save_snapshot-then-enable, mirror it.)

**Step 1b: Intercept.** At the top of the combined `if channel == 9 || (is_drum_note && drum_mode)` block (mod.rs ~1518) — before the drum trigger path:

```rust
                // Pad bank 2 transport (notes outside the drum range 36-51):
                // 83 (B5) record, 82 (A#5) play/pause, 81 (A5) pause/stop,
                // 64 (E4) undo. Never reach the drum trigger path.
                if channel == 9 && velocity > 0 {
                    match note {
                        83 => {
                            let rec = !self.drum_engine.sequencer.recording;
                            self.drum_engine.set_recording(rec);
                            return;
                        }
                        82 => {
                            let playing = !self.drum_engine.sequencer.playing;
                            self.drum_engine.set_playing(playing);
                            return;
                        }
                        81 => {
                            self.drum_engine.set_playing(false);
                            return;
                        }
                        64 => {
                            self.drum_engine.sequencer.undo();
                            return;
                        }
                        _ => {}
                    }
                }
```

`return` is safe: the ch9 branch never calls `looper.record_event`. velocity==0 offs for these notes fall through to the drum `note_off` no-op (drum.rs:934-936); `MidiEvent::NoteOff` ch9 likewise.

**Step 2: Mapping note.** Play/pause uses one note (82: toggle) plus a separate hard-stop note (81) — matches the user's "воспроизведение / пауза" pair. If the looper is the intended target instead of the drum sequencer, replace the four bodies with `LooperRecord` / `LooperTogglePlay` / `LooperTogglePlay`(stop) / `LooperUndo` equivalents — everything else stays.

**Step 3: Tests** — in `tests/sustain_audit.rs` (it has the engine harness) add; `use std::sync::atomic::Ordering;` must be added to the file's imports:

```rust
#[test]
fn pad_transport_controls_drum_seq() {
    let mut e = engine_with_init_patch();
    // NOTE: record_hit fires only while recording AND playing (drum.rs:799),
    // so playback is enabled before recording the kick.
    // record on (B5)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert_eq!(e.drum_engine.rec_atom().load(Ordering::Relaxed), 1, "B5 did not start recording");
    // play on (A#5) so record_hit actually captures
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    assert_eq!(e.drum_engine.play_atom().load(Ordering::Relaxed), 1, "A#5 did not start playback");
    // hit a kick while recording (goes through the normal drum path)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 36, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 36 });
    // record off (B5 toggle)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    assert_eq!(e.drum_engine.rec_atom().load(Ordering::Relaxed), 0, "B5 did not stop recording");
    // undo (E4) must revert the recorded hit
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 64, velocity: 127 });
    let pat = e.drum_engine.sequencer.current_pattern as usize;
    let any_hit: bool = e.drum_engine.sequencer.patterns[pat]
        .steps.iter().flatten()
        .any(|s| s.velocity > 0);
    assert!(!any_hit, "undo did not revert the recorded kick");
    // pause (A5)
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    assert_eq!(e.drum_engine.play_atom().load(Ordering::Relaxed), 0, "A5 did not pause playback");
}

#[test]
fn pad_transport_notes_do_not_trigger_drums() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 82, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 81, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 64, velocity: 127 });
    let peak = render_peak(&mut e, 200);
    assert!(peak < SILENCE, "transport notes leaked into drum audio (peak {peak})");
}
```

(`drum_engine` is a pub field of SynthEngine (mod.rs ~1013) and `rec_atom()`/`play_atom()` getters exist on DrumEngine (drum.rs:905-911) — no new getters needed. Adjust the undo assertion if `record_hit` only fires while the sequencer is playing: in that case enable playback via note 82 before recording the kick, and pause after undo.)

**Step 4:** Full gate (clippy, cargo test, rt_safety — the intercept is allocation-free) + commit:

```bash
git add src/synth/mod.rs src/synth/drum.rs tests/sustain_audit.rs
git commit -m "feat: pad bank 2 transport — B5 record, A#5 play, A5 pause, E4 undo (ch 10)"
```

---

### Task 6: Transpose change mid-note must not orphan voices (sustain hang)

**Symptom:** turning Tr (or any part change that shifts note matching) while a note sounds → its note-off is computed with the NEW transpose and never matches the voice started with the OLD one → note rings forever ("сустейн зависает").

**Root cause:** `Voice.note` stores the TRANSPOSED note; every off path matches `voice.note == transposed(note_off)`. Transpose changed between on and off ⇒ mismatch. Poly aftertouch matching (mod.rs ~1660) has the same fragility.

**Fix: match voices by the RAW (pre-transpose) note.** Store the raw note on the voice; engine passes raw notes to `Part::note_on/note_off`, part transposes internally for the actual pitch. SF2 parts keep engine-side transposition (rustysynth takes a key number), but their offs are also drained by every state-change path (`part_all_notes_off`), so the residual mismatch window is bounded.

**Files:**
- Modify: `src/synth/voice.rs` (`Voice` struct: add `pub(crate) src_note: u8`; `retrigger_note` retranspose), `src/synth/mod.rs` (Part::note_on/note_off signature change to raw note + internal transpose; all engine call sites; poly aftertouch src_note match; SetPartTranspose unchanged)

**Step 1: Voice.** Add `pub(crate) src_note: u8` next to `note: u8` (initialized wherever `note` is — find `Voice::new` / first assignment, voice.rs:118).

**Step 2: Part::note_on/note_off signature change.** Currently: engine computes `transposed = (note + part.transpose).clamp(0,127)`, calls `part.note_on(transposed, vel)`; `Part::note_off(note)` matches `voice.note == note`. Change to:

- `Part::note_on(&mut self, raw_note: u8, velocity: u8)`: computes `transposed` internally (same clamp), stores `voice.note = transposed` (actual pitch, unchanged behavior for synth internals) AND `voice.src_note = raw_note`. Range gating (`min_note/max_note`, raw note) stays in the engine NoteOn routing — unchanged.
- `Part::note_off(&mut self, raw_note: u8)`: matches `voice.src_note == raw_note` instead of `voice.note`. The Task 4 addition `Part::note_off_for` must switch to `src_note` matching too, or the pedal-up path reintroduces this bug.
- **Complete call-site list (compiler will catch stragglers, but do these explicitly):**
  - Pass RAW to part.note_on / part.note_off (drop engine transposed computation for DSP arms): live NoteOn (mod.rs ~1591), vel==0 branch (~1575), NoteOff branch (~1613), arp tick on/off in tick_block (~2549/2562), looper replay (~2568-2588), `flush_arp_note_off` (~1392, already transposed today — becomes raw), CC120/123 arp flush (~1722) and AllNotesOff arp flush (~2047) — these already pass untransposed, now correct as-is.
  - Pass RAW, NO change needed (already raw): `LooperSetLayerMute`/`SetSolo` (mod.rs ~2001/2016) — today they mismatch transposed parts; fixed for free. Do NOT add transposition there.
  - MIDI-file sequencer `DspLayer0` arm (mod.rs ~2505/2517): passes raw today — under the new API part 0's transpose will now apply to file playback (consistent with the live path; intended behavior change — note it in the commit message). The SF2 arm there stays untransposed (pre-existing, out of scope).
  - Mono mode: `note_stack`/`latch_held` now hold RAW notes; `retrigger_note` (voice.rs:526) takes the transposed note — call sites (~618, ~779) must compute the fresh transposition inside Part and set BOTH `note` and `src_note` on the retriggered voice. Piano-mode dedup match (~635) and latch note-off match (~578) compare the Part argument — switch those to `src_note` (raw).
  - `sustained_notes` array: key by RAW consistently (writers ~801/813/818); it has no readers (only whole-array clears) so keying choice is inert — keep it simple.
  - Voice stealing by `v.note` (~647/655) is pitch-based — stays on `note` (transposed).
- **Poly aftertouch** (mod.rs ~1688-1698): currently matches `voice.note == note` against the RAW MIDI note — already broken for transposed parts. Change to `voice.src_note == note` (one line, fixes the latent bug).
- SF2 arms keep engine-side transposition: `sampler.note_on(i, transposed, vel)` / `sampler.note_off(i, transposed)` — unchanged.

**Step 3: Engine-side safety net for SF2.** In `ControlEvent::SetPartTranspose` handler: do nothing extra (the existing state-change drains cover held SF2 voices). Document with a comment.

**Step 4: Tests** — in `tests/sustain_audit.rs`:

```rust
#[test]
fn transpose_change_midnote_does_not_orphan_voice() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 0 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE, "no sound");
    // Change transpose WHILE the note sounds
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 7 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "transpose change orphaned the voice (peak {after})");
}

#[test]
fn transpose_change_with_pedal_held_note_releases() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held");
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 5 });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "pedal-up left orphaned voice after transpose change (peak {after})");
}
```

(The second case is already covered by `set_sustain(0)` releasing ALL voices — it guards the invariant against future regressions.)

**Step 5:** Full gate + commit:

```bash
git add src/synth/voice.rs src/synth/mod.rs tests/sustain_audit.rs
git commit -m "fix: voices match offs by raw note — transpose change no longer orphans voices"
```

---

## Self-Review Notes

- Spec coverage: user requests → Tasks 3-4 (custom release, DSP + SF2, off by default), Task 5 (transport), Tasks 1-2 (layer bug, diagnosis-first), Task 6 (transpose-mid-note hang, reported live).
- Types: `SetSf2ReleaseScale { scale: f32 }`, `SetSustainRelease { seconds: Option<f32> }` — used consistently; sampler stores scale beside `sustain: bool` for rebuild re-apply.
- Tests from requirement; negative matrix present; transport notes verified outside drum range.
- Open decision recorded in Task 5: drum sequencer target (looper swap = replace 4 bodies).
- Review round 1 fixed inline: envelope override uses time_to_coeff + sets phase_step_r (both release representations); override test threshold corrected against real init-patch tail math; rustysynth scale threaded through Voice::start into RegionEx::start_volume_envelope BEFORE the 10 ms clamp (region_ex.rs:62, not voice.rs); SoundFont::new takes &mut File; pad transport routed via DrumEngine::set_recording/set_playing (private atoms) with save_snapshot on record start; undo test added; Task 1 hypotheses scoped to what the repro covers.
- Review round 2 fixed inline: note_off_for resets self.phase (Decay leaves phase=1.0 → Linear/Quadratic instant cut); key_release test window extended to ~250ms with threshold 0.15 (plain >0.0 could not detect leakage); dead `patches` binding removed from sf2 test; transport test plays before recording (record_hit requires playing, drum.rs:799) and uses inline atom loads (closures held &e across &mut — E0502); Task 6 added (raw-note voice matching).
