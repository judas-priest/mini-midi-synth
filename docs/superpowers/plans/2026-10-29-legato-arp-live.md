# Mono Legato + Portamento (per part) and Arp Live Controls (Latch, BPM sync) Plan

> anchor-checked: 2026-10-29
> status: not started
> Source: docs/TECHDEBT.md §14.5 (Mono Legato + Portamento) and §14.4 (Arp live-контролы: Latch/Hold + sync).

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Each task is executed by a FRESH subagent that receives ONLY this plan document.

**Goal:** Two features. (1) Per-part mono legato + portamento for DSP parts, exposed as live `ControlEvent`s, with the portamento-mode bugs fixed. (2) Arpeggiator latch (arp keeps playing after keys released; new chord replaces the held set; cleared by latch-off or panic) + proof/guard of the already-existing BPM sync.

## KEY FINDINGS (investigation results — read before implementing)

The requested DSP already largely EXISTS. This plan closes the actual gaps; do not re-implement what is listed here as existing:

- **Mono/legato/latch play modes exist** (`Part::note_on`, src/synth/mod.rs:630-660): `params.play_mode` — 0=Poly, 1=Mono (re-attack), 2=Mono-ST (single trigger = legato: new note while a key is held calls `Voice::retrigger_note`, envelopes NOT re-attacked), 3=Latch (per-part key latch via `Part.latch_held`), 4/5=Poly-High/Low, 6=Piano. GUI names at src/gui/mod.rs:35.
- **Portamento exists**: `PatchParams.portamento_time` (SECONDS, GUI slider 0..2 s, src/gui/params.rs:1359) and `portamento_mode` (0=Off, 1=Always, 2=Fingered (Legato), src/gui/mod.rs:34). `Voice` already slews pitch per-sample in the log-frequency domain (`log_freq` → `log_target_freq` with `porta_coeff`, voice.rs:594-601) — plain f32 fields, alloc-free. Preset persistence via patch params (patch_params.rs:53-56, s!/p! macros) and per-performance `PartConfig.param_overrides: BTreeMap<String, f32>` (src/preset.rs:763-765) — **no PartConfig schema change needed**. GUI editors for all three already exist (src/gui/params.rs:1306-1359) and reach the engine live via `send_edited_params` (src/gui/mod.rs:1485-1490 → `load_patch_from_edited`, mod.rs:283).
- **BUG #1 (fixed here):** the mono note-on path (mod.rs:640) passes `prev_freq = self.last_note_freq` UNCONDITIONALLY, ignoring `portamento_mode`. Consequences: with mode=Off but time>0, mono notes glide anyway; with mode=Fingered(2), the first note after all keys are released still glides from the previous phrase's last pitch (stale glide). The poly path (mod.rs:694-699) already gates correctly — copy that gating.
- **BUG #2 (fixed here):** `Voice::retrigger_note` (voice.rs:541-554) glides whenever `portamento_time > 0.001` (ignores mode) and — worse — never sets `porta_coeff`. The per-sample slew in `tick()` only runs `if self.porta_coeff < 0.999` (voice.rs:595); if the voice's last full `note_on` was instant (`porta_coeff = 1.0`), a legato `retrigger_note` with a glide time still SNAPS to the new pitch.
- **Latch (`play_mode == 3`) is a DIFFERENT feature** from the arp latch: it is a per-part key latch (`Part.latch_held`, mod.rs:600-628). The **arpeggiator has NO latch** — `Arpeggiator::note_off` (arpeggiator.rs:87-110) removes the note and, when the last key is released, resets the pattern (silence).
- **Arp rate sync to sequencer BPM ALREADY EXISTS, unconditionally**: every audio block `tick_block` does `seq_bpm = self.drum_engine.sequencer.bpm; self.arpeggiator.set_bpm(seq_bpm)` (mod.rs:2449-2450); `recalc_timing` (arpeggiator.rs:170-185) maps the rate enum (0=1/4 … 5=1/8T) to `step_samples` from that BPM. There is no separate "sync" switch to add — §14.4's "rate sync" item reduces to guard tests. This plan documents that and locks it with a unit test.
- **Arp is engine-global** (one `Arpeggiator`, mod.rs:1086), zero-alloc, fixed arrays. Its params live in patch params `arp_enabled/arp_mode/arp_rate/arp_octaves/arp_gate` (patch_params.rs:167-171, defaults at 278, from_map 457-461, serialization 610-611), are synced to the engine on patch load (mod.rs:1960-1966) and edited per-part in the GUI arp panel (src/gui/params.rs:1388-1450, Enabled checkbox sends an immediate `SetArpParams`, 1394-1407).
- **`handle_control`'s match over `ControlEvent` is exhaustive (no wildcard arm)** — mod.rs:2366 is the last arm. Adding enum variants forces a compile error until the handler arm is added. That is the designed RT-safety pattern: heap-owning variants `return Some(..)` early; plain-data variants fall through to `None`.

## Architecture

- DSP voice pipeline: `Part::note_on` (mod.rs:568+) → `Voice::note_on` / `Voice::retrigger_note` (voice.rs:193/541) → per-sample `Voice::tick` (voice.rs:569). Pitch = `freq` (log-slewed) × `mods.pitch_mult`; envelope continuity is guaranteed by `Envelope::note_on` restarting attack FROM the current output value (envelope.rs:136-151) — the audible difference between mono (re-attack) and mono-ST (legato) is the attack overshoot (Sqrt shape IIR toward 1.3) plus oscillator phase reset, which is what the legato test asserts on.
- Arp pipeline: MIDI NoteOn/Off → `Arpeggiator::note_on/note_off` (mod.rs:1651-1658 / 1708-1711); per-sample `Arpeggiator::tick()` in `tick_block` (mod.rs:2705-2737) dispatches transposed note on/off to every enabled part. `flush_arp_note_off` (mod.rs:1441-1450) releases the sounding arp voice when the last physical key is released — this is exactly the hook latch must bypass.
- Everything added here is plain scalars (`bool`, `f32`) on existing structs and plain-data enum variants → RT-safe by construction; no allocation, no locks, no I/O.

## Tech Stack

Rust. Test harnesses: `tests/sustain_audit.rs` (`engine_with_init_patch` / `render_peak` / `settle` / `init_patch_mut`, lines 10-64; engine state is NOT reachable from there — audio-observable assertions only) and `src/synth/tests.rs` (child module, `use super::*`, sees private `SynthEngine.parts` / `Part` / `pub(crate)` `Voice` fields). `SynthEngine::tick()` exists but is `#[cfg(test)]` one-sample (mod.rs:3080); unit-test helpers render whole `BLOCK_SIZE` (=32, mod.rs:349) blocks via `tick_block`. Gates: `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test --features rt-assert --test rt_safety`, `cargo deny check`.

## Decisions

- **Feature 1 scope = DSP parts only.** SF2 legato/glide (rustysynth `startAddrsOffset` skip-attack, pitch-bend glide) is OUT of scope here — future item, see Out of scope.
- **Live events, not new patch params:** mono/legato/glide already live in `PatchParams` (`play_mode`, `portamento_time`, `portamento_mode`). Add three plain-data events `SetPartMono { part, mono }`, `SetPartLegato { part, legato }`, `SetPartGlide { part, ms }` (glide in MILLISECONDS on the event, converted to seconds internally — params stay in seconds to avoid touching presets). Handlers write `part.params.*` directly, following the `SetPartTranspose` pattern (mod.rs:2044-2049). Like `SetPartTranspose`, they do NOT update GUI `edited_params` (GUI keeps editing the same params via its own panel — divergence is accepted and identical to transpose).
- **Mono toggling clears `note_stack`** (stale stack from the other mode must not resurrect notes — same rationale as `mono_pedal_up_clears_note_stack` in tests/sustain_audit.rs:218). `latch_held` is left alone (only meaningful for play_mode 3).
- **`SetPartGlide` with ms > 0 also raises `portamento_mode` 0→1** (glide time without a mode is dead config; "Always" is the least surprising default). ms=0 sets time to 0 → instant (mode left as-is).
- **Arp latch semantics (matches the §14.4 spec: "играет после отпускания; новый аккорд подхватывается без остановки; выключение повторным нажатием или паникой"):** two new fields on `Arpeggiator` — `latch: bool` (config, persisted as patch param `arp_latch`) and `latched: bool` (runtime "frozen" flag). In latch mode: `note_off` of the last physical key FREEZES the set (`latched = true`, pattern keeps cycling, position/tick counter untouched → no gap); `note_on` while frozen REPLACES the held set in place (clear + add, pattern rebuilt, position preserved → new chord picks up mid-flow); `clear()` (panic/AllNotesOff/SetArpParams-off) resets `latched`; `set_latch(false)` while frozen stops the set. `flush_arp_note_off`'s guard switches from `has_held_notes()` to a new `keeps_playing()` (= `has_held_notes() || (latch && latched)`) so the key-off that freezes does not kill the sounding arp voice.
- **Arp panic gap closed:** the CC120/123 panic path (mod.rs:1834-1837) flushes the current arp note but never calls `arpeggiator.clear()` (unlike AllNotesOff at mod.rs:2206-2211) — held (and now latched) notes survive CC120. Add `self.arpeggiator.clear()` there.
- **Rate sync:** already implemented (see Key Findings) — guard unit test only, no code.

## Out of scope (do NOT do here)

- SF2 legato/glide: skip-attack via rustysynth `startAddrsOffset` (vendor/rustysynth/src/instrument_region.rs:148), pitch-bend glide for SF2 channels, per-kit offset config — separate future item (TECHDEBT §14.5 bullet 2).
- GUI panels for `SetPartMono/Legato/Glide` events (the params editor already exposes the same three params live via patch reload; the new events are the hardware-binding surface for §14.x pad/fader work).
- Arp transpose (§14.4 mentions it) — separate item; `SetPartTranspose` already transposes arp notes at dispatch (mod.rs:2713-2730).
- `Part::latch_held` / `play_mode == 3` behavior changes; `cargo fmt`.

---

## Test Cases (from the requirement)

Requirement text: "нажимаешь вторую ноту — первая перестаёт, вторая без атаки плавно въезжает" = mono (last-note priority) + legato (no envelope re-attack while a key is held) + portamento (pitch slides over N ms); glide off = instant; poly unaffected. Arp: latch keeps playing after keys released; new chord replaces the held set; latch cleared by second toggle press or panic CC; rate follows sequencer BPM.

Written BEFORE implementation. Expectations vs current code in parentheses.

Feature 1:
1. `mono_mode_is_single_voice` (unit, src/synth/tests.rs) — play_mode=1, two overlapping note-ons → exactly 1 active voice, sounding note = second note. (GREEN now — guard.)
2. `mono_retrigger_restarts_attack` (integration, tests/sustain_audit.rs) — play_mode=1, sustain reached, second note-on while held → peak in the following 150 ms window rises ≥ 25 % above the sustain peak (Sqrt-shape attack overshoot). (GREEN now — guards the legato test's contrast.)
3. `mono_legato_keeps_sustain_level_through_note_change` (integration, tests/sustain_audit.rs) — play_mode=2 (Mono-ST), same setup → peak stays within 15 % of the sustain peak (no re-attack bump). (GREEN now — Mono-ST already works; guards the fix task against regressing it.)
4. `legato_with_no_held_key_is_normal_attack` (integration, tests/sustain_audit.rs) — play_mode=2, single note after silence → full attack bump (same overshoot assertion as test 2). (GREEN now — `retrigger` requires `note_stack.len() > 1`, mod.rs:642.)
5. `glide_slews_pitch_in_mono_mode` (unit, src/synth/tests.rs, uses new `Voice::current_freq()` test accessor) — mono, portamento_time=0.2 s, mode=Always; note 60 → 72 → ~7 ms later `current_freq()` is strictly between f(60) and f(72); 0.8 s later it is within 1 Hz of f(72). (GREEN now for this exact config.)
6. `glide_off_is_instant` (unit) — portamento_time=0.2 s but mode=Off → 1 block after the new note-on, `current_freq()` == f(72) within 0.5 Hz. (**RED** before Task 2: mono path ignores mode and glides.)
7. `fingered_portamento_does_not_glide_from_released_note` (unit) — mode=Fingered, note 60 held → released → voice released → note 72 → `current_freq()` == f(72) within 0.5 Hz immediately. (**RED** before Task 2: glides from stale `last_note_freq`.)
8. `legato_retrigger_glides_when_first_note_was_instant` (unit) — play_mode=2, mode=Fingered, time=0.2 s; first note (no glide, `porta_coeff == 1.0`), second note while held → ~7 ms later freq strictly between. (**RED** before Task 2: `retrigger_note` never sets `porta_coeff`, so tick snaps.)
9. `set_part_mono_legato_glide_events` (unit) — poly part: SetPartMono{true} → play_mode 1; SetPartLegato{true} → 2; SetPartGlide{ms:150} → portamento_time 0.15 ± 1e-4 and portamento_mode 1; SetPartGlide{ms:0} → time 0; SetPartMono{false} → play_mode 0 and `note_stack` empty; SetPartMono{true} on a latch (3) part is a no-op; part index 99 is a no-op. (**RED** before Task 3: variants don't exist.)
10. `mono_poly_piano_and_latch_presets_unchanged` — existing suites green: `mono_pedal_up_clears_note_stack`, `latch_mode_works_with_transpose` (tests/sustain_audit.rs:218/465) plus the whole `cargo test` run.

Feature 2:
11. `latch_freezes_pattern_after_last_key_release` (unit, new `#[cfg(test)]` mod in src/synth/arpeggiator.rs) — latch on, 2 keys pressed and released → `keeps_playing()`, `pattern_len == 2`, ticks keep emitting notes. (**RED** before Task 6.)
12. `latch_new_chord_replaces_held_set` (unit) — frozen with {60}, `note_on(72)` → held set = {72}, `latched == false`, still `keeps_playing()`. (**RED** before Task 6.)
13. `latch_off_while_frozen_stops` (unit) — frozen → `set_latch(false)` → `pattern_len == 0`, `!keeps_playing()`. (**RED** before Task 6.)
14. `clear_resets_latch_state` (unit) — frozen → `clear()` → `!keeps_playing()`. (**RED** before Task 6.)
15. `no_latch_stops_on_last_release` (unit) — latch off, last key off → `pattern_len == 0` (existing behavior guard). (GREEN now.)
16. `rate_follows_bpm` (unit) — rate 1 (1/8), `set_bpm` 120 → `step_samples == 12_000`; 240 → 6_000. (GREEN now — documents/locks the existing sync.)
17. `arp_latch_keeps_playing_after_key_release` (integration, tests/sustain_audit.rs) — `arp_latch=1.0` in patch, arp on, note on → sound; note off → sound continues; `SetArpLatch{false}` → settle → silence. (**RED** before Task 7: `SetArpLatch` doesn't exist; the note-off currently silences the arp.)
18. `arp_latch_new_chord_takes_over_and_off_stops` (integration) — frozen arp, new note-on → still sounding, released → still sounding; `SetArpLatch{false}` → silence. (**RED** before Task 7.)
19. `panic_cc_clears_latched_arp` (integration) — frozen arp, MIDI CC120 → settle → silence. (**RED** before Task 7: held arp notes survive CC120 today.) CC123 variant is exercised by the existing `cc123_panic_releases_pedal_held_notes` staying green.
20. `arp_latch_persists_through_patch_load` (integration) — patch with `arp_latch: 1.0` → after `load_patch_from`, `engine.arpeggiator.latch_enabled()` is true; patch without the key → false. (**RED** before Task 7.)
21. Full gates: `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo test --features rt-assert --test rt_safety`, `cargo deny check` all pass.

## Negative Matrix

- **Missing/out-of-range index:** `SetPartMono/Legato/Glide` with `part >= MAX_PARTS` → `self.parts.get_mut(part) == None` → no-op (test 9; same guard as SetPartTranspose, mod.rs:2047).
- **Zero/empty:** `SetPartGlide{ms:0}` → instant; negative ms clamps to 0; `arp_latch` key absent from a patch → `p("arp_latch", 0.0)` → latch off (test 20). Frozen arp with empty set and `tick()` → pattern_len > 0 keeps cycling; there is no "empty frozen" state (`latched` only set when a note was actually removed to zero).
- **Extreme values:** `SetPartGlide` ms clamped to 0.0..2000.0 (→ seconds 0.0..2.0, matching the GUI slider range, params.rs:1359). `porta_coeff` divides by `portamento_time * sample_rate` only when `portamento_time > 0.001` (voice.rs:211) — smallest possible samples = 48, no div-by-zero. Arp `MAX_HELD`=16 overflow → existing `if self.held_count < MAX_HELD` clamp (arpeggiator.rs:80).
- **Duplicate/concurrent:** duplicate `note_on` while frozen → set was just cleared, note re-added once; `note_off` of a note not in the held set → `found == false`, no-op (arpeggiator.rs:100); repeated `SetArpLatch{true}` → idempotent; `SetArpMono` toggles are idempotent via the match fallthrough.
- **Ordering:** `SetArpLatch{false}` handler flushes the sounding note BEFORE `set_latch(false)` clears the set — the part always receives a matching off. Panic CC flushes then clears. Patch load (`load_patch_from`) re-syncs latch from params, overwriting live state — patch wins, same as arp_enabled.
- **Mode interactions:** SetPartMono{true} on play_mode 3 (Latch) / 4 / 5 → no-op by design (documented in the handler match). SetPartLegato with play_mode != 1/2 → no-op.
- **Clock skew:** n/a — single audio thread; arp timing derives from `sequencer.bpm` read at block start (mod.rs:2449) and `recalc_timing` on `set_bpm`.
- **External-call timeout:** no new cross-thread or blocking calls; all new code is plain audio-thread state mutation.

---

### Task 0: Branch

- [ ] From the current worktree state (branch `worktree-carried-fixes`, HEAD 492dea2):
```bash
cd /home/dima/Projects/mini_midi_synth/.claude/worktrees/carried-fixes
git checkout -b legato-arp-live
```

---

## FEATURE 1 — Mono Legato + Portamento

### Task 1: RED unit tests + `Voice::current_freq()` test accessor

**Facts (anchor-verified):** `Voice` fields `freq`/`log_freq`/`log_target_freq`/`porta_coeff` are private to voice.rs; `src/synth/tests.rs` (child module of `synth`) can read `pub(crate)` fields (`note`, `src_note`, `active`) but NOT `freq`. Add a test-only accessor next to `is_releasing` (voice.rs:556-558). `SynthEngine.parts` is private but visible to `synth::tests` (declared mod.rs:1042). `preset::load_all_patches()` and `SynthEngine::set_patches` (mod.rs:1464) are the engine-construction pattern used by tests/sustain_audit.rs:32-42. Init preset envelope: attack 0.01, sustain 0.7 (presets/init.json) — tests override via patch params.

- [ ] Step 1. In `src/synth/voice.rs`, insert after `is_releasing` (voice.rs:556-558):
```rust
    /// Test-only: expose the current (pre-modulation) voice pitch for
    /// portamento assertions. Compiles out of release builds.
    #[cfg(test)]
    pub(crate) fn current_freq(&self) -> f32 {
        self.freq
    }
```

- [ ] Step 2. Append to `src/synth/tests.rs` (end of file; module has `use super::*`):
```rust
// ── Mono / legato / portamento ─────────────────────────────────

const F60: f32 = 440.0 * 2.0_f32.powf(-9.0 / 12.0);
const F72: f32 = 440.0 * 2.0_f32.powf(3.0 / 12.0);

fn patch_engine(play_mode: f32, porta_time: f32, porta_mode: f32) -> SynthEngine {
    let patches = crate::preset::load_all_patches();
    let mut patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists")
        .clone();
    patch.params.insert("play_mode".into(), play_mode);
    patch.params.insert("portamento_time".into(), porta_time);
    patch.params.insert("portamento_mode".into(), porta_mode);
    patch.params.insert("amp_release".into(), 0.05);
    let mut e = SynthEngine::new(48000.0);
    e.set_patches(patches);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    e
}

fn render_blocks(e: &mut SynthEngine, blocks: usize) {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        e.tick_block(&mut l, &mut r);
    }
}

fn active_freq(e: &SynthEngine) -> f32 {
    e.parts[0]
        .voices
        .iter()
        .find(|v| v.active)
        .map(|v| v.current_freq())
        .unwrap_or(0.0)
}

fn note(e: &mut SynthEngine, n: u8) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: n, velocity: 100 });
}

fn unnote(e: &mut SynthEngine, n: u8) {
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: n });
}

#[test]
fn mono_mode_is_single_voice() {
    let mut e = patch_engine(1.0, 0.0, 0.0);
    note(&mut e, 60);
    render_blocks(&mut e, 5);
    note(&mut e, 72);
    render_blocks(&mut e, 5);
    let active = e.parts[0].voices.iter().filter(|v| v.active).count();
    assert_eq!(active, 1, "mono mode must keep exactly one voice");
    let v = e.parts[0].voices.iter().find(|v| v.active).unwrap();
    assert_eq!(v.note, 72, "mono must follow the last note");
}

#[test]
fn glide_slews_pitch_in_mono_mode() {
    let mut e = patch_engine(1.0, 0.2, 1.0); // 200 ms, Always
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    note(&mut e, 72);
    render_blocks(&mut e, 7); // ~4.7 ms — well inside the 200 ms glide
    let f = active_freq(&e);
    assert!(
        f > F60 && f < F72,
        "expected mid-glide pitch between {F60} and {F72}, got {f}"
    );
    render_blocks(&mut e, 800); // 0.53 s >> glide time
    let f = active_freq(&e);
    assert!((f - F72).abs() < 1.0, "glide did not land on target: {f}");
}

#[test]
fn glide_off_is_instant() {
    // Time is SET but mode is Off — must not glide (mono path ignores mode
    // today: RED until the fix).
    let mut e = patch_engine(1.0, 0.2, 0.0);
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    note(&mut e, 72);
    render_blocks(&mut e, 1);
    let f = active_freq(&e);
    assert!((f - F72).abs() < 0.5, "glide must be instant in mode Off: {f}");
}

#[test]
fn fingered_portamento_does_not_glide_from_released_note() {
    // Mode Fingered (legato): first note after all keys released must be
    // instant, not a glide from the previous phrase's last pitch (RED until
    // the fix).
    let mut e = patch_engine(1.0, 0.2, 2.0);
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    unnote(&mut e, 60);
    render_blocks(&mut e, 400); // release 0.05 s → voice idle
    note(&mut e, 72);
    render_blocks(&mut e, 1);
    let f = active_freq(&e);
    assert!((f - F72).abs() < 0.5, "stale glide from released note: {f}");
}

#[test]
fn legato_retrigger_glides_when_first_note_was_instant() {
    // Mono-ST + Fingered: the first note has no glide (porta_coeff = 1.0);
    // the second, held-key note must still glide (RED until retrigger_note
    // sets porta_coeff).
    let mut e = patch_engine(2.0, 0.2, 2.0);
    note(&mut e, 60);
    render_blocks(&mut e, 20);
    note(&mut e, 72);
    render_blocks(&mut e, 7);
    let f = active_freq(&e);
    assert!(
        f > F60 && f < F72,
        "legato retrigger snapped instead of gliding: {f}"
    );
}

#[test]
fn set_part_mono_legato_glide_events() {
    let mut e = patch_engine(0.0, 0.0, 0.0);
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: true });
    assert_eq!(e.parts[0].params.play_mode, 1.0);
    let _ = e.handle_control(ControlEvent::SetPartLegato { part: 0, legato: true });
    assert_eq!(e.parts[0].params.play_mode, 2.0);
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 0, ms: 150.0 });
    assert!((e.parts[0].params.portamento_time - 0.15).abs() < 1e-4);
    assert_eq!(e.parts[0].params.portamento_mode, 1.0);
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 0, ms: 0.0 });
    assert_eq!(e.parts[0].params.portamento_time, 0.0);
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 0, ms: 5000.0 });
    assert!((e.parts[0].params.portamento_time - 2.0).abs() < 1e-4, "ms must clamp to 2 s");
    // Mono off clears the stale stack and returns to poly
    note(&mut e, 60);
    render_blocks(&mut e, 1);
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: false });
    assert_eq!(e.parts[0].params.play_mode, 0.0);
    assert!(e.parts[0].note_stack.is_empty(), "mono-off must clear note_stack");
    // Latch mode is left alone by the mono toggle
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: true });
    e.parts[0].params.play_mode = 3.0;
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 0, mono: true });
    assert_eq!(e.parts[0].params.play_mode, 3.0, "latch mode must not be clobbered");
    // Out-of-range part: no-op, no panic
    let _ = e.handle_control(ControlEvent::SetPartGlide { part: 99, ms: 50.0 });
    let _ = e.handle_control(ControlEvent::SetPartMono { part: 99, mono: true });
}
```

- [ ] Step 3. Run — expect exactly the four documented failures:
```bash
cargo test --lib mono_mode_is_single_voice 2>&1 | tail -3
cargo test --lib set_part_mono_legato_glide_events 2>&1 | tail -3
```
Expected: `mono_mode_is_single_voice` PASSES; `glide_off_is_instant`, `fingered_portamento_does_not_glide_from_released_note`, `legato_retrigger_glides_when_first_note_was_instant` FAIL (assertions on freq); `set_part_mono_legato_glide_events` FAILS to COMPILE (`SetPartMono` not found) — which also breaks the whole lib test target until Task 3. To keep the RED state reviewable you may run `cargo test --lib 2>&1 | tail -20` and note `error[E0599]: no variant or associated item named 'SetPartMono'`. Proceed to Tasks 2-3 before re-running the suite.

- [ ] Step 4. Commit the RED tests:
```bash
git add src/synth/voice.rs src/synth/tests.rs
git commit -m "test: mono/legato/glide semantics — RED (porta-mode gating, retrigger coeff, SetPart events)"
```

---

### Task 2: Fix portamento-mode gating (mono note-on path + `retrigger_note`)

**Facts (anchor-verified):**
- mod.rs:640 (inside the `if play_mode == 1 || play_mode == 2` block, mod.rs:631, after the `let idx = …` expression at 637-639): `let prev_freq = self.last_note_freq;` — unconditional. The poly path (mod.rs:694-699) is the correct pattern to copy.
- voice.rs:541-554 `retrigger_note`: glides on `portamento_time > 0.001` alone and never sets `porta_coeff`; the slew loop in `tick()` runs only `if self.porta_coeff < 0.999` (voice.rs:595). `VoiceParams` carries `_portamento_mode: f32` (voice.rs:921, set in `build_voice_params` mod.rs:775).
- The mono note-off fallback (mod.rs:802-815) calls `retrigger_note` when falling back down the stack — with the fix, it glides iff mode > 0, matching the note-on gating.

- [ ] Step 1. In `src/synth/mod.rs`, replace (mod.rs:640, keep surrounding lines intact):
```rust
            let prev_freq = self.last_note_freq;
```
with:
```rust
            // Same gating as the poly path: portamento_mode 0 (Off) never
            // glides; 2 (Fingered) glides only while another key is held
            // (the new note is already pushed, so len > 1 means a key was
            // held); 1 (Always) glides from the last pitch.
            let porta_mode = self.params.portamento_mode as u8;
            let prev_freq = match porta_mode {
                1 => self.last_note_freq,
                2 => if self.note_stack.len() > 1 { self.last_note_freq } else { None },
                _ => None,
            };
```

- [ ] Step 2. In `src/synth/voice.rs`, replace the glide block of `retrigger_note` (voice.rs:547-553):
```rust
        // Apply portamento glide (uses existing portamento_time from params)
        if params.portamento_time > 0.001 {
            // log_freq will glide toward log_target_freq in tick() naturally
        } else {
            self.freq = new_freq;
            self.log_freq = new_freq.ln();
        }
```
with:
```rust
        // Apply portamento glide (log_freq slews toward log_target_freq in
        // tick()). Gated on portamento_mode so mode 0 (Off) is instant even
        // with a glide time set — matches the note-on path. porta_coeff MUST
        // be (re)computed here: tick() only slews while porta_coeff < 0.999,
        // so a voice whose last full note_on was instant would otherwise snap.
        if params.portamento_time > 0.001 && params._portamento_mode as u8 > 0 {
            let samples = params.portamento_time * self.sample_rate;
            self.porta_coeff = 1.0 - (-4.0 / samples).exp();
        } else {
            self.freq = new_freq;
            self.log_freq = new_freq.ln();
            self.porta_coeff = 1.0;
        }
```

- [ ] Step 3. Verify Task 1's tests now pass:
```bash
cargo test --lib glide_off_is_instant fingered_portamento_does_not_glide 2>&1 | tail -5
cargo test --lib legato_retrigger_glides_when_first_note_was_instant 2>&1 | tail -3
```
Expected: all green. (`set_part_*` still fails to compile until Task 3.)

- [ ] Step 4. Commit:
```bash
git add src/synth/mod.rs src/synth/voice.rs
git commit -m "fix: mono path respects portamento_mode; retrigger_note sets porta_coeff (legato glide actually glided nowhere before)"
```

---

### Task 3: `SetPartMono` / `SetPartLegato` / `SetPartGlide` events

**Facts (anchor-verified):** `ControlEvent` enum — `SetPartTranspose { part: usize, semitones: i8 }` at mod.rs:178; `SetArpParams` is the last variant at mod.rs:250. Handler match in `handle_control` is exhaustive (no wildcard); `SetPartTranspose` arm at mod.rs:2044-2049 is the pattern (`if let Some(l) = self.parts.get_mut(part)`). Plain-data variants fall through to the `None` return after the match (mod.rs:2376-2380). `Part.params` is `PatchParams` with `pub play_mode: f32` (patch_params.rs:56) and `pub(super) portamento_time/portamento_mode` (patch_params.rs:53 — visible to `synth` module tree, so writable from `handle_control`).

- [ ] Step 1. In `src/synth/mod.rs`, add after line 178 (`SetPartTranspose { part: usize, semitones: i8 },`):
```rust
    /// Mono mode per part (last-note priority). true: poly/piano → mono
    /// (1), Mono-ST (2) → mono (1); false: mono/mono-ST → poly (0). Latch
    /// (3) / Poly-High / Poly-Low are left untouched. Clears the stale
    /// mono note stack on every transition.
    SetPartMono { part: usize, mono: bool },
    /// Legato per part: mono (1) ↔ mono-ST (2). No-op in other modes.
    SetPartLegato { part: usize, legato: bool },
    /// Portamento glide time in milliseconds (0 = instant). Clamped to
    /// 0..2000 ms (patch param `portamento_time` stays in seconds). A
    /// non-zero value also raises portamento_mode 0→1 ("Always").
    SetPartGlide { part: usize, ms: f32 },
```

- [ ] Step 2. In `handle_control`, insert directly after the `SetPartTranspose` arm (mod.rs:2044-2049):
```rust
            ControlEvent::SetPartMono { part, mono } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let cur = l.params.play_mode as u8;
                    match (mono, cur) {
                        (true, 0) | (true, 6) => l.params.play_mode = 1.0, // poly/piano → mono
                        (true, 2) => l.params.play_mode = 1.0,             // mono-ST → mono
                        (false, 1) | (false, 2) => l.params.play_mode = 0.0, // → poly
                        _ => {} // already in the requested state; latch/high/low untouched
                    }
                    // A stale mono stack from the previous mode must not
                    // resurrect notes in the new mode.
                    l.note_stack.clear();
                }
            }
            ControlEvent::SetPartLegato { part, legato } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let cur = l.params.play_mode as u8;
                    if legato && cur == 1 {
                        l.params.play_mode = 2.0;
                    } else if !legato && cur == 2 {
                        l.params.play_mode = 1.0;
                    }
                }
            }
            ControlEvent::SetPartGlide { part, ms } => {
                if let Some(l) = self.parts.get_mut(part) {
                    let secs = (ms / 1000.0).clamp(0.0, 2.0);
                    l.params.portamento_time = secs;
                    if secs > 0.0 && l.params.portamento_mode < 0.5 {
                        l.params.portamento_mode = 1.0; // "Always" — a glide time without a mode is dead config
                    }
                }
            }
```

- [ ] Step 3. Run the full unit suite:
```bash
cargo test --lib 2>&1 | tail -5
```
Expected: all green, including `set_part_mono_legato_glide_events` and `glide_slews_pitch_in_mono_mode`.

- [ ] Step 4. RT-safety check (new handler code is inside the audio thread's control path):
```bash
cargo test --features rt-assert --test rt_safety 2>&1 | tail -5
```
Expected: pass.

- [ ] Step 5. Commit:
```bash
git add src/synth/mod.rs
git commit -m "feat: SetPartMono/Legato/Glide live control events (per-part mono+legato+glide, TECHDEBT 14.5)"
```

---

### Task 4: Legato integration tests (audio-observable)

**Facts (anchor-verified):** tests/sustain_audit.rs helpers at lines 10-64. The default attack shape is Sqrt (IIR toward a 1.3 overshoot target, envelope.rs:19-33/199-207) and `Envelope::note_on` restarts attack from the CURRENT output (envelope.rs:136-151). So a mono re-attack from sustain 0.7 with attack 0.2 s produces a rising overshoot: peak within the next 150 ms ≈ 1.17 × osc-scale vs sustain ≈ 0.7 — a ≥ 25 % rise; legato (no envelope change) keeps the peak flat (±15 %). Init preset has `filter_env_amount: 0.0` and no LFO depths (presets/init.json) → amplitude is envelope-only. BLOCK_SIZE = 32 (mod.rs:349) → 1500 blocks/s; 150 ms ≈ 225 blocks; 300 blocks = 200 ms; 600 blocks = 0.4 s.

- [ ] Step 1. Append to `tests/sustain_audit.rs`:
```rust
// ── Mono legato (TECHDEBT 14.5) ────────────────────────────────

fn legato_engine(play_mode: f32) -> SynthEngine {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("play_mode".to_string(), play_mode);
    patch.params.insert("amp_attack".to_string(), 0.2);
    patch.params.insert("amp_sustain".to_string(), 0.7);
    patch.params.insert("amp_release".to_string(), 0.1);
    patch.params.insert("filter_env_amount".to_string(), 0.0);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    e
}

/// Sustain level reached, then peak of the 150 ms window after the second
/// note-on, relative to the sustain peak.
fn reattack_ratio(e: &mut SynthEngine) -> f32 {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(e, 600); // 0.4 s — attack (0.2 s) done, at sustain
    let before = render_peak(e, 100);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    let after = render_peak(e, 225); // 150 ms window
    after / before
}

#[test]
fn mono_legato_keeps_sustain_level_through_note_change() {
    let mut e = legato_engine(2.0); // Mono-ST: no re-attack while a key is held
    let ratio = reattack_ratio(&mut e);
    assert!(
        ratio < 1.15,
        "legato re-attacked the envelope (peak ratio {ratio})"
    );
}

#[test]
fn mono_retrigger_restarts_attack() {
    let mut e = legato_engine(1.0); // Mono: full re-attack
    let ratio = reattack_ratio(&mut e);
    assert!(
        ratio > 1.25,
        "mono retrigger did not restart the attack (peak ratio {ratio})"
    );
}

#[test]
fn legato_with_no_held_key_is_normal_attack() {
    let mut e = legato_engine(2.0);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let before = render_peak(&mut e, 100); // mid-attack: well below the peak
    let _ = render_peak(&mut e, 600);      // reach sustain
    // Let the first note go fully, then play again: single note = full attack
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 600); // release 0.1 s → idle
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 62, velocity: 127 });
    let after = render_peak(&mut e, 225);
    assert!(
        after > before * 2.0,
        "first note after silence must attack normally (mid-attack {before}, re-attack window {after})"
    );
}
```

- [ ] Step 2. Run:
```bash
cargo test --test sustain_audit mono_legato mono_retrigger legato_with_no_held 2>&1 | tail -6
```
Expected: all three green (these lock EXISTING Mono-ST/Mono behavior; the fixes of Task 2 must not change them).

- [ ] Step 3. Commit:
```bash
git add tests/sustain_audit.rs
git commit -m "test: audio-observable mono legato vs reattack envelopes (sustain continuity, first-note attack)"
```

---

### Task 5: Feature 1 gates

- [ ] Step 1.
```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo test 2>&1 | tail -8
cargo test --features rt-assert --test rt_safety 2>&1 | tail -3
cargo deny check 2>&1 | tail -3
```
Expected: all pass (`cargo deny` may print the pre-existing `warning[yanked]` for clipboard-win — warn-level, do not fix).

---

## FEATURE 2 — Arp Latch + BPM-sync proof

### Task 6: RED unit tests in `arpeggiator.rs` (latch semantics + BPM guard)

**Facts (anchor-verified):** src/synth/arpeggiator.rs — struct fields at 7-25 (`step_samples` private, same module → direct access from an in-file `#[cfg(test)] mod tests`); `set_params` 52-62 (clamps: `rate.min(5)`, `octaves.clamp(1,4)`); `set_bpm` 64-69; `note_on` 71-85; `note_off` 87-110 (last key off resets `position`/`tick_counter`/`pattern_len` at 101-105); `tick` 114-149; `has_held_notes` 151-153; `clear` 162-168; `recalc_timing` 170-185 (`step_samples = beats_per_step * 60 / bpm * sample_rate`; rate 1 = 0.5 beats → at 120 BPM/48 kHz = 12_000 samples).

- [ ] Step 1. Append to `src/synth/arpeggiator.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn arp() -> Arpeggiator {
        let mut a = Arpeggiator::new(48000.0);
        a.set_params(true, 0, 1, 1, 0.5); // Up, 1/8, 1 octave, gate 0.5
        a
    }

    /// Freeze an arp on the set {60, 64}.
    fn frozen() -> Arpeggiator {
        let mut a = arp();
        a.set_latch(true);
        a.note_on(60, 100);
        a.note_on(64, 100);
        a.note_off(60);
        a.note_off(64);
        a
    }

    #[test]
    fn latch_freezes_pattern_after_last_key_release() {
        let mut a = frozen();
        assert!(a.keeps_playing(), "latch must keep the pattern alive");
        assert_eq!(a.pattern_len, 2, "frozen pattern must survive key-off");
        let mut fired = 0;
        for _ in 0..48_000 {
            if a.tick().0.is_some() {
                fired += 1;
            }
        }
        assert!(fired >= 1, "frozen arp produced no notes in 1 s");
    }

    #[test]
    fn latch_new_chord_replaces_held_set() {
        let mut a = frozen();
        a.note_on(72, 100); // replaces the frozen set, playback continues
        assert!(!a.latched, "new key must unfreeze");
        assert_eq!(a.held_count, 1);
        assert_eq!(a.held_notes[0].0, 72);
        assert!(a.keeps_playing());
        assert_eq!(a.pattern_len, 1);
    }

    #[test]
    fn latch_off_while_frozen_stops() {
        let mut a = frozen();
        a.set_latch(false);
        assert!(!a.keeps_playing());
        assert_eq!(a.pattern_len, 0);
    }

    #[test]
    fn clear_resets_latch_state() {
        let mut a = frozen();
        a.clear();
        assert!(!a.latched);
        assert!(!a.keeps_playing());
    }

    #[test]
    fn no_latch_stops_on_last_release() {
        let mut a = arp(); // latch off — existing behavior
        a.note_on(60, 100);
        a.note_off(60);
        assert!(!a.keeps_playing());
        assert_eq!(a.pattern_len, 0);
    }

    #[test]
    fn rate_follows_bpm() {
        let a = arp(); // rate 1 = 1/8 = 0.5 beats
        assert_eq!(a.step_samples, 12_000, "0.5 * 60/120 * 48000");
        let mut a = arp();
        a.set_bpm(240.0);
        assert_eq!(a.step_samples, 6_000, "arp rate must track sequencer BPM");
    }
}
```

- [ ] Step 2. Run — expect compile failure on `set_latch`/`keeps_playing`/`latched` (fields/methods don't exist yet):
```bash
cargo test --lib rate_follows_bpm 2>&1 | tail -3
```
Expected: `error[E0599]`/`no method named set_latch` — the RED state. Commit:
```bash
git add src/synth/arpeggiator.rs
git commit -m "test: arp latch semantics + BPM sync guard — RED (no latch yet)"
```

---

### Task 7: Arpeggiator latch implementation + engine wiring

**Facts (anchor-verified):** enum variant `SetArpParams` at mod.rs:250; handler arm mod.rs:2366-2375 (last arm of the exhaustive match); `flush_arp_note_off` guard `if self.arpeggiator.has_held_notes()` at mod.rs:1442; CC120/123 panic block flush at mod.rs:1834-1837 with `return;` at 1838 and NO `arpeggiator.clear()`; AllNotesOff clears at mod.rs:2206-2211; patch-load arp sync at mod.rs:1960-1966; patch params arp block patch_params.rs:167-171 / 278 / 457-461 / 610-611; GUI Enabled-checkbox immediate-send pattern src/gui/params.rs:1394-1407 inside the arp panel 1388-1450.

- [ ] Step 1. `src/synth/arpeggiator.rs` — struct: after `rng_state: u32,` (line 24) add:
```rust
    /// Latch mode: the pattern keeps playing after all keys are released.
    latch: bool,
    /// Runtime "frozen" flag: latch set alive with zero physical keys held.
    latched: bool,
```
In `new()` initializer (after `rng_state: 0xDEAD_BEEF,`, line 46) add `latch: false, latched: false,`.

- [ ] Step 2. `note_on` (arpeggiator.rs:71-85) — insert at the top of the method body:
```rust
    pub fn note_on(&mut self, note: u8, velocity: u8) {
        if self.latch && self.latched {
            // Frozen latched set: a new chord replaces it in place — playback
            // continues from the current pattern position without a gap.
            self.held_count = 0;
            self.latched = false;
        }
        // Don't add duplicates
```
(keep the rest unchanged).

- [ ] Step 3. `note_off` (arpeggiator.rs:100-109) — replace the `if found { … }` block with:
```rust
        if found {
            if self.held_count == 0 {
                if self.latch {
                    // Freeze: keep the pattern sounding after the last key
                    // is released. Position/tick counter untouched — no gap.
                    self.latched = true;
                } else {
                    // Reset when all notes released (existing behavior)
                    self.position = 0;
                    self.tick_counter = 0;
                    self.pattern_len = 0;
                }
            } else {
                self.rebuild_pattern();
            }
        }
```

- [ ] Step 4. `clear()` (arpeggiator.rs:162-168) — add `self.latched = false;` as the last line. Then add the new methods after `has_held_notes` (arpeggiator.rs:151-153):
```rust
    /// The arp owns sound: keys physically held, or a latched (frozen) set.
    /// Gates flush_arp_note_off so the key-off that freezes a latched set
    /// does not kill the sounding voice.
    pub fn keeps_playing(&self) -> bool {
        self.has_held_notes() || (self.latch && self.latched)
    }

    pub fn latch_enabled(&self) -> bool {
        self.latch
    }

    pub fn set_latch(&mut self, latch: bool) {
        self.latch = latch;
        if !latch && self.latched {
            // Turning latch off while frozen: the held set is abandoned.
            self.latched = false;
            self.clear();
        }
    }
```

- [ ] Step 5. ControlEvent — in `src/synth/mod.rs` add after line 250 (`SetArpParams { … },`):
```rust
    /// Arp latch (TECHDEBT 14.4): true — arp keeps playing after keys are
    /// released; false — a frozen latched set is stopped immediately.
    SetArpLatch { latch: bool },
```

- [ ] Step 6. Handler — insert after the `SetArpParams` arm (mod.rs:2366-2375), keeping it the last arm:
```rust
            ControlEvent::SetArpLatch { latch } => {
                if !latch {
                    // Stop the frozen set first so the sounding arp voice
                    // gets a matching note-off before the set is cleared.
                    if let Some(off_note) = self.arpeggiator.flush_current_note() {
                        // Same transposed routing as flush_arp_note_off.
                        self.dispatch_arp_note_off(off_note);
                    }
                }
                self.arpeggiator.set_latch(latch);
            }
```

- [ ] Step 7. `flush_arp_note_off` guard (mod.rs:1441-1450) — replace
```rust
        if self.arpeggiator.has_held_notes() {
            return; // tick() still owns gate-offs
        }
```
with
```rust
        if self.arpeggiator.keeps_playing() {
            return; // tick() still owns gate-offs (includes a latched/frozen set)
        }
```

- [ ] Step 8. Panic CC120/123 (mod.rs:1834-1837) — after the flush `if let` block and BEFORE the `return;`, add:
```rust
                    // Panic must also drop held/latched arp notes — held
                    // notes surviving CC120 would resume the old pattern on
                    // the next key (AllNotesOff already clears, mod.rs:2211).
                    self.arpeggiator.clear();
```

- [ ] Step 9. Patch param `arp_latch` in `src/synth/patch_params.rs`:
  - after line 171 (`pub(super) arp_gate: f32,`): `pub(super) arp_latch: f32,`
  - in the defaults initializer (line 278, the `arp_enabled: 0.0, …` line): append `arp_latch: 0.0,`
  - in `from_map` (after line 461 `arp_gate: p("arp_gate", 0.5),`): `arp_latch: p("arp_latch", 0.0),`
  - in the serialization block (after line 611): `s!("arp_latch", self.arp_latch);`

- [ ] Step 10. Patch-load sync — in `handle_control`'s LoadPatch arm, after the `self.arpeggiator.set_params(…)` call (mod.rs:1961-1966), add:
```rust
                    self.arpeggiator.set_latch(params.arp_latch > 0.5);
```

- [ ] Step 11. Verify Task 6's unit tests pass:
```bash
cargo test --lib arpeggiator 2>&1 | tail -4
```
Expected: all green (latch tests + `rate_follows_bpm` + `no_latch_stops_on_last_release`).

- [ ] Step 12. Commit:
```bash
git add src/synth/arpeggiator.rs src/synth/mod.rs src/synth/patch_params.rs
git commit -m "feat: arp latch (freeze after key release, new chord replaces set) + SetArpLatch + CC120 clears held arp (TECHDEBT 14.4)"
```

---

### Task 8: GUI latch toggle + persistence + integration tests

**Facts (anchor-verified):** arp panel src/gui/params.rs:1388-1450; the `Enabled` checkbox (1394-1407) is the exact pattern: `push_undo_snapshot` + `edited_params.insert` + `changed = true` + immediate `ctrl_tx.push(ControlEvent::…)`. Persistence is free once `arp_latch` is a patch param (Task 7 Step 9): `changed = true` triggers `send_edited_params` on the outer redraw, and `PartConfig.param_overrides` carries the map verbatim (src/preset.rs:763-765). Integration harness helpers tests/sustain_audit.rs:10-64; `engine.arpeggiator` is a pub field (mod.rs:1086).

- [ ] Step 1. In `src/gui/params.rs`, inside the first `ui.horizontal(|ui| { … })` of the Arpeggiator section, directly after the `Enabled` checkbox `if … .changed() { … }` block (ends at line 1407), add:
```rust
                let mut latch = self.parts[part].edited_params.get("arp_latch").copied().unwrap_or(0.0) > 0.5;
                if ui.checkbox(&mut latch, "Latch").on_hover_text("Keep the arp playing after keys are released").changed() {
                    self.push_undo_snapshot();
                    self.parts[part].edited_params.insert("arp_latch".into(), if latch { 1.0 } else { 0.0 });
                    changed = true;
                    // Immediate live update (patch reload follows via send_edited_params)
                    let _ = self.ctrl_tx.push(crate::synth::ControlEvent::SetArpLatch { latch });
                }
```

- [ ] Step 2. Append integration tests to `tests/sustain_audit.rs`:
```rust
// ── Arp latch (TECHDEBT 14.4) ──────────────────────────────────

fn latched_arp_engine() -> SynthEngine {
    let mut e = engine_with_init_patch();
    let mut patch = init_patch_mut();
    patch.params.insert("arp_enabled".to_string(), 1.0);
    patch.params.insert("arp_rate".to_string(), 2.0); // 1/16 ≈ 125 ms steps @120 BPM
    patch.params.insert("arp_gate".to_string(), 0.5);
    patch.params.insert("arp_latch".to_string(), 1.0);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    e
}

#[test]
fn arp_latch_keeps_playing_after_key_release() {
    let mut e = latched_arp_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 200) > SILENCE, "arp produced no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let latched_peak = render_peak(&mut e, 300); // ~200 ms — spans further steps
    assert!(
        latched_peak > SILENCE,
        "latch did not keep the arp alive after key release (peak {latched_peak})"
    );
    // Second latch-toggle press (latch off) stops the frozen set
    let _ = e.handle_control(ControlEvent::SetArpLatch { latch: false });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "latch-off left sound ringing (peak {after})");
}

#[test]
fn arp_latch_new_chord_takes_over() {
    let mut e = latched_arp_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 100);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    let _ = render_peak(&mut e, 100);
    assert!(render_peak(&mut e, 100) > SILENCE, "set did not freeze");
    // New chord replaces the frozen set without stopping playback
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let _ = render_peak(&mut e, 50);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    let peak = render_peak(&mut e, 300);
    assert!(peak > SILENCE, "new chord broke the latched arp (peak {peak})");
    let _ = e.handle_control(ControlEvent::SetArpLatch { latch: false });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "latch-off left sound ringing (peak {after})");
}

#[test]
fn panic_cc_clears_latched_arp() {
    let mut e = latched_arp_engine();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = render_peak(&mut e, 100);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 100) > SILENCE, "set did not freeze");
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    settle(&mut e);
    let after = render_peak(&mut e, 200);
    assert!(after < SILENCE, "CC120 left the latched arp ringing (peak {after})");
}

#[test]
fn arp_latch_persists_through_patch_load() {
    let e = latched_arp_engine();
    assert!(e.arpeggiator.latch_enabled(), "arp_latch param did not sync on patch load");
    // Absent key defaults to off
    let patches = preset::load_all_patches();
    let patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .expect("init preset exists")
        .clone();
    let mut e2 = SynthEngine::new(SR);
    e2.set_patches(patches);
    let _ = e2.handle_control(ControlEvent::load_patch_from(0, &patch));
    assert!(!e2.arpeggiator.latch_enabled(), "default latch must be off");
}
```

- [ ] Step 3. Run:
```bash
cargo test --test sustain_audit arp_latch panic_cc_clears 2>&1 | tail -6
cargo test 2>&1 | tail -6
```
Expected: all green; whole suite green (in particular `arp_transposed_note_off_stops_sound` and `mono_pedal_up_clears_note_stack` prove non-latch paths unchanged).

- [ ] Step 4. RT-safety check (GUI-side push is a `CountingProducer::push`; handler is plain data):
```bash
cargo test --features rt-assert --test rt_safety 2>&1 | tail -3
```

- [ ] Step 5. Commit:
```bash
git add src/gui/params.rs tests/sustain_audit.rs
git commit -m "feat: arp Latch toggle in arp panel (persisted arp_latch param) + latch integration tests"
```

---

### Task 9: Docs + full gates

- [ ] Step 1. In `docs/TECHDEBT.md`, mark both items done in the repo's style (a `РЕАЛИЗОВАНА` status line under each heading, like §15):
  - Under `### 14.4 Arp live-контролы…`: add `— РЕАЛИЗОВАНА (частично) 2026-10-29: latch + panic-очистка + arp_latch парам; rate sync уже был безусловным (set_bpm из sequencer.bpm каждый блок, mod.rs tick_block); transpose арпа — отдельный пункт.`
  - Under `### 14.5 Mono Legato…`: add `— DSP-часть РЕАЛИЗОВАНА 2026-10-29: mono/legato/glide жили в patch params (play_mode 1/2, portamento_time/mode); добавлены SetPartMono/Legato/Glide live-события; исправлены баги (mono-путь игнорировал portamento_mode; retrigger_note не задавал porta_coeff — glide щёлкал мгновенно). SF2 legato/glide (skip-attack через startAddrsOffset, glide через pitch bend) — осталось, отдельный пункт.`

- [ ] Step 2. Full gates:
```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo test 2>&1 | tail -8
cargo test --features rt-assert --test rt_safety 2>&1 | tail -3
cargo deny check 2>&1 | tail -3
```
Expected: all pass. Do NOT run `cargo fmt` (project rule).

- [ ] Step 3. Commit:
```bash
git add docs/TECHDEBT.md
git commit -m "docs: mark TECHDEBT 14.4/14.5 (DSP part) implemented; note remaining SF2 legato work"
```
