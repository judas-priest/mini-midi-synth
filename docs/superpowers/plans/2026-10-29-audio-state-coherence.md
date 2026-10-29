# Audio State Coherence Plan (seq program cache desync, sustain-atom divergence)

> anchor-checked: 2026-10-29 against master @ ccdd519
> status: not started
> Source: docs/TECHDEBT.md "Carried-over" list, items at lines 25 and 27.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close two carried-over audio-state defects: (1) `seq_program_set` desyncs the `part_program[]` cache — MIDI-file program changes overwrite a live part's rustysynth channel program and the next `load_keys_soundfont`/`set_block_size` restore clobbers the sequencer's program mid-playback; (2) sustain-atom divergence — `sampler.all_notes_off()` in the `MidiSeqLoad` / `MidiSeqPlay{false}` paths force-clears only the sampler's pedal state while the engine sustain atom, `part.sustain_pedal` and `sustained_notes` stay ON, so DSP and SF2 sustain diverge until the next CC64 edge.

**Item 2 status (CC120 kill) — read this before adding scope:** CC120 is **NOT** fixed on master ccdd519: there is no `Voice::kill` on DSP voices and no `SamplerEngine::all_sound_off` (only native drum voices have `kill`, src/synth/drum.rs:154+). However it is **already fully planned** in `docs/superpowers/plans/2026-10-29-carried-bugfixes.md` (Task 4, "status: not started"), including `Envelope::kill`, `Voice::kill`, `all_sound_off`, the CC120/CC123 split, and the prerequisite `dispatch_arp_note_off` helper (its Task 3). The same plan also owns the panic-path arp flush fix (TECHDEBT line 28) and the rtrb bump. **This plan does not duplicate that work.** NEEDS_INPUT (non-blocking; default already applied): if you would rather absorb CC120 into this plan instead of running carried-bugfixes, say so and Tasks 4-5 of that plan get merged here verbatim.

**Architecture:** The engine has two sustain state stores that must agree: (a) the engine-level `sustain` atom + `Part.sustain_pedal` / `sustained_notes` (DSP; `Part::note_off` gates on `sustain_pedal > 0.5`, src/synth/mod.rs:824), and (b) `SamplerEngine.sustain` + per-channel CC64 in rustysynth (SF2). `set_sustain` (mod.rs:1387-1417) updates both; the seq stop/load paths call `sampler.all_notes_off()` directly (sampler.rs:498-507 sets only `SamplerEngine.sustain`) and skip `set_sustain` — that is the divergence. Program state lives in three places on `SamplerEngine` (src/synth/sampler.rs): `part_program[]`/`part_bank[]` (per-part cache, written only by `set_part_program` sampler.rs:352-360, restored after every rustysynth rebuild by `load_keys_soundfont` sampler.rs:238-256 and `set_block_size` sampler.rs:281-306), and `seq_channel_program[]` (per-channel GM program last sent by the MIDI-file sequencer, sampler.rs:203, dedup cache, reset to 255 only in `load_keys_soundfont` sampler.rs:239). The sequencer plays on the file's raw channels (mod.rs:2656-2660 `Sf2` arm: `seq_program_set` + `seq_note_on` before every note; ProgramChange follow at mod.rs:2676-2681), which share the same rustysynth `keys` channel space as the parts (`part_channel` allocates from the same melodic pool, sampler.rs:324-331). Fix shape: mark channels claimed by the sequencer (`seq_channel_used`), skip them in rebuild restores, reset the seq dedup cache on EVERY rebuild (`set_block_size` forgets this today — a latent variant of the same bug), and on stop/load release seq ownership and re-assert part programs.

**Tech Stack:** Rust; test harness `tests/sustain_audit.rs` (`engine_with_init_patch` / `render_peak` / `settle` / `pedal_down` / `pedal_up`, lines 10-70); new `#[cfg(test)] mod tests` at the bottom of src/synth/sampler.rs (private `SamplerEngine` fields are only visible inside the `sampler` module — src/synth/tests.rs is a sibling, not a descendant, so cache-level tests for item 1 MUST live in sampler.rs); unit tests in src/synth/tests.rs for item 2 (private `Part` fields, `use super::*`); `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo test --features rt-assert --test rt_safety`, `cargo deny check`.

**SF2 testability concession (applies to all program-cache audio behavior):** `SamplerEngine` starts with `keys.synth == None` and `SubSynth::midi` no-ops without a synth (sampler.rs:137-141), so every 0xB0/0xC0 send in these paths is unobservable in tests. Program ownership is therefore verified by cache-level unit tests (the arrays are the contract) plus code review against the anchors below — the same concession the carried-bugfixes plan records for SF2-only behavior. Item 2 (sustain) IS audio-observable for DSP parts and is tested that way.

**RT-safety:** all new code is plain fixed-size array reads/writes and existing `keys.midi(...)` sends on the audio thread — no allocation, no locks, no IO. `set_sustain(0)` is the same alloc-free code the CC120/123 arm (mod.rs:1802) and `ControlEvent::AllNotesOff` (mod.rs:2163) already run in the audio callback; `Vec::clear()` keeps capacity (no free). `rt_safety` is run after every audio-path task.

**Decision: rebuild restores skip seq-owned channels; stop/load re-asserts part programs.** While the sequencer plays, it owns the program on the channels it uses (it re-asserts per note-on anyway once `seq_channel_program` resets to 255); a rebuild must not clobber that. When playback stops or a new file loads, the sequencer releases ownership and each part's program is re-sent so live keys get their own timbre back. The deeper issue — sequencer file channels and part channels sharing one rustysynth channel space (both write 0xC0 to the same channel; a seq program change makes the live part sound like the seq patch until the part re-asserts) — is a channel-allocation design question and stays in TECHDEBT; this plan fixes the cache desync only. Known residual: if the user calls `SetPartSf2Program` on a channel the seq currently owns, the seq's dedup cache goes stale until its next actual program change; acceptable, documented here.

---

## Test Cases (from the requirement)

Requirement: "A MIDI file's program changes must not permanently desync the live parts' programs across SF2 rebuilds; stopping/loading a MIDI file must restore the parts' programs and must leave sustain state coherent — no note may keep sustaining after its key-off because a transport event force-lifted only half the pedal state."

Written BEFORE implementation. Each task is executed by a FRESH subagent that receives ONLY this plan document.

1. `midi_seq_stop_lifts_sustain_atom` — pedal down, note held by pedal, `MidiSeqPlay{playing:false}` → sustain atom OFF and pedal-held note released. (RED before Task 1: today the atom stays ON.)
2. `midi_seq_stop_stops_sustaining_new_notes` — after the stop, a NEW note must not be held after its key-off (the pedal was force-lifted everywhere). (RED before Task 1 with an audible peak: `part.sustain_pedal` is stuck at 1.0, so `Part::note_off` (mod.rs:824) parks the new note in `sustained_notes` forever.)
3. `midi_seq_load_lifts_sustain_and_stops_sustaining_new_notes` — same two assertions through the `MidiSeqLoad` path. (RED before Task 1.)
4. `midi_seq_stop_clears_part_sustain_state` (unit, src/synth/tests.rs) — after the stop: atom OFF, `part.sustain_pedal == 0.0`, no `sustained_notes` bits. (RED before Task 1 on the flag asserts.)
5. `seq_program_set_marks_channel_and_leaves_part_cache` (unit, src/synth/sampler.rs tests) — `set_part_program(0,5,0)` then `seq_program_set(0,40)` → `seq_channel_program[0]==40`, `seq_channel_used[0]` true, `part_program[0]` still 5, second identical `seq_program_set` dedups. (Compile-RED before Task 2: field does not exist.)
6. `block_size_change_resets_seq_program_cache` (unit, sampler.rs tests) — `seq_program_set(2,60)`, then `ControlEvent::SetSf2BlockSize{16}` → `seq_channel_program[2]==255` (fresh synth: the seq must re-assert per note-on) and `seq_channel_used[2]` still true (a later stop must still re-assert part programs). (RED before Task 3: today `set_block_size` never resets the cache — value stays 60, so the sequencer never re-sends after a block-size change and plays with whatever the part restore put on the channel.)
7. `seq_stop_resets_seq_program_ownership` / `seq_load_resets_seq_program_ownership` (unit, sampler.rs tests) — through `MidiSeqPlay{false}` / `MidiSeqLoad`, both seq caches reset. (RED before Task 4: caches untouched today.)
8. Existing `tests/sustain_audit.rs` suite stays green — in particular `all_notes_off_releases_pedal_held_notes` (its atom assertion is the model for test 1) and `cc123_panic_releases_pedal_held_notes`.
9. `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test --features rt-assert --test rt_safety`, `cargo deny check` all pass.

Not testable without a real .sf2 (code-review only, anchors below): 0xC0 actually reaching rustysynth on the right channels; the restore skip keeping the seq program sounding on a shared channel across a rebuild; `seq_channels_release`'s re-assert being audible to live keys after stop.

## Negative Matrix

- **Missing/out-of-range index:** `seq_program_set(ch >= 16)` — writes nothing to the caches and does NOT send the message (new guard; the old code sent it unguarded — file channels are 0-15 so no practical change). `send_part_programs` iterates `part_channel` (fixed 16) with `Some`-filter and `part_sf2` check — no OOB. `MidiSeqLoad` with a data box whose track list is empty → `seq_channels_release` still runs.
- **Zero/empty:** stop/load with no SF2 parts → `send_part_programs` iterates 16 `None` channels, sends nothing; `set_sustain(0)` on an idle engine is idempotent (same call the CC120/123 arm makes on silent engines today, covered by existing sustain_audit tests).
- **Duplicate/concurrent:** repeated `MidiSeqPlay{false}` / `MidiSeqLoad` → `seq_channels_release` is idempotent (arrays re-zeroed, re-assert re-sends same program). Repeated `seq_program_set` with the same program → dedup early-return, cache unchanged.
- **Ordering:** stop/load path order is `set_sustain(0)` (lifts atom + part flags + releases pedal-held DSP voices) → existing `voice.note_off` loop (releases key-held actives; re-arming a releasing voice is the known-harmless no-op the carried-bugfixes plan documents) → `sampler.all_notes_off()` (CC64=0 + CC123 to rustysynth) → `seq_channels_release()` (re-assert part programs last, so nothing overwrites them afterwards).
- **Clock skew:** n/a — single audio thread; all state changes happen inside `handle_control` on the audio thread.
- **External-call timeout:** none — no new cross-thread calls; `handle_control` return values marked `#[must_use]` are still `let _ =`-ed by callers per house convention (`CountingProducer` counts drops).
- **Stale used-mask after unload:** `UnloadKeysSoundFont` does not touch `seq_channel_used` — intentional: a later stop must still clear it; the mask alone sends no MIDI.

---

### Task 0: Branch

- [ ] From master:
```bash
cd /home/dima/Projects/mini_midi_synth
git checkout master && git pull
git checkout -b fix-audio-state-coherence
```

---

### Task 1: Sustain-atom divergence on MidiSeqLoad / MidiSeqPlay-stop

**Facts (anchor-verified):** `set_sustain` (src/synth/mod.rs:1387-1417) is the ONLY coherent writer: it stores the engine `sustain` atom (mod.rs:1389), sends CC64 via `sampler.sustain_pedal` (mod.rs:1391, which sets `SamplerEngine.sustain`, sampler.rs:409-415), sets `part.sustain_pedal` (mod.rs:1393) and on release clears `sustained_notes` / `note_stack` / `latch_held` and releases pedal-held voices. Two seq paths bypass it:
- `ControlEvent::MidiSeqLoad` (mod.rs:2284-2306): `self.sampler.all_notes_off()` at mod.rs:2287 — sampler.rs:498-507 sets `SamplerEngine.sustain = false` and sends CC64=0 + CC123, but the engine atom, `part.sustain_pedal` and `sustained_notes` stay ON.
- `ControlEvent::MidiSeqPlay { playing:false }` (mod.rs:2307-2320): same call at mod.rs:2313.
Both paths then `voice.note_off()` every active DSP voice, so the currently sounding notes do stop — the divergence is in the flags: the next key-off on a DSP part keeps sustaining (mod.rs:824 gates on `part.sustain_pedal > 0.5`), and the GUI sustain atom reads stuck ON, until a real CC64 edge arrives. The CC120/123 arm (mod.rs:1798-1823) and `ControlEvent::AllNotesOff` (mod.rs:2162-2187) already do this correctly via `set_sustain(0)` at mod.rs:1802/2163 — this task gives the seq paths the same treatment. No arpeggiator/choke handling is added here: the current seq paths deliberately do not touch those, and this plan does not change that scope.

- [ ] Step 1. Add failing tests to `tests/sustain_audit.rs` (append at end of file, after `rec_on_stopped_transport_auto_plays`, line 552):
```rust
// ── Sustain-atom coherence on MIDI-seq stop/load ───────────────

fn empty_seq_data() -> Box<mini_midi_synth::synth::midi_player::MidiSeqData> {
    Box::new(mini_midi_synth::synth::midi_player::MidiSeqData {
        ticks_per_beat: 480,
        total_ticks: 480,
        events: Vec::new(),
        track_names: Vec::new(),
        track_channels: Vec::new(),
    })
}

#[test]
fn midi_seq_stop_lifts_sustain_atom() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held by pedal");
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    assert!(!e.sustain_atom().load(Ordering::Relaxed),
        "seq stop left sustain atom ON");
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "seq stop left pedal-held note ringing (peak {after})");
}

#[test]
fn midi_seq_stop_stops_sustaining_new_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    // The stop force-lifted the pedal everywhere: a NEW note must not be
    // held after its key-off (no new CC64 edge arrives in between).
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "seq stop left DSP sustaining new notes (peak {after})");
}

#[test]
fn midi_seq_load_lifts_sustain_and_stops_sustaining_new_notes() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE);
    let _ = e.handle_control(ControlEvent::MidiSeqLoad { data: empty_seq_data() });
    assert!(!e.sustain_atom().load(Ordering::Relaxed),
        "seq load left sustain atom ON");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 62 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "seq load left DSP sustaining new notes (peak {after})");
}
```
(`Ordering` is already imported at the top of the file, line 8. `sustain_atom()` is pub, mod.rs:1377-1379.)
- [ ] Step 2. Run (RED):
```bash
cargo test --test sustain_audit midi_seq
```
Expected: all three FAIL — `midi_seq_stop_lifts_sustain_atom` and `midi_seq_load_...` with `"left sustain atom ON"`; `midi_seq_stop_stops_sustaining_new_notes` with `"left DSP sustaining new notes (peak ...)"` (peak ≈ sustain level: `part.sustain_pedal` stuck at 1.0 parks note 62 in `sustained_notes`, mod.rs:824-828).
- [ ] Step 3. Add the unit-coherence test to `src/synth/tests.rs` (append at end of file; the module has `use super::*` and sees private `Part` fields):
```rust
// ── Sustain-atom coherence on MIDI-seq stop ────────────────────

#[test]
fn midi_seq_stop_clears_part_sustain_state() {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 64, value: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..50 { e.tick(); }
    assert!(e.parts[0].sustain_pedal > 0.5, "pedal did not engage the part flag");
    let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
    assert!(!e.sustain_atom().load(std::sync::atomic::Ordering::Relaxed),
        "seq stop left sustain atom ON");
    assert_eq!(e.parts[0].sustain_pedal, 0.0, "part pedal flag stuck ON after seq stop");
    assert!(!e.parts[0].sustained_notes.iter().any(|&s| s),
        "sustained_notes stuck ON after seq stop");
}
```
- [ ] Step 4. Run (RED on the two flag asserts):
```bash
cargo test --lib midi_seq_stop_clears_part_sustain_state
```
Expected FAIL: `"part pedal flag stuck ON after seq stop"`.
- [ ] Step 5. Implement — src/synth/mod.rs, `ControlEvent::MidiSeqLoad` arm. Replace lines 2285-2287:
```rust
                // The old file's sounding notes will never see their note-offs
                // from the new event stream.
                self.sampler.all_notes_off();
```
with:
```rust
                // The old file's sounding notes will never see their note-offs
                // from the new event stream. Lift sustain everywhere FIRST:
                // all_notes_off alone clears only the sampler's pedal state —
                // the engine atom, part.sustain_pedal and sustained_notes
                // would stay ON and DSP would keep sustaining after key-offs
                // until the next CC64 edge (same ordering as the CC120/123 arm).
                self.set_sustain(0);
                self.sampler.all_notes_off();
```
- [ ] Step 6. src/synth/mod.rs, `ControlEvent::MidiSeqPlay` arm. Replace lines 2311-2313:
```rust
                if !playing {
                    self.midi_player.reset_position();
                    // All notes off on all engines
                    self.sampler.all_notes_off();
```
with:
```rust
                if !playing {
                    self.midi_player.reset_position();
                    // Lift sustain everywhere first (see MidiSeqLoad): the
                    // engine atom and part flags must agree with the sampler.
                    self.set_sustain(0);
                    // All notes off on all engines
                    self.sampler.all_notes_off();
```
- [ ] Step 7. Run (GREEN):
```bash
cargo test --lib midi_seq_stop_clears_part_sustain_state
cargo test --test sustain_audit
```
Expected: new unit test `ok`; the whole sustain_audit suite `ok` (the three new tests now pass; `all_notes_off_releases_pedal_held_notes` proves the pre-existing `ControlEvent::AllNotesOff` path is unchanged).
- [ ] Step 8. RT-safety + lint:
```bash
cargo test --features rt-assert --test rt_safety
cargo clippy --all-targets -- -D warnings
```
Expected: both pass (`set_sustain` is the existing alloc-free path already used in the audio-thread CC120/123 arm; `Vec::clear` keeps capacity).
- [ ] Step 9. Commit:
```bash
git add src/synth/mod.rs src/synth/tests.rs tests/sustain_audit.rs
git commit -m "fix: seq stop/load lifted only the sampler's pedal state — run set_sustain(0) so atom, part flags and SF2 stay coherent"
```

---

### Task 2: `seq_channel_used` ownership mask in `SamplerEngine`

**Facts (anchor-verified):** `SamplerEngine` (src/synth/sampler.rs:188-213) tracks `seq_channel_program: [u8; 16]` (sampler.rs:203, init `[255; 16]` at sampler.rs:228) as a per-channel dedup cache for `seq_program_set` (sampler.rs:465-474): it sends 0xC0 on the file channel and skips the message when the cache already holds the program. There is no record of WHICH channels the sequencer has claimed, so the rebuild restores (`load_keys_soundfont` sampler.rs:242-250, `set_block_size` sampler.rs:288-295) cannot skip them. Note these tests live in a new `#[cfg(test)] mod tests` at the bottom of src/synth/sampler.rs — the cache fields are private to the `sampler` module and src/synth/tests.rs (sibling module) cannot see them.

- [ ] Step 1. Append to src/synth/sampler.rs (end of file):
```rust
// ── Program-cache ownership tests (no .sf2: keys.synth is None, every
//    keys.midi send is a no-op; the arrays below ARE the contract) ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{ControlEvent, SynthEngine};

    #[test]
    fn seq_program_set_marks_channel_and_leaves_part_cache() {
        let mut s = SamplerEngine::new(48000.0);
        s.set_part_program(0, 5, 0);
        assert_eq!(s.part_program[0], 5);
        s.seq_program_set(0, 40);
        assert_eq!(s.seq_channel_program[0], 40, "seq cache not updated");
        assert!(s.seq_channel_used[0], "seq channel 0 not marked as used");
        assert_eq!(s.part_program[0], 5, "seq program overwrote the part cache");
        // dedup: an identical program must not re-send / change anything
        s.seq_program_set(0, 40);
        assert_eq!(s.seq_channel_program[0], 40);
        // out-of-range channel: no cache write, no panic
        s.seq_program_set(16, 7);
        assert!(s.seq_channel_used.iter().skip(1).all(|&u| !u),
            "out-of-range channel polluted the used mask");
    }
}
```
- [ ] Step 2. Run (RED — compile error, field missing):
```bash
cargo test --lib seq_program_set_marks_channel
```
Expected: `error[E0609]: no field 'seq_channel_used' on type 'SamplerEngine'` (compilation failure IS the red state for a new-field TDD step).
- [ ] Step 3. Implement. In src/synth/sampler.rs, after the `seq_channel_program` field declaration (after line 203):
```rust
    /// Channels whose GM program the MIDI-file sequencer currently owns
    /// (claimed by `seq_program_set`, released by `seq_channels_release`).
    /// A rustysynth rebuild must NOT restore part programs onto these — the
    /// sequencer re-asserts its own program on the next note-on because
    /// `seq_channel_program` resets to 255 on rebuild.
    seq_channel_used: [bool; 16],
```
In `SamplerEngine::new`, after `seq_channel_program: [255; 16], // 255 = not yet initialised` (line 228):
```rust
            seq_channel_used: [false; 16],
```
Replace `seq_program_set` (sampler.rs:465-474) with:
```rust
    /// MIDI sequencer: set GM program for a channel of the keys synth.
    /// Skips the MIDI message if the program is already set (avoids per-note
    /// overhead). Marks the channel as seq-owned until `seq_channels_release`
    /// so a rebuild's part-restore skips it.
    pub fn seq_program_set(&mut self, channel: u8, program: u8) {
        let ch = channel as usize;
        if ch < 16 {
            self.seq_channel_used[ch] = true;
            if self.seq_channel_program[ch] == program {
                return; // already set, no-op
            }
            self.seq_channel_program[ch] = program;
        }
        self.keys.midi(channel as i32, 0xC0, program as i32, 0);
    }
```
(Behavior note: the old code sent the 0xC0 unguarded for `ch >= 16`; the new guard skips it. SMF channels are 0-15, so no practical change — and sending channel 16 to rustysynth was never meaningful.)
- [ ] Step 4. Run (GREEN):
```bash
cargo test --lib seq_program_set_marks_channel
cargo test
```
Expected: new test `ok`; full suite `ok` (no existing caller behavior changed — `seq_program_set` is called from mod.rs:2332, mod.rs:2658, mod.rs:2679, all `ch < 16`).
- [ ] Step 5. Commit:
```bash
git add src/synth/sampler.rs
git commit -m "feat: SamplerEngine tracks which channels the sequencer owns (seq_channel_used)"
```

---

### Task 3: Rebuild restores skip seq-owned channels; `set_block_size` resets the seq dedup cache

**Facts (anchor-verified):** Two restore sites duplicate the part-program loop: `load_keys_soundfont` (sampler.rs:238-256 — resets `seq_channel_program` at :239, rebuilds at :240, restores at :242-250) and `set_block_size` (sampler.rs:281-306 — rebuilds keys at :286, restores at :288-295, re-applies release scale at :296-298). Two defects: (a) both restores send `part_program[i]` onto channels the sequencer currently owns, clobbering the seq program mid-playback (the TECHDEBT line 25 symptom); (b) `set_block_size` never resets `seq_channel_program` even though the rebuild created a fresh `Synthesizer` — after a block-size change the dedup cache claims programs are still set that the new synth never received, so the sequencer never re-asserts them and plays with whatever the part restore put on the channel. Refactor: one shared helper, parameterized by the skip.

- [ ] Step 1. Add the failing test to the `#[cfg(test)] mod tests` block in src/synth/sampler.rs (inside `mod tests`, after the Task 2 test):
```rust
    #[test]
    fn block_size_change_resets_seq_program_cache() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(2, 60);
        let _ = e.handle_control(ControlEvent::SetSf2BlockSize { size: 16 });
        assert_eq!(e.sampler.seq_channel_program[2], 255,
            "keys rebuild left a stale seq dedup cache — the sequencer never re-asserts");
        assert!(e.sampler.seq_channel_used[2],
            "used mask must survive a rebuild so a later stop re-asserts part programs");
        // Same guarantee through the sample-rate path (routes through load_keys_soundfont)
        e.sampler.seq_program_set(5, 33);
        let _ = e.handle_control(ControlEvent::SetSf2BlockSize { size: 32 });
        assert_eq!(e.sampler.seq_channel_program[5], 255,
            "second rebuild kept a stale seq dedup cache");
    }
```
- [ ] Step 2. Run (RED):
```bash
cargo test --lib block_size_change_resets_seq_program_cache
```
Expected FAIL: `"keys rebuild left a stale seq dedup cache ..."` — `seq_channel_program[2]` is still 60 (`set_block_size` never touches it today).
- [ ] Step 3. Implement in src/synth/sampler.rs. Insert after `part_channel_of` (after line 350):
```rust
    /// Re-send bank+program for every SF2-enabled part that owns a channel.
    /// With `skip_seq_channels`, channels currently owned by the MIDI-file
    /// sequencer are left alone: the sequencer owns the program there while
    /// it plays, and re-asserts it per note-on after a rebuild (its dedup
    /// cache resets to 255). RT-safe: fixed loops, existing `keys.midi`.
    fn send_part_programs(&mut self, skip_seq_channels: bool) {
        for (i, ch) in self.part_channel.iter().enumerate() {
            let Some(ch) = ch else { continue };
            if !self.part_sf2[i] { continue; }
            if skip_seq_channels && self.seq_channel_used[ch] { continue; }
            self.keys.midi(ch as i32, 0xB0, 0, self.part_bank[i] as i32);
            self.keys.midi(ch as i32, 0xC0, self.part_program[i] as i32, 0);
        }
    }
```
In `load_keys_soundfont` replace the restore loop (lines 241-250) with:
```rust
        // Restore programs on the new synth — but not on channels the
        // sequencer currently owns (it re-asserts per note-on; the cache
        // reset above makes that happen).
        self.send_part_programs(true);
```
(`self.seq_channel_program = [255; 16];` at line 239 stays, BEFORE the rebuild, exactly where it is.)
In `set_block_size` replace the restore loop + release-scale block (lines 287-298) with:
```rust
            // Rebuild made a fresh Synthesizer: the sequencer's programs are
            // gone with it — reset the dedup cache so seq note-ons re-send
            // them (this path used to forget the reset), and restore part
            // programs only on channels the sequencer does not own.
            self.seq_channel_program = [255; 16];
            self.send_part_programs(true);
            // Rebuild made a fresh Synthesizer — re-apply the release scale
            if let Some(synth) = &mut self.keys.synth {
                synth.set_release_time_scale(self.release_scale);
            }
```
(`self.block_size = size;` and the `if let Some(sf) = self.keys.soundfont.clone()` guard above stay unchanged; the drums branch and `apply_sustain()` below stay unchanged. `set_sample_rate` routes through `load_keys_soundfont` (sampler.rs:313), so it inherits the fix — the second rebuild in that path resets the cache again, harmless.)
- [ ] Step 4. Run (GREEN):
```bash
cargo test --lib block_size_change_resets_seq_program_cache
cargo test --lib seq_program_set_marks_channel
cargo test
```
Expected: all `ok`. (`send_part_programs` is behavior-identical to the old loops when `seq_channel_used` is all-false, which is the state every existing test sees.)
- [ ] Step 5. Lint + RT-safety:
```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```
Expected: pass (helper is a fixed 16-iteration loop over existing arrays).
- [ ] Step 6. Commit:
```bash
git add src/synth/sampler.rs
git commit -m "fix: SF2 rebuilds clobbered seq-owned channel programs and set_block_size forgot the seq cache reset — shared send_part_programs(skip_seq)"
```

---

### Task 4: Release seq ownership on stop/load and re-assert part programs

**Facts (anchor-verified):** After Tasks 2-3 the mask exists but nothing ever clears it: once a file has claimed a channel, that channel's part program is never restored by a rebuild again, and after playback stops the part keeps sounding like the seq patch until an explicit `SetPartSf2Program` (mod.rs:2231-2235). The stop and load arms are `ControlEvent::MidiSeqPlay { playing:false }` (mod.rs:2307-2320, after `sampler.all_notes_off()` at :2313, plus Task 1's `set_sustain(0)`) and `ControlEvent::MidiSeqLoad` (mod.rs:2284-2306, after `sampler.all_notes_off()` at :2287). `MidiSeqSetTrackInstrument` (mod.rs:2327-2335) deliberately does NOT release — it re-claims via `seq_program_set` at :2332. The CC120/123 arm and `ControlEvent::AllNotesOff` do NOT release either: CC123 does not alter program state, and if playback keeps running the sequencer must keep its claims.

- [ ] Step 1. Add the failing tests to the `#[cfg(test)] mod tests` block in src/synth/sampler.rs:
```rust
    #[test]
    fn seq_stop_resets_seq_program_ownership() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(3, 90);
        let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
        assert_eq!(e.sampler.seq_channel_program[3], 255,
            "stop did not reset the seq program cache");
        assert!(!e.sampler.seq_channel_used[3],
            "stop did not clear the seq used mask");
    }

    #[test]
    fn seq_load_resets_seq_program_ownership() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(3, 90);
        let data = Box::new(crate::synth::midi_player::MidiSeqData {
            ticks_per_beat: 480,
            total_ticks: 480,
            events: Vec::new(),
            track_names: Vec::new(),
            track_channels: Vec::new(),
        });
        let _ = e.handle_control(ControlEvent::MidiSeqLoad { data });
        assert_eq!(e.sampler.seq_channel_program[3], 255,
            "load did not reset the seq program cache");
        assert!(!e.sampler.seq_channel_used[3],
            "load did not clear the seq used mask");
    }

    #[test]
    fn seq_ownership_survives_rebuild_but_not_stop() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(1, 70);
        let _ = e.handle_control(ControlEvent::SetSf2BlockSize { size: 24 });
        assert!(e.sampler.seq_channel_used[1], "rebuild must not release ownership");
        let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
        assert!(!e.sampler.seq_channel_used[1], "stop must release ownership");
    }
```
- [ ] Step 2. Run (RED):
```bash
cargo test --lib seq_
```
Expected: the three new tests FAIL (`seq_stop_resets...`, `seq_load_resets...`: today stop/load leave `seq_channel_program[3] == 90` and `seq_channel_used[3] == true`; `seq_ownership_survives_rebuild_but_not_stop` fails on its LAST assert, the stop one — the rebuild asserts are already green from Task 3). Earlier `seq_` tests from Tasks 2-3 stay green.
- [ ] Step 3. Implement. In src/synth/sampler.rs, insert after `send_part_programs` (added in Task 3):
```rust
    /// MIDI-file playback stopped or a new file loaded: the sequencer no
    /// longer owns any channel's program. Reset both seq caches and re-assert
    /// every part's program so live keys get their own timbre back (a seq
    /// program change on a shared channel used to desync `part_program[]`
    /// until an explicit SetPartSf2Program). RT-safe: array writes + the
    /// fixed `send_part_programs` loop.
    pub fn seq_channels_release(&mut self) {
        self.seq_channel_program = [255; 16];
        self.seq_channel_used = [false; 16];
        self.send_part_programs(false);
    }
```
In src/synth/mod.rs, `ControlEvent::MidiSeqLoad` arm — after the line `self.sampler.all_notes_off();` (which Task 1 placed under the `set_sustain(0)` call, mod.rs ~2290):
```rust
                // The sequencer no longer owns any channel program
                self.sampler.seq_channels_release();
```
In the `ControlEvent::MidiSeqPlay` arm, inside `if !playing {`, after `self.sampler.all_notes_off();` (mod.rs ~2319):
```rust
                    // Playback stopped: give the shared channels back to the parts
                    self.sampler.seq_channels_release();
```
- [ ] Step 4. Run (GREEN):
```bash
cargo test --lib seq_
cargo test --test sustain_audit
cargo test
```
Expected: all sampler-cache tests `ok`; full suites `ok` (`seq_channels_release` on an engine with no SF2 parts sends nothing — `send_part_programs` filters on `part_channel` `Some` + `part_sf2`).
- [ ] Step 5. Lint + RT-safety:
```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```
Expected: pass (no allocation; no locks; runs in `handle_control` on the audio thread like the rest of the arm).
- [ ] Step 6. Commit:
```bash
git add src/synth/sampler.rs src/synth/mod.rs
git commit -m "fix: seq stop/load now releases channel program ownership and re-asserts part programs (seq_channels_release)"
```

---

### Task 5: Full gate

- [ ] Step 1. Run the whole gate from CLAUDE.md:
```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```
Expected: all four pass; no new advisories (no dependency changes in this plan).
- [ ] Step 2. Verify the tree is clean:
```bash
git status --short
```
Expected: empty (all four fixes committed by their tasks). Update docs/TECHDEBT.md carried-over list only if the user asked for it in the same session — the entries at lines 25 and 27 are resolved by this plan.

---

## Code-review checklist (SF2-only behavior, untestable without .sf2)

- `load_keys_soundfont` order: reset `seq_channel_program` → rebuild → `send_part_programs(true)` → `apply_sustain()` → release scale. The reset MUST stay before the rebuild so the very first post-rebuild seq note-on re-sends its program (mod.rs:2656-2660 calls `seq_program_set` before every SF2 `seq_note_on`; the ProgramChange follow at mod.rs:2676-2681 re-claims too).
- `send_part_programs(false)` in `seq_channels_release` is the only place part programs are re-sent WITHOUT the seq skip — correct, because ownership was just cleared.
- Known residual (documented, out of scope): user calls `SetPartSf2Program` on a seq-owned channel mid-playback → part program sounds until the seq's next actual program change (dedup cache stale). Full fix = disjoint sequencer channel allocation; remains in TECHDEBT.
- `set_sample_rate` (sampler.rs:309-321) calls `load_keys_soundfont` (reset + skip restore) and later `apply_sustain` — double reset is harmless.
