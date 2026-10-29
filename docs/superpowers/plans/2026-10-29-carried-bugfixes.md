# Carried-Over Bugfixes Plan (mute gate, double cleanup, CC120 kill, transposed arp flush, rtrb advisory)

> anchor-checked: 2026-10-29
> status: not started
> Source: docs/TECHDEBT.md "Carried-over" list, items 24/26/28/29/30.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close five carried-over defects: (1) mute does not gate the mix; (2) `SetPartEnabled{false}` runs its cleanup twice; (3) CC120 releases instead of killing; (4) panic-path arp flushes dispatch untransposed off-notes; (5) rtrb RUSTSEC-2026-0274.

**Architecture:** The engine has two voice owners — DSP `Part.voices` (src/synth/voice.rs, 8 voices per part) and the rustysynth `SamplerEngine` (src/synth/sampler.rs; keys channels per part, drums on ch9). DSP part output is summed per part in `SynthEngine::tick_block` (src/synth/mod.rs:2740-2750), where only `enabled` is checked — `mute` never reaches the mix. SF2 output is summed globally by `SamplerEngine::tick` (all channels mixed into one stereo pair), so a per-part mute gate in the mix loop cannot reach SF2; SF2 is gated at the source with CC7. Voice lifecycle: `Voice::note_off` only arms the release envelope (voice.rs:508-511); there is no kill. rustysynth already has an immediate kill: `Synthesizer::note_off_all(immediate)` / `note_off_all_channel(ch, true)` (vendor/rustysynth/src/synthesizer.rs:259/275, CC 0x78 wiring at synthesizer.rs:166).

**Tech Stack:** Rust; test harness `tests/sustain_audit.rs` (`engine_with_init_patch` / `render_peak` / `settle`, lines 10-46); unit tests in `src/synth/tests.rs` (child module, has access to private `Part` fields); `cargo test`, `cargo clippy`, `cargo deny`.

**Decision: mute = 5 ms fade for DSP, instant CC7 gate for SF2.**
- DSP parts get a per-part scalar `mute_gain` ramped per sample in the mix loop: 1.0 → 0.0 over 5 ms when muted (240 samples @48k), and back on unmute. Feasible without allocation (one `f32` field on `Part`, `max`/`min` per sample). A fade was chosen over an instant gate because muting mid-note with an instant gate clicks audibly; 5 ms is short enough that the part is fully silent within 10 ms. Voices keep ticking during the fade so their release envelopes reach idle and the voice slot is freed.
- SF2 parts cannot be gated in the mix (global sum), so mute additionally sends CC7=0 to the part's rustysynth channel (`set_layer_volume(part, 0.0)`, sampler.rs:377-382 — instant per-channel gain multiply; a click is possible but acceptable, and the existing `part_all_notes_off` release drain keeps the tail short). Unmute restores CC7 from `Part.volume`.
- Side benefit: the mix gate also silences looper-replay note-ons into a muted part (mod.rs:2720 only checks `enabled` today).

**Out of scope (do not do here):** `seq_program_set` cache desync; sustain-atom divergence on MidiSeqLoad/Play-stop; `ControlEvent::AllNotesOff` (GUI panic, mod.rs:2160) staying release-style — only the MIDI CC120 arm gets the kill; native `drum_engine` voices are one-shot percussive and are not killed by CC120 (rustysynth drum voices are, via `all_sound_off`); `cargo fmt`.

---

## Test Cases (from the requirement)

Requirement: "Muting a part must silence it (within ~10 ms) even for notes started before the mute; disabling a part must run exactly one cleanup; CC120 must silence instantly while CC123 keeps a release tail; panic paths must dispatch transposed arp note-offs; rtrb must be patched to 0.3.5."

Written BEFORE implementation. Each task is executed by a FRESH subagent that receives ONLY this plan document.

1. `mute_gates_the_mix_within_10ms` — held note sounding, `SetPartMute{true}` → after discarding a ~5 ms fade window, the next ~10 ms render is below SILENCE. (RED before Task 5: today the mute handler only note-offs, leaving a 0.3 s release tail that the mix keeps summing.)
2. `unmute_restores_sound` — mute, wait, unmute → sound returns. (GREEN before and after; guards the CC7 restore and the fade-in.)
3. `mute_toggles_on_silent_engine_are_noop` — repeated mute/unmute with no voices → silence, no panic. (GREEN before/after; guards the new unmute CC7 path.)
4. `part_disable_releases_each_voice_once_and_cleans_state` (unit, src/synth/tests.rs) — sounding voice, `SetPartEnabled{false}` → every previously-active voice is releasing exactly once, held-note state cleared, re-enable restarts notes. (GREEN before and after — behavior-unchanged guard for Task 2.)
5. `dispatch_arp_note_off_releases_dsp_voice_with_transpose_set` (unit, src/synth/tests.rs) — transpose=12, sounding voice, direct call of the new helper → voice released. (GREEN after Task 3; guards the DSP branch of the helper.)
6. `cc120_kills_sound_immediately` — sounding note, MIDI CC120 → peak below SILENCE within 2 blocks, no settle. (RED before Task 4: today CC120 arms a 0.3 s release.)
7. `cc123_keeps_release_tail_then_silences` — sounding note, CC123 → tail audible immediately after, silence after settle. (GREEN before/after; guards against making CC123 kill too.)
8. `cc120_on_silent_engine_is_noop` — CC120 with no voices → silence, no panic. (GREEN.)
9. Existing `tests/sustain_audit.rs` suite must stay green — in particular `cc120_panic_releases_pedal_held_notes` (kill also satisfies settle-silence) and `mute_releases_sounding_notes`.
10. `cargo deny check advisories` passes after `cargo update -p rtrb` (0.3.3 → 0.3.5); only the pre-existing `warning[yanked]` for clipboard-win (warn-level per deny.toml `[advisories] yanked = "warn"`) may remain.

SF2-only behaviors (mute during SF2 sustain, CC120 killing SF2 voices audibly) cannot be driven in tests without a real .sf2 — `SamplerEngine` starts with `keys.synth == None`, making every SF2 path a no-op. They are verified by code review against the anchors in this plan (the mute test list in the eternal-sustain plan made the same concession).

## Negative Matrix

- **Missing/out-of-range index:** `SetPartMute` / `SetPartEnabled` with `part >= 8` → `self.parts.get_mut(part)` returns None → no-op (mod.rs:1955/1976). CC7 restore uses the same guard.
- **Zero/empty:** mute with no active voices → fade steps a scalar, nothing sounds, no panic. CC120 on an empty engine → `v.kill()` loop iterates zero voices; `all_sound_off` skips `None` synths (test 8).
- **SF2 not loaded:** `keys.synth == None` → `set_layer_volume` / `all_sound_off` are no-ops via `if let Some` (sampler.rs:379/440 pattern).
- **Extreme values:** transpose clamp — `(off as i16 + part.transpose as i16).clamp(0, 127)` (existing pattern, mod.rs:1432); helper keeps it. `mute_gain` clamped by `.max(0.0)` / `.min(1.0)`.
- **Duplicate/concurrent:** repeated CC120 / CC123 after CC120 → kill is idempotent (`v.active` already false); repeated `SetPartEnabled{false}` → second pass finds only releasing voices, `note_off` on a releasing voice re-arms release from current output — harmless, and after Task 2 there is exactly one pass anyway.
- **Ordering:** CC120 handler lifts sustain first (`set_sustain(0)`, mod.rs:1800) so rustysynth hold-pedal cannot swallow anything and the GUI atom cannot stay stuck.
- **Clock skew:** n/a — single audio thread, event ordering only.
- **External-call timeout:** no new cross-thread calls; mute fade and kills are pure audio-thread state.

---

### Task 0: Branch

- [ ] From master:
```bash
cd /home/dima/Projects/mini_midi_synth
git checkout master && git pull
git checkout -b bugfix-carried-over
```

---

### Task 1: rtrb RUSTSEC-2026-0274 (Cargo.lock only, no code)

**Facts:** Cargo.lock pins rtrb 0.3.3; deny.toml `[advisories]` has `yanked = "warn"` and no rtrb ignore entry. Advisory fix is internal to `ReadChunk::commit` — the 0.3.x API (`RingBuffer::new`, `Producer::push`, `Consumer::pop`) is unchanged. Project usage (verified): src/lib.rs:171/175/176 (`RingBuffer::new`), src/audio.rs:177/182/196/249 (`pop`), src/midi.rs:176 (`push`), src/synth/mod.rs:1457 (`set_feedback_tx`), src/metrics.rs (`CountingProducer` wraps `rtrb::Producer`). No API migration needed.

- [ ] Step 1. Confirm current version:
```bash
grep -A1 'name = "rtrb"' Cargo.lock
```
Expected: `version = "0.3.3"`.

- [ ] Step 2. Update:
```bash
cargo update -p rtrb
```
Expected output contains: `Updating rtrb v0.3.3 -> v0.3.5` (exact arrow formatting may vary; no other crates should move — if others move, inspect `git diff Cargo.lock` and revert unrelated bumps with `cargo update -p <crate> --precise <old>`).

- [ ] Step 3. Verify:
```bash
grep -A1 'name = "rtrb"' Cargo.lock
cargo deny check advisories
```
Expected: `version = "0.3.5"`; advisories check ends with no `error[...]` lines (a `warning[yanked]` for clipboard-win 5.4.1 is pre-existing and warn-level — do not fix it here). Exit code 0.

- [ ] Step 4. Quick compile + full test (API compatibility proof):
```bash
cargo test
```
Expected: all suites pass (`cargo test` compiles every rtrb call site).

- [ ] Step 5. Commit:
```bash
git add Cargo.lock
git commit -m "fix: bump rtrb to 0.3.5 (RUSTSEC-2026-0274 double-free in ReadChunk::commit)"
```

---

### Task 2: Collapse duplicate `SetPartEnabled{false}` cleanup

**Facts (anchor-verified):** src/synth/mod.rs:1954-1974. The `if !enabled` block at 1956-1965 (sampler drain, voice note-offs, `sustained_notes`/`keys_down` clear, `note_stack`/`latch_held` clear) is followed by a *second* `if !enabled` block at 1967-1972 that note-offs the voices again and re-clears `note_stack`/`latch_held`. The second `note_off` pass re-arms the release envelope of already-releasing voices. Collapse into one block. Behavior is unchanged by design — the test below is a guard, expected GREEN before and after.

- [ ] Step 1. Add the guard test to `src/synth/tests.rs` (append at end of file; the module has `use super::*` and can see private `Part` fields):
```rust
// ── Part enable/disable state guard ────────────────────────────

#[test]
fn part_disable_releases_each_voice_once_and_cleans_state() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    let active: Vec<bool> = e.parts[0].voices.iter().map(|v| v.active).collect();
    assert!(active.iter().any(|&a| a), "no active voice to disable");
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: false });
    for (i, v) in e.parts[0].voices.iter().enumerate() {
        if active[i] {
            assert!(v.is_releasing(), "voice {i} not released after part disable");
        }
    }
    assert!(e.parts[0].note_stack.is_empty(), "note_stack not cleared");
    assert!(e.parts[0].latch_held.is_empty(), "latch_held not cleared");
    assert!(!e.parts[0].sustained_notes.iter().any(|&s| s), "sustained_notes not cleared");
    assert!(!e.parts[0].keys_down.iter().any(|&k| k), "keys_down not cleared");
    // Re-enable: notes must sound again
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "part did not restart after re-enable");
}
```
- [ ] Step 2. Run — expect GREEN both now and after the change (guard):
```bash
cargo test --lib part_disable_releases_each_voice_once
```
Expected: `test synth::tests::part_disable_releases_each_voice_once_and_cleans_state ... ok`.

- [ ] Step 3. Replace mod.rs:1954-1974 (`ControlEvent::SetPartEnabled` arm) with:
```rust
            ControlEvent::SetPartEnabled { part, enabled } => {
                if let Some(l) = self.parts.get_mut(part) {
                    if !enabled {
                        // Single cleanup: drain SF2 voices, release DSP voices,
                        // clear held-note state. (This used to be duplicated in
                        // a second `if !enabled` block below, whose note_off
                        // pass re-armed the release envelope of voices that
                        // were already releasing.)
                        self.sampler.part_all_notes_off(part);
                        for voice in &mut l.voices {
                            if voice.active { voice.note_off(); }
                        }
                        l.sustained_notes = [false; 128];
                        l.keys_down = [false; 128];
                        l.note_stack.clear();
                        l.latch_held.clear();
                    }
                    l.enabled = enabled;
                }
            }
```
- [ ] Step 4. Run:
```bash
cargo test --lib part_disable_releases_each_voice_once
cargo test --test sustain_audit part_disable_releases_pedal_held_notes
```
Expected: both `ok`. Second command proves the graceful release behavior is unchanged.

- [ ] Step 5. Commit:
```bash
git add src/synth/mod.rs src/synth/tests.rs
git commit -m "fix: SetPartEnabled{false} ran its voice cleanup block twice — collapse into one"
```

---

### Task 3: Transposed arp note-off dispatch for panic paths

**Facts (anchor-verified):** `flush_arp_note_off` (mod.rs:1425-1439) dispatches the arp gate-off with part transpose applied and routes SF2 by transposed pitch. Three panic paths duplicate this loop *without* transpose, so an SF2 part with transpose never receives the matching note-off (the arp gate voice hangs until a later full drain):
- CC120/123 arm: mod.rs:1813-1819
- `ControlEvent::AllNotesOff`: mod.rs:2177-2183
- `ControlEvent::SetArpParams` (disable): mod.rs:2342-2348

Fix: extract a shared helper `dispatch_arp_note_off` next to `flush_arp_note_off` and call it from all four sites. DSP parts receive the raw note (they match offs by `src_note`), so audio behavior for DSP is unchanged — the bug is SF2-only and untestable without a soundfont (see Test Cases note). Do NOT add looper recording to the panic paths (they never recorded; `flush_arp_note_off` keeps its own `record_event`).

- [ ] Step 1. Add helper + refactor `flush_arp_note_off` (mod.rs:1425-1439). Replace the whole function with:
```rust
    /// Dispatches an arp note-off to every enabled part with part transpose
    /// applied: SF2 channels are keyed by transposed pitch, DSP parts receive
    /// the raw note (they match offs by src_note). Shared by flush_arp_note_off
    /// and the panic paths (CC120/123, AllNotesOff, SetArpParams-off) so the
    /// SF2 gate voice always gets a matching off, transposed like its on.
    fn dispatch_arp_note_off(&mut self, off: u8) {
        for (i, part) in self.parts.iter_mut().enumerate() {
            if !part.enabled { continue; }
            let transposed = (off as i16 + part.transpose as i16).clamp(0, 127) as u8;
            if part.sf2_mode { self.sampler.note_off(i, transposed); }
            else { part.note_off(off); } // raw: part matches by src_note
        }
    }

    /// Last held arp key released: the arpeggiator resets pattern_len, so
    /// tick() will never dispatch the pending gate-off — the sounding voice
    /// would hang forever. Dispatch the off here (same routing as tick_block).
    fn flush_arp_note_off(&mut self) {
        if self.arpeggiator.has_held_notes() {
            return; // tick() still owns gate-offs
        }
        if let Some(off) = self.arpeggiator.flush_current_note() {
            self.dispatch_arp_note_off(off);
            let ap = self.active_part.load(std::sync::atomic::Ordering::Relaxed);
            self.looper.record_event(off, 0, ap);
        }
    }
```
- [ ] Step 2. Replace the three panic-path loops.
  - mod.rs:1813-1819 (inside the `cc == 120 || cc == 123` arm, after `self.drum_engine.reset_choke_state();`) with:
```rust
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
```
  - mod.rs:2177-2183 (inside `ControlEvent::AllNotesOff`, comment line 2176 `// Clear arpeggiator held notes` stays) with:
```rust
                if let Some(off_note) = self.arpeggiator.flush_current_note() {
                    // Same transposed routing as flush_arp_note_off.
                    self.dispatch_arp_note_off(off_note);
                }
```
  - mod.rs:2342-2348 (inside `ControlEvent::SetArpParams`, `if !enabled {` block) with:
```rust
                if !enabled {
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                }
```
- [ ] Step 3. Add the unit test to `src/synth/tests.rs` (guards the DSP branch of the helper; SF2 branch is code-reviewed only — no .sf2 in CI):
```rust
#[test]
fn dispatch_arp_note_off_releases_dsp_voice_with_transpose_set() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: 12 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "no sounding voice");
    e.dispatch_arp_note_off(60); // raw key; DSP matches by src_note
    assert!(
        e.parts[0].voices.iter().all(|v| !v.active || v.is_releasing()),
        "dispatch_arp_note_off did not release the sounding voice"
    );
}
```
- [ ] Step 4. Run:
```bash
cargo test --lib dispatch_arp_note_off
cargo test --test sustain_audit
```
Expected: new test `ok`; the whole sustain_audit suite (incl. `arp_transposed_note_off_stops_sound`, `panic_after_held_pedal_up_kills_all`) stays green — proof the refactor changed no DSP behavior.
- [ ] Step 5. Commit:
```bash
git add src/synth/mod.rs src/synth/tests.rs
git commit -m "fix: panic-path arp flushes dispatched untransposed SF2 note-offs — share dispatch_arp_note_off"
```

---

### Task 4: CC120 = immediate kill, CC123 stays release-style

**Facts (anchor-verified):** mod.rs:1792-1821 treats CC120 and CC123 identically: release-style `v.note_off()` on DSP voices + `sampler.all_notes_off()` (which sends CC123 to rustysynth, sampler.rs:498-507). rustysynth supports an immediate kill: `Synthesizer::note_off_all(true)` (vendor/rustysynth/src/synthesizer.rs:259-267; per-channel CC 0x78 wiring at :166). `Voice` has no kill method (only `note_off`, voice.rs:508-511) — add one. `Envelope` has a private `stage: EnvStage` and `output: f32` (envelope.rs:44+, set in `note_on`/`note_off`).

- [ ] Step 1. Add failing test to `tests/sustain_audit.rs` (append after `cc120_panic_releases_pedal_held_notes`, line 125):
```rust
#[test]
fn cc120_kills_sound_immediately() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE, "no sound to kill");
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    // No settle: All Sound Off must be audible silence within two blocks
    let right_after = render_peak(&mut e, 2);
    assert!(right_after < SILENCE, "CC120 left a release tail (peak {right_after})");
}

#[test]
fn cc123_keeps_release_tail_then_silences() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 100) > SILENCE);
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 123, value: 0 });
    // All Notes Off is release-style: the tail must still be audible right after
    let tail = render_peak(&mut e, 2);
    assert!(tail > SILENCE, "CC123 lost its release tail (peak {tail})");
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC123 left ringing (peak {after})");
}

#[test]
fn cc120_on_silent_engine_is_noop() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    let peak = render_peak(&mut e, 10);
    assert!(peak < SILENCE, "CC120 on a silent engine produced sound (peak {peak})");
}
```
- [ ] Step 2. Run (RED):
```bash
cargo test --test sustain_audit cc120_kills_sound_immediately
```
Expected FAIL: `CC120 left a release tail (peak ...)` — peak ≈ sustain level (release 0.3 s, mod.rs init preset amp_release). The other two new tests are GREEN already (guards).
- [ ] Step 3. `Envelope::kill` — src/synth/envelope.rs, insert after `note_off` (after line 158):
```rust
    /// Immediate kill (CC120): zero output, Idle stage. No allocation.
    pub fn kill(&mut self) {
        self.stage = EnvStage::Idle;
        self.output = 0.0;
    }
```
- [ ] Step 4. `Voice::kill` — src/synth/voice.rs, insert after `note_off` (after line 511):
```rust
    /// Immediate kill (CC120 All Sound Off): voice stops producing sound from
    /// the next tick, no release tail, no allocation. Compare `note_off`,
    /// which only arms the release envelope.
    pub fn kill(&mut self) {
        self.active = false;
        self.amp_env.kill();
        self.filter_env.kill();
    }
```
- [ ] Step 5. `SamplerEngine::all_sound_off` — src/synth/sampler.rs, insert after `all_notes_off` (after line 507):
```rust
    /// All Sound Off (CC120): kill every rustysynth voice immediately, no
    /// release tail. Pedal state is irrelevant (voices are cleared) but is
    /// still cleared so a later note-off is not swallowed.
    pub fn all_sound_off(&mut self) {
        self.sustain = false;
        if let Some(synth) = &mut self.keys.synth { synth.note_off_all(true); }
        if let Some(synth) = &mut self.drums.synth { synth.note_off_all(true); }
    }
```
- [ ] Step 6. Split the CC arm — src/synth/mod.rs:1796-1821. Replace the whole `if cc == 120 || cc == 123 { ... return; }` block with:
```rust
                if cc == 120 || cc == 123 {
                    // Lift the sustain state everywhere first: rustysynth
                    // gates CC123 on the channel hold pedal, and the GUI
                    // sustain atom must not stay stuck ON.
                    self.set_sustain(0);
                    let all_sound_off = cc == 120; // All Sound Off kills, All Notes Off releases
                    for part in &mut self.parts {
                        for v in &mut part.voices {
                            if v.active {
                                if all_sound_off { v.kill(); } else { v.note_off(); }
                            }
                        }
                        part.note_stack.clear();
                        part.latch_held.clear();
                        part.sustain_pedal = 0.0;
                        part.sustained_notes = [false; 128];
                        part.keys_down = [false; 128];
                    }
                    if all_sound_off {
                        self.sampler.all_sound_off();
                    } else {
                        self.sampler.all_notes_off();
                    }
                    self.drum_engine.reset_choke_state();
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                    return;
                }
```
(Task 3 must be done first — this block calls `dispatch_arp_note_off`.)
- [ ] Step 7. Run (GREEN):
```bash
cargo test --test sustain_audit
cargo test --features rt-assert --test rt_safety
```
Expected: all sustain_audit tests ok (incl. `cc120_panic_releases_pedal_held_notes`, `repeated_cc123_is_idempotent`, `panic_after_held_pedal_up_kills_all`); rt_safety passes (kills are plain field writes, `note_off_all(true)` is rustysynth-internal, no alloc).
- [ ] Step 8. Commit:
```bash
git add src/synth/envelope.rs src/synth/voice.rs src/synth/sampler.rs src/synth/mod.rs tests/sustain_audit.rs
git commit -m "fix: CC120 All Sound Off now kills voices immediately; CC123 stays release-style"
```

---

### Task 5: Mute gates the mix (5 ms fade) + SF2 CC7 gate

**Facts (anchor-verified):** `Part` has `mute: bool` (mod.rs:427) but the mix loop (mod.rs:2740-2750) checks only `!part.enabled` — a muted part's release tail keeps being summed. Mute gates only new note-ons at mod.rs:1660 (live) and mod.rs:2703 (arp); looper replay (mod.rs:2720) checks only `enabled`. `SetPartMute` (mod.rs:1975-1991) already drains SF2 (`part_all_notes_off`, release-style) and note-offs DSP voices, so the tail is the only leak — but it can be seconds on long-release patches. SF2 output is a global sum (mod.rs:2753), so the mix gate cannot reach it; CC7 per channel does (`set_layer_volume`, sampler.rs:377-382; rustysynth applies channel volume as a per-sample gain).

- [ ] Step 1. Add failing tests to `tests/sustain_audit.rs` (append after `mute_releases_sounding_notes`, line 225):
```rust
#[test]
fn mute_gates_the_mix_within_10ms() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to mute");
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    // Discard the ~5 ms fade window (240 samples @48k = 8 blocks of 32);
    // everything after it must be gated to zero even though the voice is
    // still in its 0.3 s release.
    let _ = render_peak(&mut e, 8);
    let silenced = render_peak(&mut e, 16);
    assert!(silenced < SILENCE, "muted part still sounding ~10ms after mute (peak {silenced})");
}

#[test]
fn unmute_restores_sound() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound");
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
    let _ = render_peak(&mut e, 200);
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: false });
    // The release tail (0.3 s) is still alive; fade-in must bring it back
    let back = render_peak(&mut e, 200);
    assert!(back > SILENCE, "unmute did not restore sound (peak {back})");
}

#[test]
fn mute_toggles_on_silent_engine_are_noop() {
    let mut e = engine_with_init_patch();
    for _ in 0..5 {
        let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true });
        let _ = e.handle_control(ControlEvent::SetPartMute { part: 0, mute: false });
    }
    let peak = render_peak(&mut e, 50);
    assert!(peak < SILENCE, "mute toggling produced sound (peak {peak})");
}
```
- [ ] Step 2. Run (RED):
```bash
cargo test --test sustain_audit mute_gates_the_mix_within_10ms
```
Expected FAIL: `muted part still sounding ~10ms after mute (peak ...)` (peak ≈ sustain level). `unmute_restores_sound` and `mute_toggles...` are GREEN already.
- [ ] Step 3. Add the `mute_gain` field — src/synth/mod.rs:427, after `mute: bool,`:
```rust
    mute: bool,      // temporary silence without removing from scene
    /// Mix gate for mute: 1.0 open; ramps to 0 over ~5 ms when muted, back to
    /// 1.0 on unmute (click-free). Per-sample, audio thread only.
    mute_gain: f32,
```
- [ ] Step 4. Initialize it — src/synth/mod.rs:491. Change:
```rust
            voices, age_counter: 0, volume: 0.8, enabled: false, mute: false,
```
to:
```rust
            voices, age_counter: 0, volume: 0.8, enabled: false, mute: false, mute_gain: 1.0,
```
- [ ] Step 5. Gate the mix — replace mod.rs:2740-2750 (the `// Voice ticks` block) with:
```rust
            // Voice ticks (audio rate, using cached modulation)
            let mute_fade_step = 1.0 / (0.005 * self.sample_rate);
            for (li, part) in self.parts.iter_mut().enumerate() {
                if !part.enabled { continue; }
                let (l, r) = part.tick(&layer_mods_snapshot[li]);
                // Mute is a mix gate: a muted part contributes zero within ~5 ms
                // (linear fade both ways, so the gate itself does not click).
                // Voices keep ticking so their release envelopes reach idle and
                // the voice slot is freed.
                part.mute_gain = if part.mute {
                    (part.mute_gain - mute_fade_step).max(0.0)
                } else {
                    (part.mute_gain + mute_fade_step).min(1.0)
                };
                let fade = part.mute_gain;
                // Apply per-part pan (constant-power: sqrt of (0.5 ± pan*0.5))
                let pan = part.pan.clamp(-1.0, 1.0);
                let gain_l = ((0.5 - pan * 0.5) as f64).sqrt() as f32;
                let gain_r = ((0.5 + pan * 0.5) as f64).sqrt() as f32;
                out_l += l * gain_l * fade;
                out_r += r * gain_r * fade;
            }
```
(`self.sample_rate` is `f32`, set in `set_sample_rate`, mod.rs:2359-2360; `mute_fade_step` is computed once per block — no per-sample division cost concern, and no allocation.)
- [ ] Step 6. SF2 gate on mute — replace mod.rs:1975-1991 (`ControlEvent::SetPartMute` arm) with:
```rust
            ControlEvent::SetPartMute { part, mute } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.mute = mute;
                    if mute {
                        // A muted part must not keep sounding; SF2 voices live
                        // in the sampler, not in l.voices.
                        self.sampler.part_all_notes_off(part);
                        // SF2 output is summed globally in tick_block, so the
                        // mix gate cannot reach it — gate at the source with
                        // CC7 (instant; a click is possible but the release
                        // drain above keeps the tail short).
                        self.sampler.set_layer_volume(part, if part.mute { 0.0 } else { part.volume });
                        for voice in &mut l.voices {
                            if voice.active { voice.note_off(); }
                        }
                        l.sustained_notes = [false; 128];
                        l.keys_down = [false; 128];
                        l.note_stack.clear();
                        l.latch_held.clear();
                    } else {
                        // Restore the SF2 channel volume zeroed on mute.
                        self.sampler.set_layer_volume(part, l.volume);
                    }
                }
            }
```
- [ ] Step 7. Run (GREEN):
```bash
cargo test --test sustain_audit
cargo test --features rt-assert --test rt_safety
```
Expected: all sustain_audit tests ok (incl. existing `mute_releases_sounding_notes`, `looper_records_all_sounding_parts` — looper *recording* is untouched; the mix gate only silences replay of muted parts); rt_safety passes (fade is arithmetic on an existing struct, no alloc/lock/IO).
- [ ] Step 8. Commit:
```bash
git add src/synth/mod.rs tests/sustain_audit.rs
git commit -m "fix: mute now gates the mix (5ms fade); SF2 parts gated via CC7 — muted part silences within 10ms"
```

---

### Task 6: Full gate

- [ ] Step 1. Run the whole gate from CLAUDE.md:
```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```
Expected: all four pass. (`cargo deny check advisories` already passed in Task 1; the full check also runs licenses/bans/sources, which are unaffected by these changes. If `cargo deny check` fails on the pre-existing yanked clipboard-win warning at warn level, confirm it is warn-only — warnings do not fail the check.)
- [ ] Step 2. No commit needed if nothing is dirty; if docs were touched:
```bash
git status --short
```
Expected: clean tree (all five fixes committed by their tasks).
