# Console GUI Polish Plan (pad-remap modals global, per-part root-only, static sustain-release)

> anchor-checked: 2026-10-29 against branch `worktree-carried-fixes` @ 492dea2 (5 carried-fix commits on master ccdd519)
> status: executed 2026-10-29 (Plan D: #11 + #6 complete earlier; #7 + review follow-ups this pass)
> Source: docs/TECHDEBT.md items #11, #6, #7.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking. Each task is executed by a FRESH subagent that receives ONLY this plan document.

**Goal:** (1) Pad-remap dialogs work on every tab, not only Drums (TECHDEBT #11); (2) per-part "root only" mode — the part sounds only the lowest currently-held key, for bass splits (TECHDEBT #6); (3) custom sustain-release returns as static settings: DSP pedal-up release duration + SF2 release-time scale (TECHDEBT #7).

**Architecture:** All three items touch different layers.
- **#11** — pure GUI move. Both remap modals (`egui::Modal`) are drawn inside `App::draw_drum_sequencer` (src/gui/drums.rs:87-230), which is only called from the `show_drums` branch of the CentralPanel (src/gui/mod.rs:1162-1165). They are driven by App flags (`show_drum_remap`, mod.rs:307; `remap_pad`, mod.rs:309) that are set by pad clicks in the bottom panel — reachable from any tab. Moving the two modal blocks into a method called from `update` with the root `ctx` makes them render regardless of the active tab. `egui::Modal` is the codebase's modal primitive (drums.rs:99/189); toasts already draw directly on `ctx` before the CentralPanel (mod.rs:1122-1145) — the insertion point mirrors that pattern.
- **#6** — engine feature. Note routing to parts happens at eight dispatch loops in src/synth/mod.rs (live MIDI ons/offs ~1660-1723, arp output in tick_block ~2713-2735, looper replay ~2740-2759, MIDI-file player `DspLayer0` ~2672-2688), each with the same shape: guard → range/velocity check → `part.note_on`/`part.note_off` (DSP) or `sampler.note_on/off(i, transposed)` (SF2). The gate lives in two new `Part` state machines (`root_on`/`root_off`, tracking a fixed `[bool; 128]` held-mask + `Option<u8>` active root) behind two new engine helpers `route_note_on`/`route_note_off` that all sites call. No allocation (fixed arrays), so the audio-thread rt-invariants hold.
- **#7** — history is fully resolved, no design reduction needed. The DSP mechanism existed in commit 9a200a4 (`Envelope::note_off_for`, `Voice::note_off_for`, `Part::note_off_for`, `SynthEngine.sustain_release: Option<f32>`, `ControlEvent::SetSustainRelease`) and was later removed by the revert 59ed413 only because the *fader-position-coupled* experiment (SetSustainSlider) was unplayable — the static-duration override itself was fine. The SF2 mechanism from 1d11ee2 is still present and dead: vendor `Synthesizer::set_release_time_scale` (vendor/rustysynth/src/synthesizer.rs:549), `SamplerEngine::set_release_time_scale` (src/synth/sampler.rs:418), `ControlEvent::SetSf2ReleaseScale` (src/synth/mod.rs:234, handler 2270) — nothing in the GUI sends it. This plan restores the DSP side (adapted to the current keys_down-aware `set_sustain`, src/synth/mod.rs:1390-1424) and wires static GUI controls for both engines.

**Tech Stack:** Rust; egui (`egui::Modal`, `egui::DragValue`); test harness `tests/sustain_audit.rs` (`engine_with_init_patch` / `render_peak` / `settle` / `pedal_down` / `pedal_up`, lines 10-70); unit tests in `src/synth/tests.rs` (child module — reads private `Part` fields, see tests.rs:547-558 for the pattern); envelope-field unit test in a new `#[cfg(test)] mod tests` inside src/synth/envelope.rs (fields are private to that module, sibling modules cannot reach them); gates: `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test --features rt-assert --test rt_safety`, `cargo deny check`. No new dependencies (cargo deny unaffected).

**Decision #6 (ВАЖНО: play_mode 3 (Latch) — обходить gate в root_on: тап-офф при root-only+ latch может быть Suppress'нут; в root_on ранний `if self.play_mode == 3 { return normal dispatch }`) — "lowest-note-priority", per part, both engines.**
While `root_only` is ON for a part, every note-on routed to that part is sounded only if it is LOWER than the currently sounding root (or there is none). A lower key replaces the root: the old root's voice is released outright (pedal does not hold a replaced root — a bass layer must not pile up sustained roots) and the new lowest key starts a voice with the part's current `last_velocity`. Releasing the root key re-keys to the next-lowest still-held key. Duplicate note-on for the current root is suppressed (no double voice). A note-off for a non-root note only updates the held mask.
- **Alternative rejected — chord.rs root detection:** `chord::detect` (src/chord.rs:32-79) returns a chord *name* for >= 3 distinct pitch classes and None otherwise (chord.rs:40, 97-103); it yields a pitch class, not an octave note, and goes silent on clusters/2-note intervals. A bass layer that only fires on detected triads is fragile for live playing.
- **Alternative rejected — "poly-low play mode" reuse:** `play_mode == 5` (mod.rs:677-684) only changes voice *stealing*; every key still sounds. Not the same feature.
- Root-only interacts with `play_mode` as: poly modes (0/4/5/6) fully gated; mono modes (1/2) already sound one note — the gate is a no-op there (harmless); latch (3) is NOT gated (latch toggles on note-on; the gate would break the toggle contract).
- Toggle semantics: `SetPartRootOnly` resets `root_held`/`root_active` in both directions — a stale `root_active` across the toggle would trigger a phantom re-key on the next note-off. Part-disable, part-mute and panic paths clear the same state alongside the existing `keys_down`/`note_stack` clears.
- SF2 limitation: SF2 tracking cannot be audio-tested without a real .sf2 (`SamplerEngine` starts with `keys.synth == None`, making every SF2 path a no-op) — same concession as the carried-bugfixes plan; verified by code review against the anchors here.

**Decision #7 — two static global settings, restored from history.**
- DSP: **Pedal-up release** ("SR") — toggle + slider 0.05..=8.0 s (default 0.3 s when enabled), sends `SetSustainRelease { seconds: Option<f32> }`. While ON, every pedal-up release of a pedal-held (not key-held) voice takes exactly this duration instead of the patch `amp_release`. Persisted as `config.ui.sustain_release: Option<f32>` (None = off), restored at startup. CC64 is never read into it.
- SF2: **Release scale** — DragValue 0.05..=3.0 (default 1.0 = file defaults), sends the existing `SetSf2ReleaseScale { scale }`. This is the only mechanism the rustysynth vendor patch provides; it scales SF2 release times at voice *start* (synthesizer.rs:245-246) and therefore applies to all SF2 releases, not only pedal-up ones — a documented approximation, kept as an independent control because a single value cannot drive both engines without an arbitrary mapping (deviation from the one-slider wording in TECHDEBT #7, justified by the mechanism audit above). Persisted as `config.ui.sf2_release_scale: f32`.
- Out of scope (per TECHDEBT #7 research): half-pedal continuous scaling, Min/Max CC range mapping, fader-position coupling (reverted in 59ed413 — do NOT reintroduce `SetSustainSlider`).

---

## Test Cases (from the requirement)

Written BEFORE implementation. RED status noted per case.

**#11 (UI — compile + manual coverage; no egui test harness exists in the repo):**
1. With any tab active (Synth params, FX chain, MIDI seq, Looper, Drums), clicking a drawn hardware pad in the bottom panel opens the single-pad remap dialog; the Remap button on the drums panel opens the full-grid dialog — both close via Close button / click-outside (`should_close()`), and state (`drum_remap`, `drum_pad_vel`) persists to config exactly as before (verified manually; the moved code must be a verbatim move with only `ui.ctx()` → `ctx`).
2. The Remap button remains visible ONLY on the drums panel (drums.rs:81-85 unchanged).

**#6 (engine — audio + unit):**
3. `root_only_suppresses_higher_notes` (unit, src/synth/tests.rs) — root-only on, note-on 60 → ≥1 active voice; note-on 64 while 60 held → still exactly 1 active non-releasing voice, `root_active == Some(60)`; note-off 64 → root keeps sounding. (RED before: today both notes sound, 2 active voices.)
4. `root_only_rekeys_to_new_lowest` (unit) — hold 64 (sounds), press 60 → 64's voice releasing, 60's voice active, `root_active == Some(60)`. (RED before.)
5. `root_only_release_of_root_rekeys_next_lowest` (unit) — hold 60 + 64, release 60 → voice active at note 64, 60's voice released, `root_active == Some(64)`. (RED before.)
6. `root_only_rekey_releases_old_root_even_under_pedal` (unit) — pedal down, hold 60, press 64, release 60, pedal up, release 64 → settle → engine silent (no piled-up sustained root). (RED before: the old root would be pedal-held forever.)
7. `root_only_suppressed_note_leaves_no_stuck_voice` (audio, tests/root_only.rs (создать в Task 2: копия харнесса engine_with_init_patch/render_peak/settle из sustain_audit.rs + тест root_only_suppressed_note_leaves_no_stuck_voice: SetPartRootOnly → аккорд → низший звучит, прочие voice_count 0)) — root-only on via `handle_control`, hold 60 + 72, release both, settle → `render_peak < SILENCE`. (GREEN-able only after the gate; guards voice-leak.)
8. `root_only_toggle_resets_tracking` (unit) — sounding root, `SetPartRootOnly { on: false }` then `{ on: true }` → `root_active == None`, `root_held` all false, next note-on sounds normally. (RED before: fields don't exist.)
9. Existing suites stay green: `tests/sustain_audit.rs` (in particular `pedal_up_releases_held_notes`, `multi_held_keys_pedal_up_then_individual_offs` — non-root-only behavior must be byte-identical), `src/synth/tests.rs`.

**#7 (engine — audio + unit):**
10. `pedal_rel_setting_shortens_pedal_up_tail` (tests/sustain_audit.rs, restored from git history 59ed413^ with the event renamed) — `SetSustainRelease { seconds: Some(0.03) }`, pedal-held note, pedal-up → at ~100 ms after release peak < 0.15. (RED before: `SetSustainRelease` doesn't exist — compile error IS the red; after the enum variant lands but before `set_sustain` uses it, the peak stays > 0.15 — that is the behavioral red.)
11. `pedal_rel_off_keeps_full_default_tails` (restored) — SR off, pedal-held note, pedal-up → at ~250 ms peak > 0.15 (init patch `amp_release` = 0.3 s, src/synth/patch_params.rs:187). (GREEN before and after — behavior-unchanged guard.)
12. `envelope_note_off_for_sets_both_release_representations` (unit, new `#[cfg(test)] mod tests` in src/synth/envelope.rs) — after `note_off_for(0.05)`: `is_releasing()`, `phase_step_r == 1/(0.05·sr)` (Linear/Quadratic shapes read this), `release_coeff ∈ (0,1)` (IIR shapes read this), and ~2400 ticks drive output below 1e-3. (RED: method doesn't exist.)
13. `held_key_survives_pedal_up` and `repedal_articulation_keeps_held_key` (existing, sustain_audit.rs:516-551) must stay green — the override releases ONLY voices whose key is not physically held (`!keys_down[src_note]`), same predicate as the current plain path.
14. SF2 scale (manual only): with an SF2 loaded, setting the scale slider shortens sample release tails — no automated test possible without a real .sf2 (same concession as above).

---

## Negative Matrix

- **Missing/out-of-range index:** `SetPartRootOnly { part: 12 }` → `self.parts.get_mut(part)` returns None → no-op (pattern of `SetPartMute`, mod.rs:1993). `route_note_on/off(i, ..)` is only called with `i` from `enumerate()` over `self.parts` or a checked `get_mut` — never a bare index.
- **null/None:** `root_active: Option<u8>` None = no root sounding; `sustain_release: Option<f32>` None = off (patch release) — both handled by explicit match/`if let`, no unwrap anywhere.
- **Zero/empty:** root-only on a part with zero held keys → `root_off` finds no next key, returns None, no voice started; note-on when out of the part's key/velocity range → suppressed before the gate is consulted (root_held NOT set — out-of-range keys must not re-key the bass later); `note_off_for` on an idle envelope early-returns (9a200a4 verbatim).
- **Extreme values:** transposition in the routing helpers uses the existing clamp `(n as i16 + transpose as i16).clamp(0, 127)` (pattern of mod.rs:1662); re-key velocity `(part.last_velocity * 127.0) as u8` ≤ 127 (last_velocity is 0..1, set at mod.rs:585); SR seconds clamped by `t.max(0.02)` inside `note_off_for`; SF2 scale range-bound by the DragValue (0.05..=3.0).
- **Duplicate/concurrent:** duplicate note-on of the current root → `root_on` returns Suppress (no double voice); note-off for a never-pressed key → held mask bit already false, `root_active != Some(raw)` → None, plain release path (no-op); repeated `SetPartRootOnly` toggles → tracking reset each time, idempotent; CC64 during root-only re-key → old root already force-released, `sustained_notes` bit cleared.
- **Clock skew:** n/a — all gate state lives on the single audio thread; GUI only sends boolean events.
- **External-call timeout:** no new cross-thread calls. `SetSustainRelease`/`SetPartRootOnly` are plain enum variants in the existing 512-slot rtrb ring; loss is counted by `CountingProducer` as before (GUI re-pushes on next slider move).
- **rt-safety:** root gate = two fixed `[bool; 128]`/`Option<u8>` on `Part` + stack bitmap in `set_sustain`; no allocation, no lock, no I/O — `rt_safety` must stay green after every engine task.

---

### Task 0: Branch

- [x] From the worktree:
```bash
cd /home/dima/Projects/mini_midi_synth/.claude/worktrees/carried-fixes
git status --short   # must be clean (docs/TECHDEBT.md edit belongs to the caller, stash if dirty)
git checkout -b console-gui-polish
```
Expected: `Switched to a new branch 'console-gui-polish'`, based on worktree-carried-fixes (492dea2).

---

### Task 1: Move remap modals out of the drums tab (TECHDEBT #11)

**Facts:** The full-grid modal is src/gui/drums.rs:87-173 (guarded by `self.show_drum_remap`, Modal id `"drum_remap_dialog"`, drums.rs:99); the single-pad modal is drums.rs:175-230 (guarded by `self.remap_pad`, Modal id `"remap_pad_dialog"`, drums.rs:189). Both currently use `.show(ui.ctx(), ...)`. `draw_drum_sequencer` is called only from mod.rs:1164 (`else if self.show_drums` branch). `update` has `ctx: &egui::Context` (mod.rs:425); the toasts block ends at mod.rs:1145, the CentralPanel starts at mod.rs:1148. The Remap button (drums.rs:81-85) STAYS where it is.

- [x] Step 1. In src/gui/drums.rs add a new method (place it directly after `draw_drum_sequencer` ends):
```rust
impl App {
    /// Pad-remap modals, drawn from the main update loop so pad clicks work
    /// on every tab (TECHDEBT #11). The Remap button lives on the drums panel.
    pub(super) fn draw_drum_remap_modals(&mut self, ctx: &egui::Context) {
        // MOVED VERBATIM from draw_drum_sequencer: the two blocks
        // `if self.show_drum_remap { ... }` (was drums.rs:87-173) and
        // `if let Some(p) = self.remap_pad { ... }` (was drums.rs:175-230),
        // with the single mechanical change `.show(ui.ctx(), ...)` -> `.show(ctx, ...)`.
    }
}
```
Move the two blocks into it exactly as they are (keep the `kit_names`/`kit_notes` locals, `pad_row` closure, Reset/Close handling, `push_drum_remap`/`save_drum_remap`/`push_drum_pad_vel`/`save_drum_pad_vel` calls, the detach-other-pad loop at old drums.rs:218-226, and both `dialog.should_close()` blocks). Delete them from `draw_drum_sequencer`. Note: `egui::Modal::new(...).show(ctx, ...)` is valid — `ui.ctx()` inside update IS this same `ctx`.
- [x] Step 2. In src/gui/mod.rs `update`, insert between the toasts block (ends mod.rs:1145) and the `// Central:` comment (mod.rs:1147):
```rust
        // Pad remap dialogs — global so pad clicks work on any tab (TECHDEBT #11)
        self.draw_drum_remap_modals(ctx);
```
- [x] Step 3. Compile + gates:
```bash
cargo clippy --all-targets -- -D warnings && cargo test --test drum_remap --test drum_pad_vel
```
Expected: no warnings; both test binaries pass (the config push/save behavior is unchanged).
- [x] Step 4. Manual check: `cargo run --release` — on the Synth/FX/MIDI-seq/Looper tabs, click a drawn pad in the bottom panel → single-pad dialog opens and its instrument change takes effect; on the Drums tab the Remap button still opens the full-grid dialog; Close and click-outside dismiss both.
- [x] Step 5. Commit:
```bash
git add src/gui/drums.rs src/gui/mod.rs
git commit -m "gui: pad remap modals render on every tab (moved to update loop)"
```

---

### Task 2: Root-only gate — Part state machine + engine routing (TECHDEBT #6)

**Facts:** `Part` fields end at mod.rs:487 (`cached_lfo_amp_base`); `keys_down` is mod.rs:470; `Part::new` initializer block ends at mod.rs:536 (`cached_lfo_amp_base: 1.0,`); `Part::note_on` starts mod.rs:579, `note_off` mod.rs:793. Live dispatch: velocity==0 loop mod.rs:1659-1671, note-on loop mod.rs:1672-1697, `MidiEvent::NoteOff` loop mod.rs:1711-1723. Arp: off mod.rs:2713-2723, on mod.rs:2724-2735. Looper replay mod.rs:2739-2759. MIDI-file `DspLayer0` note-on mod.rs:2672-2677, note-off mod.rs:2693-2697. Cleanup sites that clear `note_stack`/`latch_held`: mod.rs:1765-1768, 1822-1826, 1914-1917, 1984-1987 (SetPartEnabled), 2007-2010 (SetPartMute). `last_velocity` is 0..1 (mod.rs:585).

- [x] Step 1. Write the failing unit tests in src/synth/tests.rs (append after `dispatch_arp_note_off_releases_dsp_voice_with_transpose_set`, tests.rs:581):
```rust
// ── Root-only mode (TECHDEBT #6) ───────────────────────────────

fn root_only_engine() -> SynthEngine {
    let mut e = SynthEngine::new(48000.0);
    let patches = crate::preset::load_all_patches();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patches[0]));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRootOnly { part: 0, on: true });
    e
}

#[test]
fn root_only_suppresses_higher_notes() {
    let mut e = root_only_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active), "lowest note did not sound");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].voices.iter().filter(|v| v.active && !v.is_releasing()).count(), 1,
        "higher note was not suppressed");
    assert_eq!(e.parts[0].root_active, Some(60));
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 64 }); // non-root off: no-op
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, Some(60), "non-root note-off re-keyed the root");
}

#[test]
fn root_only_rekeys_to_new_lowest() {
    let mut e = root_only_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 60),
        "new lowest did not start a voice");
    assert!(e.parts[0].voices.iter().filter(|v| v.active && v.is_releasing() && v.src_note == 64).count() >= 1,
        "old root was not released");
    assert_eq!(e.parts[0].root_active, Some(60));
}

#[test]
fn root_only_release_of_root_rekeys_next_lowest() {
    let mut e = root_only_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing() && v.note == 64),
        "next-lowest key did not re-key after root release");
    assert_eq!(e.parts[0].root_active, Some(64));
}

#[test]
fn root_only_toggle_resets_tracking() {
    let mut e = root_only_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    let _ = e.handle_control(ControlEvent::SetPartRootOnly { part: 0, on: false });
    let _ = e.handle_control(ControlEvent::SetPartRootOnly { part: 0, on: true });
    assert_eq!(e.parts[0].root_active, None, "toggle did not reset root_active");
    assert!(!e.parts[0].root_held.iter().any(|&h| h), "toggle did not reset root_held");
    // A stale root would have swallowed this note:
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 100 });
    for _ in 0..100 { e.tick(); }
    assert!(e.parts[0].voices.iter().any(|v| v.active && !v.is_releasing()),
        "note after toggle was suppressed by stale tracking");
}

#[test]
fn root_only_out_of_range_key_does_not_join_held_mask() {
    let mut e = root_only_engine();
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 48, max_note: 70 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 30, velocity: 100 }); // out of range
    for _ in 0..100 { e.tick(); }
    assert!(!e.parts[0].root_held[30], "out-of-range key joined the held mask");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    for _ in 0..100 { e.tick(); }
    assert_eq!(e.parts[0].root_active, None, "out-of-range key re-keyed after root off");
}
```
```bash
cargo test --lib root_only 2>&1 | tail -5
```
Expected: compile error on `SetPartRootOnly` / `root_active` / `root_held` — the red.
- [x] Step 2. Add the `ControlEvent` variant in src/synth/mod.rs after `SetPartMute` (mod.rs:173):
```rust
    /// Root-note-only mode (bass split): part sounds only the lowest held key
    SetPartRootOnly { part: usize, on: bool },
```
- [x] Step 3. Add the decision enum (place right before `pub enum ControlEvent`, mod.rs:170) and Part state. Fields after `keys_down` (mod.rs:470):
```rust
    /// Root-only mode (TECHDEBT #6): gate + per-part lowest-note tracking.
    root_only: bool,
    root_held: [bool; 128],   // keys currently physically held, for re-keying
    root_active: Option<u8>,  // raw note currently sounding as the root
```
Initializer in `Part::new` after `lowest_held: 60, highest_held: 60,` (mod.rs:516):
```rust
            root_only: false, root_held: [false; 128], root_active: None,
```
Enum:
```rust
/// Root-only gate decision for one note-on (TECHDEBT #6).
pub(crate) enum RootDecision {
    /// Suppressed: an equal-or-lower key is already the sounding root.
    Suppress,
    /// Sound `note`; `release` (raw) is the old root to release first.
    Sound { release: Option<u8>, note: u8 },
}
```
- [x] Step 4. Add the Part state machines (place after `note_off` ends, ~mod.rs:870):
```rust
    /// Root-only gate, note-on side. Called for EVERY in-range note-on routed
    /// to this part (both engines) via SynthEngine::route_note_on.
    fn root_on(&mut self, raw: u8) -> RootDecision {
        self.root_held[raw as usize] = true;
        if !self.root_only { return RootDecision::Sound { release: None, note: raw }; }
        match self.root_active {
            None => { self.root_active = Some(raw); RootDecision::Sound { release: None, note: raw } }
            Some(cur) if raw < cur => {
                self.root_active = Some(raw);
                RootDecision::Sound { release: Some(cur), note: raw }
            }
            Some(_) => RootDecision::Suppress, // duplicate or higher key
        }
    }

    /// Root-only gate, note-off side. Returns the next-lowest still-held key
    /// to re-key to (Some only when `raw` WAS the sounding root).
    fn root_off(&mut self, raw: u8) -> Option<u8> {
        self.root_held[raw as usize] = false;
        if !self.root_only || self.root_active != Some(raw) { return None; }
        self.root_active = (0u8..128).find(|&n| self.root_held[n as usize]);
        self.root_active
    }
```
- [x] Step 5. Add the engine routing helpers (place near `dispatch_arp_note_off`, ~mod.rs:1428):
```rust
    /// Shared part note-on dispatch: range/velocity gate, root-only gate
    /// (TECHDEBT #6), DSP voice or SF2 sampler. The enabled/mute guards stay
    /// at the call sites, which intentionally differ (offs skip mute).
    fn route_note_on(&mut self, i: usize, raw: u8, velocity: u8) {
        let (sf2_mode, transpose, lo, hi, vmin, vmax) = {
            let p = &self.parts[i];
            (p.sf2_mode, p.transpose, p.min_note, p.max_note, p.vel_min, p.vel_max)
        };
        if raw < lo || raw > hi || velocity < vmin || velocity > vmax { return; }
        let RootDecision::Sound { release, note } = self.parts[i].root_on(raw) else { return };
        let tr = |n: u8| (n as i16 + transpose as i16).clamp(0, 127) as u8;
        if sf2_mode {
            if let Some(old) = release { self.sampler.note_off(i, tr(old)); }
            self.sampler.note_on(i, tr(note), velocity);
        } else {
            if let Some(old) = release {
                // The replaced root leaves outright: a bass layer must not
                // pile up pedal-sustained roots.
                let part = &mut self.parts[i];
                for v in &mut part.voices {
                    if v.active && v.src_note == old { v.note_off(); }
                }
                part.sustained_notes[old as usize] = false;
            }
            self.parts[i].note_on(note, velocity);
        }
    }

    /// Shared part note-off dispatch with root-only re-keying. Offs skip the
    /// range check (matching today's behavior) so a range change mid-note
    /// cannot orphan a voice.
    fn route_note_off(&mut self, i: usize, raw: u8) {
        let (sf2_mode, transpose, root_only, last_vel) = {
            let p = &self.parts[i];
            (p.sf2_mode, p.transpose, p.root_only, p.last_velocity)
        };
        let next = self.parts[i].root_off(raw);
        let tr = |n: u8| (n as i16 + transpose as i16).clamp(0, 127) as u8;
        if sf2_mode {
            self.sampler.note_off(i, tr(raw));
            if let Some(n) = next { self.sampler.note_on(i, tr(n), (last_vel * 127.0) as u8); }
        } else {
            self.parts[i].note_off(raw);
            if root_only {
                let part = &mut self.parts[i];
                for v in &mut part.voices {
                    if v.active && v.src_note == raw { v.note_off(); }
                }
                part.sustained_notes[raw as usize] = false;
                if let Some(n) = next { part.note_on(n, (last_vel * 127.0) as u8); }
            }
        }
    }
```
- [x] Step 6. Replace the dispatch-site bodies, preserving each site's own guards and looper `record_event` lines exactly:
  - Live velocity==0 offs (mod.rs:1659-1671): inside the `for (i, part)` loop replace the `transposed`/`sf2_mode` branch with `self.route_note_off(i, note);` (keep the `if !part.enabled { continue; }` guard and the `self.looper.record_event(note, 0, i as u8)` line; the loop must switch to index access without a simultaneous `&mut self.sampler` — write it as `for i in 0..self.parts.len() { if !self.parts[i].enabled { continue; } ... }`).
  - Live note-ons (mod.rs:1672-1697): same conversion, body becomes the two existing guards (`!enabled || mute`), then `self.route_note_on(i, note, velocity);` then the `record_event` line (keep its comment block).
  - `MidiEvent::NoteOff` loop (mod.rs:1711-1723): body becomes `self.route_note_off(i, note);` + the `record_event` line; NO enabled/range guard (matches today: offs go to all parts).
  - Arp off (mod.rs:2713-2723) and arp on (mod.rs:2724-2735): replace each loop body's dispatch with `self.route_note_off(i, off_note);` / `self.route_note_on(i, note, vel);`, keeping the `enabled`/`enabled || mute` guards and the `looper.record_event` lines.
  - Looper replay (mod.rs:2739-2759): replace the `vel > 0` / else dispatch with `self.route_note_on(pi, note, vel);` / `self.route_note_off(pi, note);`, keeping the `if !part.enabled { continue; }` guard.
  - MIDI-file `DspLayer0` (mod.rs:2672-2677 on, 2693-2697 off): replace `part.note_on(note, velocity)` with `self.route_note_on(0, note, velocity)` and `part.note_off(note)` with `self.route_note_off(0, note)` (keep `get_mut(0)`-style guard by testing `self.parts.len() > 0` — or simply call through a `if !self.parts.is_empty()` check).
- [x] Step 7. Add the control handler after `SetPartMute`'s (mod.rs:2016):
```rust
            ControlEvent::SetPartRootOnly { part, on } => {
                if let Some(l) = self.parts.get_mut(part) {
                    l.root_only = on;
                    // Reset tracking both ways: a stale root_active across the
                    // toggle would re-key on the next unrelated note-off.
                    l.root_held = [false; 128];
                    l.root_active = None;
                }
            }
```
- [x] Step 8. Extend the five cleanup sites (mod.rs:1765-1768, 1822-1826, 1914-1917, 1984-1987, 2007-2010) — append to each block that clears `note_stack`/`latch_held`:
```rust
                    l.root_held = [false; 128]; // (use `part.` where the site uses `part`)
                    l.root_active = None;
```
- [x] Step 9. Green:
```bash
cargo test --lib root_only && cargo test --test sustain_audit
```
Expected: all new `root_only*` tests pass; the full sustain_audit suite passes unchanged.
- [x] Step 10. Gates + commit:
```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
git add src/synth/ && git commit -m "feat: per-part root-only mode (lowest-note priority) for bass splits"
```

---

### Task 3: Root-only — PartConfig persistence + GUI (TECHDEBT #6, part 2)

**Facts:** `PartConfig` src/preset.rs:756-781 (serde pattern: `#[serde(default)] pub mute: bool` at 775-776); `init_performance` builds one PartConfig at preset.rs:839-853; GUI `PartState` src/gui/mod.rs:141-164, `new_empty` 166-178; routing strip Mute toggle src/gui/layers.rs:151-155; perf save builds PartConfig at layers.rs:221-232; perf load restores at layers.rs:268-298 and pushes per-part events at 289-298.

- [x] Step 1. Failing roundtrip test — extend `performance_part_config_roundtrip` in tests/sustain_audit.rs (line 342): add `root_only: true` to `orig`, `assert!(back.root_only);` after the mute assert, extend `old_json` stays as-is (it must still deserialize) and add `assert!(!legacy.root_only);`.
```bash
cargo test --test sustain_audit performance_part_config 2>&1 | tail -3
```
Expected: compile error (`root_only` field missing) — red.
- [x] Step 2. preset.rs — add to `PartConfig` after `vel_max` (line 780):
```rust
    /// Root-note-only mode (bass split): sounds only the lowest held key.
    #[serde(default)]
    pub root_only: bool,
```
Add `root_only: false,` to the `init_performance` PartConfig literal (preset.rs:839-853).
- [x] Step 3. GUI state — `PartState` field after `transpose` (mod.rs:153):
```rust
    /// Root-only (bass split): engine gates this part to the lowest held key
    pub root_only: bool,
```
`new_empty` (mod.rs:170-172): add `root_only: false,` to the initializer line.
- [x] Step 4. GUI routing strip — in src/gui/layers.rs after the Mute toggle block (155):
```rust
                let mut root_only = self.parts[part].root_only;
                if ui.checkbox(&mut root_only, "Root").on_hover_text(
                    "Root-only (bass split): this part sounds only the lowest held key")
                    .changed()
                {
                    self.parts[part].root_only = root_only;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part, on: root_only });
                }
```
- [x] Step 5. Perf save (layers.rs:224-231): add `root_only: l.root_only,` to the PartConfig literal. Perf load (layers.rs:282 area, after `self.parts[i].mute = part.mute;`):
```rust
                        self.parts[i].root_only = part.root_only;
```
and in the event-push block (after the `SetPartMute` push, layers.rs:291):
```rust
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part: i, on: part.root_only });
```
- [x] Step 6. Green + gates + commit:
```bash
cargo test --test sustain_audit performance_part_config && cargo clippy --all-targets -- -D warnings && cargo test
git add src/preset.rs src/gui/mod.rs src/gui/layers.rs tests/sustain_audit.rs
git commit -m "feat: root-only persists in PartConfig; Root checkbox in part routing strip"
```
Manual: save a performance with Root on part A1, reload it, play a chord — only the lowest key sounds on that part.

---

### Task 4: Sustain release — DSP restore (TECHDEBT #7, engine)

**Facts:** Restored from commit 9a200a4 (reverted by 59ed413 only together with the fader-coupled experiment). `Envelope::note_off` is src/synth/envelope.rs:154-159; `time_to_coeff` envelope.rs:70; `Voice::note_off` voice.rs:508; `SynthEngine` field `sustain` mod.rs:1102, initializer mod.rs:1360; `set_sustain` mod.rs:1390-1424 — its pedal-up release loop is mod.rs:1407-1411; `ControlEvent::SetSustain { on }` handler mod.rs:2187-2189.

- [x] Step 1. Failing tests first — append to tests/sustain_audit.rs:
```rust
// ── Static pedal-up release (TECHDEBT #7, restored from 9a200a4) ──

#[test]
fn pedal_rel_setting_shortens_pedal_up_tail() {
    // Static SR 0.03s: pedal-up tail is far below the patch's amp_release
    // (0.3s on init) — ≈ silent at ~100ms post-release.
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::SetSustainRelease { seconds: Some(0.03) });
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held");
    pedal_up(&mut e);
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..150 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~100ms
    let peak = render_peak(&mut e, 10);
    assert!(peak < 0.15, "SR did not shorten the pedal-up tail (peak {peak})");
}

#[test]
fn pedal_rel_off_keeps_full_default_tails() {
    // SR off: pedal-up takes the patch's full amp_release (0.3s on init)
    // — still audible at ~250ms post-release.
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "no sound to hold");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 20) > SILENCE, "note not held");
    pedal_up(&mut e);
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..375 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r); } // ~250ms
    let peak = render_peak(&mut e, 10);
    assert!(peak > 0.15, "default tail lost with SR off (peak {peak})");
}
```
And in src/synth/envelope.rs append at file end:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn note_off_for_sets_both_release_representations() {
        let mut env = Envelope::new(48000.0);
        env.set_adsr(0.01, 0.1, 0.7, 0.3);
        env.note_off(); // enter Release so note_off_for skips its idle early-return
        env.note_off_for(0.05);
        assert!(env.is_releasing());
        // Linear/Quadratic shapes read phase_step_r
        assert!((env.phase_step_r - 1.0 / (0.05 * 48000.0)).abs() < 1e-9);
        // IIR shapes read release_coeff
        assert!(env.release_coeff > 0.0 && env.release_coeff < 1.0);
        // The full duration actually elapses: silent just past 0.05s
        for _ in 0..(0.055 * 48000.0) as usize { env.tick(); }
        assert!(env.output < 1e-3, "release did not complete in the requested time");
    }
    #[test]
    fn note_off_for_on_idle_is_noop() {
        let mut env = Envelope::new(48000.0);
        env.note_off_for(0.05);
        assert!(env.is_idle());
    }
}
```
```bash
cargo test --test sustain_audit pedal_rel 2>&1 | tail -3; cargo test --lib note_off_for 2>&1 | tail -3
```
Expected: compile errors (`SetSustainRelease`, `note_off_for` missing) — the red.
- [x] Step 2. src/synth/envelope.rs — add after `note_off` (line 159), verbatim from 9a200a4:
```rust
    /// Note-off with an explicit release duration (seconds), overriding the
    /// configured release rate. Used for the static pedal-up release setting.
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
    }
```
- [x] Step 3. src/synth/voice.rs — after `note_off` (508-511):
```rust
    /// Release with an explicit duration; amp env only (filter env keeps the
    /// patch's own release so tonal closure stays unchanged).
    pub fn note_off_for(&mut self, seconds: f32) {
        self.amp_env.note_off_for(seconds);
    }
```
- [x] Step 4. src/synth/mod.rs — `ControlEvent` variant after `SetSustain` (mod.rs:212):
```rust
    /// Static pedal-up release duration in seconds (None = patch release)
    SetSustainRelease { seconds: Option<f32> },
```
`Part` method (place next to `note_off`, ~mod.rs:870), verbatim from 9a200a4:
```rust
    /// Release all voices playing `note` with an explicit amp-release duration
    /// (matches by voice.note — the caller builds its needs bitmap by note).
    fn note_off_for(&mut self, note: u8, seconds: f32) {
        for voice in &mut self.voices {
            if voice.active && voice.note == note { voice.note_off_for(seconds); }
        }
    }
```
Engine field after `sustain` (mod.rs:1102) + initializer after mod.rs:1360:
```rust
    /// Static pedal-up release duration (None = patch release)
    sustain_release: Option<f32>,
```
```rust
            sustain_release: None,
```
Handler after mod.rs:2189:
```rust
            ControlEvent::SetSustainRelease { seconds } => {
                self.sustain_release = seconds;
            }
```
- [x] Step 5. Merge the override into `set_sustain`'s pedal-up branch (mod.rs:1407-1411). Replace the plain loop with:
```rust
                if let Some(secs) = self.sustain_release {
                    // Override: pedal-released voices take `secs` instead of the
                    // patch release. Key-held voices (`keys_down`) are untouched
                    // — same predicate as the plain path below. Two passes over
                    // a stack bitmap keep the borrows of `part` disjoint.
                    let mut needs = [false; 128];
                    for voice in &part.voices {
                        if voice.active && !voice.is_releasing()
                            && !part.keys_down[voice.src_note as usize]
                        {
                            needs[voice.note as usize] = true;
                        }
                    }
                    for (note, &held) in needs.iter().enumerate() {
                        if held { part.note_off_for(note as u8, secs); }
                    }
                } else {
                    for voice in &mut part.voices {
                        if voice.active && !voice.is_releasing()
                            && !part.keys_down[voice.src_note as usize]
                        {
                            voice.note_off();
                        }
                    }
                }
```
(The surrounding latch/`sustained_notes`/`note_stack` cleanup at mod.rs:1398-1420 stays untouched.)
- [x] Step 6. Green:
```bash
cargo test --test sustain_audit
```
Expected: ALL tests pass, including the two new ones AND the pre-existing `pedal_up_releases_held_notes`, `held_key_survives_pedal_up`, `repedal_articulation_keeps_held_key`, `multi_held_keys_pedal_up_then_individual_offs`, `panic_after_held_pedal_up_kills_all` — the override must not change any off-pedal behavior.
- [x] Step 7. Gates + commit:
```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
git add src/synth/ tests/sustain_audit.rs
git commit -m "feat: restore DSP pedal-up release override (SetSustainRelease, from 9a200a4)"
```

---

### Task 5: Sustain release — static GUI settings + SF2 scale wiring (TECHDEBT #7, GUI)

**Facts:** App struct field `sustain_atom` src/gui/mod.rs:289; global controls bar F1 Sustain block gui/mod.rs:741-749; `send_initial_presets` pushes engine state at gui/mod.rs:1223-1224 (`DrumSetVolume`/`DrumSetFixedVelocity`); `save_global_to_config` gui/mod.rs:1293-1311 (`drum_fixed_velocity` line at 1300); `UiSettings` src/config.rs (field `drum_fixed_velocity` at config.rs:105, `Default` impl at config.rs:161); App construction in `run_gui` src/lib.rs:740-741 (`sustain_atom: c.sustain_atom,` / `show_help: false,`). `SetSf2ReleaseScale` exists but no GUI sender (src/synth/mod.rs:234/2270). The GUI block below is the 9a200a4 GUI (removed in 59ed413) restored at the new anchor, minus the fader experiment.

- [x] Step 1. src/config.rs — in `UiSettings` after `drum_fixed_velocity` (105):
```rust
    /// Static pedal-up release in seconds (None = off, patch release)
    #[serde(default)]
    pub sustain_release: Option<f32>,
    /// SF2 release-time scale, 1.0 = file defaults (GM2 CC72 semantics)
    #[serde(default = "default_sf2_release_scale")]
    pub sf2_release_scale: f32,
```
plus near the other `fn default_*` helpers and in `Default for Config` after `drum_fixed_velocity: false,` (161):
```rust
fn default_sf2_release_scale() -> f32 { 1.0 }
```
```rust
                sustain_release: None,
                sf2_release_scale: 1.0,
```
- [x] Step 2. src/gui/mod.rs — App fields after `sustain_atom` (289):
```rust
    /// Static pedal-up release duration (None = off, patch release)
    pub sustain_release: Option<f32>,
    /// SF2 release-time scale (1.0 = file defaults)
    pub sf2_release_scale: f32,
```
- [x] Step 3. Global controls bar — after the F1 Sustain button block (mod.rs:747-749), before the `ui.separator()` at 751:
```rust
                // Static pedal-up release (TECHDEBT #7): CC64 value never enters it
                let sr_on = self.sustain_release.is_some();
                if ui.button("SR").on_hover_text(
                    "Pedal-up release: held notes release in a fixed time instead of the patch release.").clicked()
                {
                    self.sustain_release = if sr_on { None } else { Some(0.3) };
                    let _ = self.ctrl_tx.push(ControlEvent::SetSustainRelease { seconds: self.sustain_release });
                    self.save_global_to_config();
                }
                if sr_on {
                    let mut secs = self.sustain_release.unwrap_or(0.3);
                    if ui.add(egui::Slider::new(&mut secs, 0.05..=8.0)
                        .fixed_decimals(2).suffix(" s")).changed() {
                        self.sustain_release = Some(secs);
                        let _ = self.ctrl_tx.push(ControlEvent::SetSustainRelease { seconds: Some(secs) });
                        self.save_global_to_config();
                    }
                }
                ui.label("SF2 rel:");
                let mut scale = self.sf2_release_scale;
                if ui.add(egui::DragValue::new(&mut scale).range(0.05..=3.0).speed(0.05).suffix("\u{00d7}"))
                    .on_hover_text("SF2 release-time scale (1.0 = file defaults)").changed()
                {
                    self.sf2_release_scale = scale;
                    let _ = self.ctrl_tx.push(ControlEvent::SetSf2ReleaseScale { scale });
                    self.save_global_to_config();
                }
```
- [x] Step 4. Persistence — in `save_global_to_config` after the `drum_fixed_velocity` line (mod.rs:1300):
```rust
        self.config.ui.sustain_release = self.sustain_release;
        self.config.ui.sf2_release_scale = self.sf2_release_scale;
```
In `send_initial_presets` after the `DrumSetFixedVelocity` push (mod.rs:1224):
```rust
        let _ = self.ctrl_tx.push(ControlEvent::SetSustainRelease { seconds: self.sustain_release });
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2ReleaseScale { scale: self.sf2_release_scale });
```
- [x] Step 5. src/lib.rs `run_gui` — App construction after `sustain_atom: c.sustain_atom,` (740) (both settings live in `UiSettings`):
```rust
        sustain_release: c.config.ui.sustain_release,
        sf2_release_scale: c.config.ui.sf2_release_scale,
```
- [x] Step 6. Green + gates + commit:
```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
git add src/config.rs src/gui/mod.rs src/lib.rs
git commit -m "gui: static SR (pedal-up release) + SF2 release-scale settings, persisted"
```
Manual: toggle SR → pedal-held notes release in ~0.3 s regardless of patch; with an SF2 loaded, SF2 rel ×0.2 audibly shortens sample tails; both survive an app restart (config.json `ui.sustain_release` / `ui.sf2_release_scale`).

---

### Task 6: Final gates

- [x] Full gate run from the repo root of the worktree:
```bash
cd /home/dima/Projects/mini_midi_synth/.claude/worktrees/carried-fixes
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```
Expected: all clean. `cargo deny` unaffected (no dependency changes; the pre-existing `warning[yanked]` for clipboard-win stays warn-level).
- [x] Update docs/TECHDEBT.md: mark #6, #7, #11 as done (one line each, with the plan reference). Commit:
```bash
git add docs/TECHDEBT.md
git commit -m "docs: techdebt #6/#7/#11 implemented (console-gui-polish)"
```

---

## Out of scope (do not do here)

- Chord detection for recording (TECHDEBT #5) — root-only deliberately does NOT use `chord.rs`.
- Mono legato / portamento (TECHDEBT #14.5) — separate plan.
- Root-only for the MIDI-file-player SF2 tracks (`seq_note_on` channels) — those are the midi player's own channels, not parts; `DspLayer0` IS routed (Task 2 Step 6).
- GUI-level tests for the modal move — the repo has no egui test harness; covered by compile + manual checklist.
- Any fader/pedal-position coupling of release (the exact 59ed413 revert reason) and half-pedal CC range mapping (TECHDEBT #7 research: out of scope until a continuous pedal exists).
- `cargo fmt` over the codebase (repo rule); new code follows the compact style shown in the blocks above.
