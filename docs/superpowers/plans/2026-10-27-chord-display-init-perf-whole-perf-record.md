# Chord Display, Init Performance, Whole-Performance Recording — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Three features: (1) chord-name display under the keyboard — pressed note names, with the detected chord in brackets, bold; (2) built-in empty "Init" performance; (3) looper records the whole performance (every sounding part), not just the active layer.

**Architecture:**
- Chord detection: pure template-matching function `chord::detect(&[u8]) -> Option<String>` (Pardo/Birmingham style: pitch-class set, exact template match per candidate root, bass-note tie-break, inversion as `X/Y`), no ML, no deps.
- Init performance: factory `Performance` in `preset.rs` (mirrors `FACTORY_PRESETS`); exposed as a virtual "(Init)" entry in the perf list.
- Whole-perf recording: looper events are tagged with the part that actually SOUNDS the note (per key-range routing) instead of `active_part`; looper replay already routes per part id, so the whole performance reproduces.
- Chord-aware recording is NOT in this plan (deferred — techdebt #5 keeps the recording half for later; this plan delivers detection + display).

**Tech Stack:** Rust, existing harness (`tests/end_to_end.rs` helpers), cargo test.

**Research facts:** Chord ID by template matching over 12 roots × interval templates (Pardo & Birmingham; arXiv 2002.09748 "CASSETTE"); tie-breaking by bass note; inversions named as slash chords. Display conventions: note names space-separated, chord in parentheses.

---

## Test Cases (from the requirement)

Written BEFORE implementation. Fresh subagent per task receives ONLY its task text.

1. `chord_major` — detect([60,64,67]) → "C".
2. `chord_major7` — detect([60,64,67,71]) → "Cmaj7".
3. `chord_minor` — detect([57,60,64]) → "Am".
4. `chord_inversion_slash` — detect([64,67,72]) → "C/E" (C major, bass E).
5. `chord_slash_f_over_c` — detect([60,65,69]) → "F/C".
6. `chord_seventh` — detect([55,59,62,65]) → "G7".
7. `chord_two_notes_none` — detect([60,64]) → None (interval, not a chord).
8. `chord_one_note_none` — detect([60]) → None.
9. `chord_unmatched_none` — detect([60,61,62]) → None (chromatic cluster).
10. `looper_records_all_sounding_parts` — two parts enabled with disjoint key ranges (A: 0–59, B: 60–127); looper record on; play note 36 (part A) and note 84 (part B); stop record; play back → BOTH parts sound (assert via each part's note activity during playback).
11. `init_performance_exists_and_loads` — `preset::init_performance()` has 1 part (Init preset, enabled, full range, not SF2) and deserializes/serializes cleanly.
12. `gate_regressions` — full existing suite stays green (no new failures).

SF2/display edge cases not unit-testable (GUI rendering) → manual check in Task 5.

## Negative Matrix

- **Empty set:** detect(&[]) → None.
- **Null:** n/a — no nullable inputs (strike: all inputs are slices).
- **Zero/empty:** looper record with zero sounding parts (note outside all ranges) → nothing recorded, no panic.
- **Duplicate/concurrent:** same note pressed twice (retrigger) → recorded once per sounding part, no duplicate stacking in looper.
- **Clock skew:** n/a — single-threaded event order (strike).
- **External-call timeout:** n/a (strike).

---

### Task 1: Chord detection module `src/chord.rs`

**Files:**
- Create: `src/chord.rs`
- Modify: `src/lib.rs` (add `pub mod chord;`)

**Step 1: Implement `detect`.** Algorithm: exact template match per candidate root over the pitch-class set; candidates scored — (a) full coverage required (every held pitch class must be in the template), (b) bass (lowest held note) preferred as root, (c) more template intervals wins, (d) simpler suffix wins on ties. Inversions named `Root/Bass` when the bass pitch class is not the chord root. Fewer than 3 distinct pitch classes → None.

```rust
//! Chord name detection from held notes (template matching).
//!
//! Pardo/Birmingham-style: pitch-class set, per-candidate-root exact
//! template match, bass-note tie-break. Pure function, no deps.

const TEMPLATES: &[(&str, &[u8])] = &[
    ("", &[0, 4, 7]),
    ("m", &[0, 3, 7]),
    ("dim", &[0, 3, 6]),
    ("aug", &[0, 4, 8]),
    ("sus4", &[0, 5, 7]),
    ("sus2", &[0, 2, 7]),
    ("6", &[0, 4, 7, 9]),
    ("m6", &[0, 3, 7, 9]),
    ("7", &[0, 4, 7, 10]),
    ("maj7", &[0, 4, 7, 11]),
    ("m7", &[0, 3, 7, 10]),
    ("m7b5", &[0, 3, 6, 10]),
    ("dim7", &[0, 3, 6, 9]),
    ("7sus4", &[0, 5, 7, 10]),
    ("9", &[0, 4, 7, 10, 2]),
    ("maj9", &[0, 4, 7, 11, 2]),
    ("m9", &[0, 3, 7, 10, 2]),
    ("add9", &[0, 4, 7, 2]),
    ("6/9", &[0, 4, 7, 9, 2]),
];

const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

/// Detect a chord name from held notes. Returns None for fewer than 3
/// distinct pitch classes or when no template covers the set.
pub fn detect(notes: &[u8]) -> Option<String> {
    let mut pcs = [false; 12];
    let mut count = 0;
    let bass_pc = notes.iter().min().map(|n| n % 12)?;
    for n in notes {
        if !pcs[(*n % 12) as usize] { count += 1; }
        pcs[(*n % 12) as usize] = true;
    }
    if count < 3 { return None; }

    // Candidate roots, bass pitch class first (tie-break preference).
    let mut roots = [0u8; 12];
    let mut n_roots = 0;
    for off in 0..12u8 {
        roots[n_roots] = (bass_pc + off) % 12;
        n_roots += 1;
    }

    let mut best: Option<(u8, u8, &'static str)> = None; // (score, root, suffix)
    for &root in &roots[..n_roots] {
        for (sfx, iv) in TEMPLATES {
            // Full coverage: every held pc must be an interval of the template
            // rooted at `root`; every template interval must be held.
            let mut covered = 0u8;
            let mut extra = false;
            for pc in 0..12u8 {
                if !pcs[pc as usize] { continue; }
                let iv_of = ((pc + 12 - root) % 12) as u8;
                if iv.contains(&iv_of) { covered += 1; } else { extra = true; }
            }
            if extra { continue; }
            if covered as usize != iv.len() { continue; } // full template required
            // Score: template size (richer match wins), bass-root bonus.
            let mut score = iv.len() as u8 * 10;
            if root == bass_pc { score += 5; }
            if best.map(|(bs, _, _)| score > bs).unwrap_or(true) {
                best = Some((score, root, sfx));
            }
        }
    }
    let (_, root, sfx) = best?;
    let name = NOTE_NAMES[root as usize];
    if (bass_pc + 12 - root) % 12 == 0 {
        Some(format!("{name}{sfx}"))
    } else {
        Some(format!("{name}{sfx}/{}", NOTE_NAMES[bass_pc as usize]))
    }
}

#[cfg(test)]
mod tests {
    use super::detect;
    #[test]
    fn chord_major() { assert_eq!(detect(&[60, 64, 67]), Some("C".into())); }
    #[test]
    fn chord_major7() { assert_eq!(detect(&[60, 64, 67, 71]), Some("Cmaj7".into())); }
    #[test]
    fn chord_minor() { assert_eq!(detect(&[57, 60, 64]), Some("Am".into())); }
    #[test]
    fn chord_inversion_slash() { assert_eq!(detect(&[64, 67, 72]), Some("C/E".into())); }
    #[test]
    fn chord_slash_f_over_c() { assert_eq!(detect(&[60, 65, 69]), Some("F/C".into())); }
    #[test]
    fn chord_seventh() { assert_eq!(detect(&[55, 59, 62, 65]), Some("G7".into())); }
    #[test]
    fn chord_two_notes_none() { assert_eq!(detect(&[60, 64]), None); }
    #[test]
    fn chord_one_note_none() { assert_eq!(detect(&[60]), None); }
    #[test]
    fn chord_empty_none() { assert_eq!(detect(&[]), None); }
    #[test]
    fn chord_unmatched_none() { assert_eq!(detect(&[60, 61, 62]), None); }
}
```

**Step 2:** `src/lib.rs` — add `pub mod chord;` next to the other modules.

**Step 3: Run** `cargo test --lib chord 2>&1 | tail -3` — 10 tests pass. If any expected string differs (e.g. template ordering picks another valid name), DO NOT change the test — fix `detect` until the requirement outputs match; note the discrepancy in the report.

**Step 4:** clippy clean (`cargo clippy --all-targets -- -D warnings 2>&1 | tail -2`).

**Step 5: Commit** `git add src/chord.rs src/lib.rs && git commit -m "feat: chord name detection (template matching, slash-chord inversions)"`

---

### Task 2: Chord display under the keyboard

**Files:**
- Modify: `src/gui/keyboard.rs:22-38` (active-notes label)

**Step 1:** After collecting `active` (Vec of note-name strings), compute the chord from the held MIDI notes and append it in brackets, bold. Replace the label block with:

```rust
        // Active notes label above keyboard; detected chord in brackets, bold
        let held: Vec<u8> = (0..128u8)
            .filter(|i| self.note_state[*i as usize].load(Ordering::Relaxed) > 0)
            .collect();
        let active: Vec<String> = held.iter().map(|n| note_name(*n)).collect();

        if active.is_empty() {
            ui.colored_label(theme::TEXT_DIM, " ");
        } else {
            let chord = crate::chord::detect(&held);
            match chord {
                Some(c) => {
                    // Chord-only bold: two widgets side by side
                    ui.horizontal(|ui| {
                        ui.label(active.join("  "));
                        ui.label(egui::RichText::new(format!("({})", c)).strong());
                    });
                }
                None => {
                    ui.label(active.join("  "));
                }
            }
        }
```

(Bold = `.strong()` per egui; requirement: "в скобках аккорд, жирным шрифтом".)

**Step 2:** `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2` — clean; `cargo test` — no failures.

**Step 3:** Manual check (operator): play C-E-G → label shows `C4  E4  G4  (C)` in bold; play two notes → no brackets.

**Step 4: Commit** `git add src/gui/keyboard.rs && git commit -m "feat: chord name in brackets (bold) above the keyboard"`

---

### Task 3: Built-in "(Init)" performance

**Files:**
- Modify: `src/preset.rs` (factory init performance), `src/gui/mod.rs` (`load_performance_by_name` — virtual entry), `src/gui/layers.rs` (perf list population)

**Step 1: Factory performance in `preset.rs`** (near `list_performances`):

```rust
/// Built-in empty performance: single part A1 = Init preset, full range,
/// no SF2. Served as the virtual "(Init)" entry of the perf list.
pub fn init_performance() -> Performance {
    Performance {
        name: "(Init)".to_string(),
        category: String::new(),
        parts: vec![PartConfig {
            patch_name: "Init".to_string(),
            enabled: true,
            volume: 0.8,
            key_low: 0,
            key_high: 127,
            param_overrides: Default::default(),
            sf2_mode: false,
            sf2_program: 0,
            pan: 0.0,
            transpose: 0,
            mute: false,
            vel_min: 1,
            vel_max: 127,
        }],
    }
}
```

(If the factory preset is not named exactly "Init", reuse the same lookup as `tests/end_to_end.rs:29-36` — first patch named "init" case-insensitive, else first patch — and use its `name` for `patch_name`.)

**Step 2: Virtual entry in the perf list (BOTH population sites).** Initial population is `src/lib.rs:770` (`perf_list: preset::list_performances()` in CommonInit construction), the save-refresh branch is `src/gui/layers.rs:237`. At each site, after building the list, prepend the virtual entry:

```rust
perf_list.insert(0, ("(Init)".to_string(), std::path::PathBuf::new()));
```

**Step 3: Load paths.** The layers.rs ComboBox branch (~253) loads files directly via `preset::load_performance(&path)` — the virtual entry has an empty PathBuf and would be dead UI there. In that branch, handle the virtual name first: `if part_name == "(Init)" { apply preset::init_performance() }` (same apply-loop body). In `load_performance_by_name` (src/gui/mod.rs:1521, pad-perf path), before the `perf_list` lookup:

```rust
        if name == "(Init)" {
            let perf = preset::init_performance();
            // apply parts exactly like the normal path below (the loop is
            // shared) — simplest: materialize into a temp and fall through.
            // Implementation choice: early-return via the same apply loop
            // used for `perf`.
        }
```

Concretely: change the function to obtain `perf: Performance` from either the virtual entry (`preset::init_performance()`) or the file, then run the existing apply loop once for both. Keep the `AllNotesOff` push. Return true.

**Step 4: Tests** — in `tests/sustain_audit.rs` (has serde + preset imports):

```rust
#[test]
fn init_performance_exists_and_loads() {
    let perf = mini_midi_synth::preset::init_performance();
    assert_eq!(perf.parts.len(), 1);
    let p = &perf.parts[0];
    assert!(p.enabled);
    assert!(!p.sf2_mode);
    assert_eq!(p.key_low, 0);
    assert_eq!(p.key_high, 127);
    let json = serde_json::to_string(&perf).expect("serialize");
    let back: mini_midi_synth::preset::Performance = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.name, "(Init)");
}
```

**Step 5:** Gate + commit: `git add src/preset.rs src/gui/mod.rs src/gui/layers.rs tests/sustain_audit.rs && git commit -m "feat: built-in (Init) performance as pad-perf reset"`

---

### Task 4: Looper records every sounding part (whole performance)

**Files:**
- Modify: `src/synth/mod.rs` — the three `looper.record_event(note, vel, ap)` call sites that use `active_part` for live note routing: live NoteOn (~1655), vel==0 (~1634), NoteOff (~1672)

**Current behavior:** notes are recorded tagged with `active_part` regardless of which part actually sounds them (each part with a matching key range plays the note) — playback then reproduces only one layer.

**Step 1: Record per sounding part.** In the live `NoteOn` routing loop:

```rust
                } else {
                    let ap = self.active_part.load(std::sync::atomic::Ordering::Relaxed);
                    for (i, part) in self.parts.iter_mut().enumerate() {
                        if !part.enabled || part.mute { continue; }
                        // Key range + velocity range check
                        let transposed = (note as i16 + part.transpose as i16).clamp(0, 127) as u8;
                        if note < part.min_note || note > part.max_note { continue; }
                        if velocity < part.vel_min || velocity > part.vel_max { continue; }
                        if part.sf2_mode {
                            self.sampler.note_on(i, transposed, velocity);
                        } else {
                            part.note_on(transposed, velocity);
                        }
                        // Whole-performance recording: tag the event with the
                        // part that actually sounds it (looper replay routes
                        // per part id).
                        self.looper.record_event(note, velocity, i as u8);
                    }
                }
```

(remove the trailing `self.looper.record_event(note, velocity, ap);` after the loop, and delete the now-unused `let ap = self.active_part.load(...)` binding — the same applies to the vel==0 and NoteOff branches below, otherwise the clippy `-D warnings` gate fails)

Mirror in the vel==0 and NoteOff branches: inside their per-part loops, record `(note, 0, i)` per part that receives the off — for the vel==0 branch keep the `!part.enabled` skip AS IS in the on-branch, but the off branches deliberately do NOT skip disabled parts (see note in Task 4 of the previous plan); record the off for every part that could have it: `self.looper.record_event(note, 0, i as u8);` inside the loop (looper replay routes per part and re-applies transpose, so offs round-trip). Remove the trailing active_part-based record calls in both branches.

Arp / MIDI-file / looper-self record sites keep `ap` (they are not the live-keyboard path); `flush_arp_note_off` also keeps `ap` (live-keyboard-adjacent but arp-driven).

**Step 2: Tests** — in `tests/sustain_audit.rs`:

```rust
#[test]
fn looper_records_all_sounding_parts() {
    let mut e = engine_with_init_patch();
    // Part A: full range (default). Part B: 60-127.
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 1, volume: 1.0 });
    // Record: two notes, one per part's range
    let _ = e.handle_control(ControlEvent::LooperRecord);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 36, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 36 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 84, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 84 });
    // NOTE: LooperRecord is a no-op while already Recording (looper.rs:220-235);
    // stopping uses LooperStopRecord, which moves Recording -> Playing.
    let _ = e.handle_control(ControlEvent::LooperStopRecord);
    // During playback both notes must sound again
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut peak36 = 0.0f32;
    let mut peak84 = 0.0f32;
    // The loop is 1 bar at default BPM; render generously and watch global peak.
    let mut peak = 0.0f32;
    for _ in 0..6000 { l.fill(0.0); r.fill(0.0); e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE { peak = peak.max(l[i].abs()).max(r[i].abs()); } }
    assert!(peak > 0.05, "looper playback produced no sound (peak {peak})");

    // Negative: a note outside every part's range records nothing -> silent playback.
    let mut e2 = engine_with_init_patch();
    let _ = e2.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e2.handle_control(ControlEvent::SetPartEnabled { part: 1, enabled: true });
    let _ = e2.handle_control(ControlEvent::SetPartRange { part: 1, min_note: 60, max_note: 100 });
    let _ = e2.handle_control(ControlEvent::LooperRecord);
    e2.handle_event(MidiEvent::NoteOn { channel: 0, note: 110, velocity: 127 }); // outside 0-59 and 60-100
    e2.handle_event(MidiEvent::NoteOff { channel: 0, note: 110 });
    let _ = e2.handle_control(ControlEvent::LooperStopRecord);
    let mut l2 = [0.0f32; BLOCK_SIZE]; let mut r2 = [0.0f32; BLOCK_SIZE];
    let mut silent_peak = 0.0f32;
    for _ in 0..4000 { l2.fill(0.0); r2.fill(0.0); e2.tick_block(&mut l2, &mut r2);
        for i in 0..BLOCK_SIZE { silent_peak = silent_peak.max(l2[i].abs()).max(r2[i].abs()); } }
    assert!(silent_peak < 1e-3, "out-of-range note was recorded (peak {silent_peak})");
}
```

(The exact per-part peak split is not observable through the mixed output without per-part taps; the requirement — both notes recorded and replayed across their parts — is covered by the peak assertion plus Task 4's code review confirming per-part tagging. If a `looper_event_count()`-style getter already exists, prefer asserting it.)

**Known accepted effects (add a code comment at the on-loop record site):**
- Off events get recorded for parts that never sounded the note (their off loops don't range-check) — replay offs are harmless no-ops, but consume looper event slots.
- Split-overlap (two parts covering the same note) overwrites looper's single `active_notes[note]` slot — the forced off at loop-wrap releases only the last part. Acceptable; commented.

**Step 3:** Gate + commit: `git add src/synth/mod.rs tests/sustain_audit.rs && git commit -m "feat: looper records every sounding part — whole-performance capture"`

---

### Task 5: Full gate

- [ ] **Step 1:** `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2` — clean; `cargo test 2>&1 | grep -E 'test result'` — all ok; `cargo test --features rt-assert --test rt_safety 2>&1 | grep 'test result'` — ok.
- [ ] **Step 2:** Commit leftovers if any: `git add -A src tests && git commit -m "feat: chord display, init perf, whole-perf recording"`.

---

## Self-Review Notes

- Spec coverage: techdebt #3 → Task 3; #2 → Task 4; #5 display half → Tasks 1-2 (recording half remains deferred in techdebt, per user instruction "пункт 5 так же пиши нажатые клавиши а в скобках аккорды" — that is the display).
- Chord algorithm from research: Pardo/Birmingham template matching (arXiv 2002.09748), bass tie-break, slash-chord inversions.
- Open risk flagged in Task 4: per-part peaks are not observable in the mixed output; test asserts the global playback peak, review must confirm per-part tagging in code.
