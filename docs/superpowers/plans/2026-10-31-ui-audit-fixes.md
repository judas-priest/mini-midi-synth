# UI Audit Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-10-31
status: executed 2026-10-31 (auto-repeat fix follow-up включён; экранная верификация T12 step 2 — за человеком)

**Goal:** Fix the actionable findings of the UI audit `docs/UI_AUDIT_2026-10-31.md`: contrast below 3:1, clipped XY-pad labels, RU/EN mixing, unconfirmed hotkey `X`, hidden slider values, unreachable pitch sequencer, inconsistent destructive-action confirmation, split drum palettes, color-only state, full-screen modals for reversible choices.

**Architecture:** All changes are GUI-layer (`src/gui/`). New: `src/gui/confirm.rs` (two-stage destructive-button helper shared by buttons and hotkeys). Theme constants in `src/gui/theme.rs` become the single source of muted-state and drum-category colors; a unit test module enforces WCAG contrast arithmetic so the values cannot regress. No audio-thread code is touched (`src/synth/`, `src/audio.rs` untouched → `rt_safety` run is a regression check only).

**Tech Stack:** Rust, eframe/egui 0.31 (`egui::Modal`, `egui::Window`, `StrokeKind::Inside`, `RichText::strikethrough()` — все доступны в 0.31.1, сверено с Cargo.lock).

**Spec:** `docs/UI_AUDIT_2026-10-31.md` — Quick wins 1–5, Strategic 1–6, plus two cheap Medium items (п.11 pitch-label clipping/contrast, п.5 key-zone map).

---

## Test Cases (from the requirement)

The requirement is the audit text. Testable assertions, written BEFORE implementation:

1. **TC1 muted-state contrast** — `SEQ_GATE_OFF`, `LOOPER_LAYER_MUTED`, `MIDI_SEQ_MUTE_INACTIVE`, `MIDI_SEQ_LOOP_OFF` each have WCAG contrast ≥ 3:1 against `BG_PANEL` (audit п.5: «поднять 4 константы ниже 3:1»).
2. **TC2 key-zone contrast** — every `PART_COLORS[i]` has contrast ≥ 3:1 against `BG_TIMELINE` (audit п.5, key-zone map: dimmed inactive zone invisible).
3. **TC3 drum palette unity** — the six pad-category constants are literally the corresponding `DRUM_COLORS` entries (audit п.10: «единый источник цветов»).

Assertions 1–2 live in `theme.rs` `#[cfg(test)]` from Task 1; assertion 3 is added in Task 3. Everything else in this plan is visual/behavioral GUI code — verified per task by `cargo run` + a manual checklist step, not unit tests (egui drawing has no headless test harness in this repo).

## Negative Matrix

- **missing field** — N/A: no parsing/serialization touched.
- **null/None** — `Option<Instant>` confirm state starts `None`; `confirm_button` treats `None` as disarmed (explicit code path). Covered.
- **zero/empty** — Clear with an empty looper: gated by `add_enabled(event_count > 0 || state != Idle)` (kept). Drum Clear on an empty grid pushes a no-op event — unchanged pre-existing behavior.
- **clock skew** — N/A: single-process `Instant`, one clock.
- **duplicate/concurrent** — double-click inside the 2 s window: first click arms, second confirms, third re-arms (arm is consumed on confirm). Manual check in Task 10.
- **external-call timeout** — N/A: no new I/O; `ctrl_tx.push` is a non-blocking ring push.
- **timer expiry between clicks** — expired arming is disarmed at the top of `confirm_button`; the next click re-arms instead of firing. Explicit code path.

---

### Task 1: WCAG contrast test module in theme.rs (failing first)

**Files:**
- Modify: `src/gui/theme.rs` (append at end of file)

- [x] **Step 1: Write the failing tests**

Append to `src/gui/theme.rs`:

```rust

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rel_luminance(c: Color32) -> f64 {
        fn ch(v: u8) -> f64 {
            let s = v as f64 / 255.0;
            if s <= 0.04045 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * ch(c.r()) + 0.7152 * ch(c.g()) + 0.0722 * ch(c.b())
    }

    /// WCAG 2.x contrast ratio.
    fn contrast(a: Color32, b: Color32) -> f64 {
        let (la, lb) = (rel_luminance(a), rel_luminance(b));
        let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Audit Medium п.5: muted/inactive state tokens must meet WCAG 1.4.11 (≥3:1).
    #[test]
    fn muted_state_tokens_meet_non_text_contrast() {
        let cases: [(&str, Color32, Color32); 4] = [
            ("SEQ_GATE_OFF", SEQ_GATE_OFF, BG_PANEL),
            ("LOOPER_LAYER_MUTED", LOOPER_LAYER_MUTED, BG_PANEL),
            ("MIDI_SEQ_MUTE_INACTIVE", MIDI_SEQ_MUTE_INACTIVE, BG_PANEL),
            ("MIDI_SEQ_LOOP_OFF", MIDI_SEQ_LOOP_OFF, BG_PANEL),
        ];
        for (name, fg, bg) in cases {
            let c = contrast(fg, bg);
            assert!(c >= 3.0, "{name}: {c:.2}:1 < 3:1 (WCAG 1.4.11)");
        }
    }

    /// Audit Medium п.5: every key-zone color must stay visible on the map
    /// background. The map paints BG_PANEL (layers.rs:516), not BG_TIMELINE —
    /// guard the pairing the code actually uses.
    #[test]
    fn key_zone_colors_meet_non_text_contrast() {
        for (i, c) in PART_COLORS.iter().enumerate() {
            let cr = contrast(*c, BG_PANEL);
            assert!(cr >= 3.0, "PART_COLORS[{i}]: {cr:.2}:1 < 3:1 on BG_PANEL");
        }
    }
}
```

- [x] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib gui::theme`
Expected: COMPILE ERROR — `MIDI_SEQ_LOOP_OFF` does not exist (it is added in Task 2). That is the red state. (TC2 itself passes arithmetically already — `PART_COLORS` are full colors; the failing piece is only the missing constant.)

- [x] **Step 3: Do NOT commit yet**

The tree is red until Task 2 adds the constants. Commit the tests together with the implementation in Task 2 Step 6 (keeps every commit green for bisect).

---

### Task 2: Raise muted-state tokens ≥3:1; key-zone map without dimming

**Files:**
- Modify: `src/gui/theme.rs:179,187,195` (constants)
- Modify: `src/gui/midi_seq.rs:35-40` (Loop-off color)
- Modify: `src/gui/layers.rs:511-537` (key-zone map)

- [x] **Step 1: Change the three constants and add the fourth**

In `src/gui/theme.rs`:

```rust
/// Gate-off indicator color (non-text fill, ≥3:1 on BG_PANEL — WCAG 1.4.11;
/// was gray(50) = 1.30:1).
pub const SEQ_GATE_OFF: Color32 = Color32::from_gray(110);
```

```rust
/// Mute button inactive (text — ≥4.5:1 on BG_PANEL, WCAG 1.4.3; was gray(80) = 2.1:1).
pub const MIDI_SEQ_MUTE_INACTIVE: Color32 = Color32::from_gray(140);
```

```rust
/// Loop button inactive (text — ≥4.5:1; was egui DARK_GRAY = gray(40) ≈ 1.15:1).
pub const MIDI_SEQ_LOOP_OFF: Color32 = Color32::from_gray(140);
```

```rust
/// Muted layer label (text — ≥4.5:1; was gray(80) = 2.1:1).
pub const LOOPER_LAYER_MUTED: Color32 = Color32::from_gray(140);
```

(Keep each constant in its existing section: `SEQ_GATE_OFF` in pitch/step-sequencer, `MIDI_SEQ_*` in MIDI-sequencer, `LOOPER_LAYER_MUTED` in looper. Replace the old definitions, do not duplicate.)

Contrast math (sRGB): `SEQ_GATE_OFF` is a filled rect → non-text 3:1 suffices: gray(110) on gray(30) = 3.27:1. The other three render text <18pt → 4.5:1: gray(140) on gray(30) = 4.96:1, on gray(27) = 5.12:1. All also clear 3:1 (1.4.11).

- [x] **Step 2: Use the new Loop-off constant**

In `src/gui/midi_seq.rs:35-40`, replace:

```rust
            let loop_col = if self.midi_seq_looping {
                theme::MIDI_SEQ_LOOP_ON
            } else {
                egui::Color32::DARK_GRAY
            };
```

with:

```rust
            let loop_col = if self.midi_seq_looping {
                theme::MIDI_SEQ_LOOP_ON
            } else {
                theme::MIDI_SEQ_LOOP_OFF
            };
```

- [x] **Step 3: Key-zone map — full-color zones, active marked by stroke after the octave lines**

In `src/gui/layers.rs`, function `draw_key_zone_map` (lines 511–537). Replace the parts loop body (the `let color = if i == active … from_rgba_premultiplied` branch is deleted, zones fill at full color):

```rust
    for (i, part) in parts.iter().enumerate() {
        if !part.enabled { continue; }
        let c = colors[i % colors.len()];
        // Полный цвет у всех зон: приглушённые неактивные зоны падали ниже
        // 3:1 по фону (audit Medium п.5). Активная зона — белая обводка
        // (неколоровый признак, WCAG 1.4.1), а не затемнение.
        let lo = part.min_note as f32 / 127.0;
        let hi = (part.max_note as f32 + 1.0) / 127.0;
        let zone = egui::Rect::from_min_max(
            egui::pos2(rect.left() + lo * total_w, rect.top()),
            egui::pos2((rect.left() + hi * total_w).min(rect.right()), rect.bottom()),
        );
        painter.rect_filled(zone, 0.0, c);
    }
```

Keep the existing octave-separator loop (lines 530–536) as is, and AFTER it append the stroke loop — after, so the gray(90) dividers don't cut through the white outline:

```rust
    // Активная зона поверх октавных разделителей (иначе серые линии 0.5px
    // прорезают белую обводку на границах, кратных 12).
    for (i, part) in parts.iter().enumerate() {
        if !part.enabled || i != active { continue; }
        let lo = part.min_note as f32 / 127.0;
        let hi = (part.max_note as f32 + 1.0) / 127.0;
        let zone = egui::Rect::from_min_max(
            egui::pos2(rect.left() + lo * total_w, rect.top()),
            egui::pos2((rect.left() + hi * total_w).min(rect.right()), rect.bottom()),
        );
        painter.rect_stroke(
            zone, 0.0,
            egui::Stroke::new(1.5, egui::Color32::WHITE),
            egui::StrokeKind::Inside,
        );
    }
```

- [x] **Step 4: Run tests to verify TC1 and TC2 pass**

Run: `cargo test --lib gui::theme`
Expected: both tests PASS (`muted_state_tokens_meet_non_text_contrast`, `key_zone_colors_meet_non_text_contrast`) — 2 tests total.

- [x] **Step 5: Visual check**

Run: `cargo run` — key-zone map: all zones vivid, active outlined white (no dark hairline through the outline at octave boundaries). Drum grid gate-off cells and MIDI-seq M/Loop buttons visibly gray but legible.

- [x] **Step 6: Commit (tests + implementation together)**

```bash
git add src/gui/theme.rs src/gui/midi_seq.rs src/gui/layers.rs
git commit -m "fix: raise muted-state colors to WCAG 3:1, stroke active key-zone (audit п.5)"
```

---

### Task 3: Single source for drum pad category colors

**Files:**
- Modify: `src/gui/theme.rs` (add 6 consts + test)
- Modify: `src/gui/keyboard.rs:136-158` (pad_color closure)

- [x] **Step 1: Add the failing test**

Append inside `mod tests` in `src/gui/theme.rs`:

```rust
    /// Audit Medium п.10: pads and step grid must share one palette source.
    #[test]
    fn drum_category_colors_derive_from_grid_palette() {
        assert_eq!(DRUM_CAT_KICK, DRUM_COLORS[0]);
        assert_eq!(DRUM_CAT_SNARE, DRUM_COLORS[2]);
        assert_eq!(DRUM_CAT_HATS, DRUM_COLORS[6]);
        assert_eq!(DRUM_CAT_CYMBALS, DRUM_COLORS[13]);
        assert_eq!(DRUM_CAT_TOMS, DRUM_COLORS[9]);
        assert_eq!(DRUM_CAT_PERC, DRUM_COLORS[3]);
    }
```

Run: `cargo test --lib gui::theme`
Expected: COMPILE ERROR — `DRUM_CAT_*` not found.

- [x] **Step 2: Add the constants**

In `src/gui/theme.rs`, after the `DRUM_COLORS` array (end of file, before `mod tests`):

```rust
// ── Drum pad categories (single source with DRUM_COLORS) ────────────────────
/// Pad color for kick sounds = grid Kick.
pub const DRUM_CAT_KICK: Color32 = DRUM_COLORS[0];
/// Pad color for snare sounds = grid Snare.
pub const DRUM_CAT_SNARE: Color32 = DRUM_COLORS[2];
/// Pad color for hi-hat sounds = grid Closed HH.
pub const DRUM_CAT_HATS: Color32 = DRUM_COLORS[6];
/// Pad color for cymbal sounds = grid Crash.
pub const DRUM_CAT_CYMBALS: Color32 = DRUM_COLORS[13];
/// Pad color for tom sounds = grid Low Tom.
pub const DRUM_CAT_TOMS: Color32 = DRUM_COLORS[9];
/// Pad color for perc/clap/rim sounds = grid Clap.
pub const DRUM_CAT_PERC: Color32 = DRUM_COLORS[3];
```

- [x] **Step 3: Use them in keyboard.rs**

In `src/gui/keyboard.rs:136-158`, replace the `pad_color` closure body's hardcoded colors:

```rust
            let pad_color = |note: u8| -> egui::Color32 {
                let i = (note - 36) as usize;
                let target = self.drum_remap[i] as usize;
                let kit_idx = target.wrapping_sub(30);
                let name = if kit_idx < self.drum_kit_names.len() {
                    self.drum_kit_names[kit_idx].to_lowercase()
                } else {
                    String::new()
                };
                if name.contains("hat") || name.contains("hh") {
                    theme::DRUM_CAT_HATS
                } else if name.contains("crash") || name.contains("cymbal") || name.contains("ride") || name.contains("china") {
                    theme::DRUM_CAT_CYMBALS
                } else if name.contains("tom") {
                    theme::DRUM_CAT_TOMS
                } else if name.contains("kick") {
                    theme::DRUM_CAT_KICK
                } else if name.contains("snare") {
                    theme::DRUM_CAT_SNARE
                } else {
                    theme::DRUM_CAT_PERC
                }
            };
```

- [x] **Step 4: Run tests**

Run: `cargo test --lib gui::theme`
Expected: 3 tests PASS (2 from Task 1 + this one).

- [x] **Step 5: Visual check**

Run: `cargo run` — on-screen pads now show snare=orange, hats=cyan (same as the step grid), not green/yellow.

- [x] **Step 6: Commit**

```bash
git add src/gui/theme.rs src/gui/keyboard.rs
git commit -m "fix: pad category colors derive from DRUM_COLORS (audit п.10)"
```

---

### Task 4: Non-color state cues for mute/loop/solo (WCAG 1.4.1)

**Files:**
- Modify: `src/gui/midi_seq.rs:159-171` (M button), `:35-45` (Loop button)
- Modify: `src/gui/drums.rs:1073-1091` (looper S/M buttons)
- Modify: `src/gui/layers.rs:306-313` (muted part label)

- [x] **Step 1: MIDI-seq M button — bold when muted**

In `src/gui/midi_seq.rs:159-167`, replace:

```rust
                        let m_col = if track.muted {
                            egui::Color32::YELLOW
                        } else {
                            theme::MIDI_SEQ_MUTE_INACTIVE
                        };
                        if ui
                            .button(egui::RichText::new("M").color(m_col).monospace())
                            .on_hover_text(if track.muted { "Unmute" } else { "Mute" })
                            .clicked()
```

with:

```rust
                        let mut m_text = egui::RichText::new("M").monospace();
                        if track.muted {
                            m_text = m_text.color(egui::Color32::YELLOW).strong();
                        } else {
                            m_text = m_text.color(theme::MIDI_SEQ_MUTE_INACTIVE);
                        }
                        if ui
                            .button(m_text)
                            .on_hover_text(if track.muted { "Unmute" } else { "Mute" })
                            .clicked()
```

- [x] **Step 2: MIDI-seq Loop button — bold when on**

In `src/gui/midi_seq.rs:40`, replace:

```rust
            if ui.button(egui::RichText::new("↻ Loop").color(loop_col)).clicked() {
```

with:

```rust
            let mut loop_text = egui::RichText::new("↻ Loop").color(loop_col);
            if self.midi_seq_looping { loop_text = loop_text.strong(); }
            if ui.button(loop_text).clicked() {
```

- [x] **Step 3: Looper S/M buttons — bold when active**

In `src/gui/drums.rs`, solo button (around line 1074):

```rust
                let solo_color = if soloed { egui::Color32::YELLOW } else { egui::Color32::GRAY };
                let mut s_text = egui::RichText::new("S").color(solo_color);
                if soloed { s_text = s_text.strong(); }
                if ui.button(s_text).on_hover_text("Solo layer").clicked() {
```

mute button (around line 1086):

```rust
                let mute_color = if muted { egui::Color32::RED } else { egui::Color32::GRAY };
                let mut m_text = egui::RichText::new("M").color(mute_color);
                if muted { m_text = m_text.strong(); }
                if ui.button(m_text).on_hover_text("Mute layer").clicked() {
```

- [x] **Step 4: Muted part label — strikethrough + lighter gray**

In `src/gui/layers.rs:306-313`, replace:

```rust
            let label_text = if muted {
                egui::RichText::new(label).strong().color(egui::Color32::GRAY)
            } else {
                egui::RichText::new(label).strong()
            };
```

with:

```rust
            let label_text = if muted {
                // Зачёркивание — второй признак состояния помимо цвета (WCAG 1.4.1);
                // gray(150) вместо GRAY — 5.6:1 вместо 4.2:1.
                egui::RichText::new(label).strong().strikethrough()
                    .color(egui::Color32::from_gray(150))
            } else {
                egui::RichText::new(label).strong()
            };
```

- [x] **Step 5: Build + visual check**

Run: `cargo clippy --all-targets -- -D warnings` — clean.
Run: `cargo run` — muted MIDI track / looper layer / part visibly differ from unmuted even in grayscale (bold / strikethrough).

- [x] **Step 6: Commit**

```bash
git add src/gui/midi_seq.rs src/gui/drums.rs src/gui/layers.rs
git commit -m "fix: non-color state cues for mute/loop/solo (WCAG 1.4.1, audit п.6)"
```

---

### Task 5: XY-pad axis labels inside the clip-rect; pitch labels unclipped

**Files:**
- Modify: `src/gui/macros.rs:132-136,181-196`
- Modify: `src/gui/drums.rs:720-730`

- [x] **Step 1: Reserve label strips in the XY-pad allocation**

In `src/gui/macros.rs`, replace lines 132–136:

```rust
        // Pad area
        let size = egui::vec2(200.0, 200.0);
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        response.clone().on_hover_text("Drag to control two macros");
        let painter = ui.painter_at(rect);
```

with:

```rust
        // Pad area + reserved strips for axis labels: раньше подписи рисовались
        // за пределами widget-rect и клип-ректа (audit High п.4) и просто
        // не были видны.
        let size = egui::vec2(200.0, 200.0);
        let label_w = 16.0; // левая полоса под подпись Y
        let label_h = 16.0; // нижняя полоса под подпись X
        let (outer, response) = ui.allocate_exact_size(
            egui::vec2(size.x + label_w, size.y + label_h),
            egui::Sense::click_and_drag(),
        );
        response.clone().on_hover_text("Drag to control two macros");
        let rect = egui::Rect::from_min_max(
            egui::pos2(outer.left() + label_w, outer.top()),
            egui::pos2(outer.right(), outer.bottom() - label_h),
        );
        let painter = ui.painter_at(outer);
```

Everything else in the function (grid, dot, drag math) already uses `rect` and stays unchanged, КРОМЕ интеракции: полосы-стрипы теперь внутри click_and_drag-аллокации, и клик по стрипу без гейта заснапил бы макрос в крайнее значение. В блоке «Handle drag / click» (macros.rs:199-202) добавь гейт по `rect`:

```rust
        if response.dragged() || response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if rect.contains(pos) {
                    // ... существующее тело обновления макросов, +1 уровень вложенности
                }
            }
        }
```

- [x] **Step 2: Move the label anchors into the strips and paint via `ui.painter()`**

Keep the reserved strips from Step 1 (they guarantee the labels don't overlap neighbouring widgets), but paint the labels with `ui.painter()` — full ui clip-rect, immune to clipping — and anchor them centered in the strips (the old anchors `rect.bottom() + 10.0`/`rect.left() - 4.0` still land half-outside `outer`: CENTER_TOP grows downward off the bottom edge, RIGHT_CENTER extends leftward past the left edge). Replace lines 181–196:

```rust
        // Axis labels — ui.painter() (full clip-rect), anchors centered in the
        // reserved strips of `outer`. painter_at(outer) резал бы их: CENTER_TOP
        // растёт вниз за нижний край, RIGHT_CENTER — влево за левый.
        ui.painter().text(
            egui::pos2(rect.center().x, outer.bottom() - label_h * 0.5),
            egui::Align2::CENTER_CENTER,
            x_name,
            egui::FontId::proportional(11.0),
            theme::TEXT_OVERLAY,
        );
        // Y label (rotated text not trivial in egui, place to the left).
        // Длинные имена могут немного заходить на левый край пэда — это
        // приемлемо; главное, что подпись видна.
        ui.painter().text(
            egui::pos2(outer.left() + label_w * 0.5, rect.center().y),
            egui::Align2::CENTER_CENTER,
            y_name,
            egui::FontId::proportional(11.0),
            theme::TEXT_OVERLAY,
        );
```

- [x] **Step 3: Pitch labels — clamp + contrast fallback**

In `src/gui/drums.rs:720-730`, replace:

```rust
                    // Pitch label
                    if pitch != 0.0 {
                        let label = format!("{:+}", step.pitch);
                        painter.text(
                            egui::pos2(x + step_w * 0.5, if pitch > 0.0 { bar_top - 8.0 } else { bar_bottom + 8.0 }),
                            egui::Align2::CENTER_CENTER,
                            &label,
                            egui::FontId::proportional(9.0),
                            theme::TEXT_OVERLAY,
                        );
                    }
```

with:

```rust
                    // Pitch label. Clamped inside the allocation: при pitch=±24
                    // бар доходит до края и подпись над ним клипается
                    // (audit Medium п.11). Если подпись легла на бар — чёрный
                    // цвет (7:1 на синем вместо 1.43:1 у gray(180)).
                    if pitch != 0.0 {
                        let label = format!("{:+}", step.pitch);
                        let raw_y = if pitch > 0.0 { bar_top - 8.0 } else { bar_bottom + 8.0 };
                        let y = raw_y.clamp(rect.top() + 6.0, rect.top() + bar_h - 6.0);
                        let label_pos = egui::pos2(x + step_w * 0.5, y);
                        let color = if bar_rect.contains(label_pos) {
                            egui::Color32::BLACK
                        } else {
                            theme::TEXT_OVERLAY
                        };
                        painter.text(
                            label_pos,
                            egui::Align2::CENTER_CENTER,
                            &label,
                            egui::FontId::proportional(9.0),
                            color,
                        );
                    }
```

- [x] **Step 4: Visual check**

Run: `cargo run` — XY-pad: X label under the pad, Y label left of the pad, both visible. Pitch seq: set a step to pitch +24 — label visible (black on the bar top), not cut off.

- [x] **Step 5: Commit**

```bash
git add src/gui/macros.rs src/gui/drums.rs
git commit -m "fix: XY-pad axis labels and pitch labels no longer clipped (audit High 4, Medium 11)"
```

---

### Task 6: English labels in the parts panel

**Files:**
- Modify: `src/gui/layers.rs:37-43,124-127,139,148,158`
- Modify: `src/gui/drums.rs:872`

- [x] **Step 1: Replace the RU strings**

In `src/gui/layers.rs`:

Line 37-39:
```rust
                ui.small_button("Split: ON ✕")
                    .on_hover_text("Turn split off: remove right hand (zone B)")
```

Line 41-43:
```rust
                ui.button("+ Right hand (Split)")
                    .on_hover_text("Add right hand: zone B, keyboard splits at the Split point")
```

Lines 123-127:
```rust
            let a_header = if split_on {
                format!("◀ Left hand (A) — 0..{}", sp.saturating_sub(1))
            } else {
                "◀ Left hand (A)".to_string()
            };
```

Line 139:
```rust
                && ui.small_button("+ Add part").on_hover_text("Add part to Zone A").clicked()
```

Line 148:
```rust
                    ui.label(egui::RichText::new(format!("▶ Right hand (B) — {sp}..127")).weak().small());
```

Line 158:
```rust
                && ui.small_button("+ Add part").on_hover_text("Add part to Zone B").clicked()
```

In `src/gui/drums.rs:872`:
```rust
                        format!("Cycle {} ({} steps per loop)", rec_cycle, loop_steps)
```

- [x] **Step 2: Verify no RU strings remain in gui**

Run: `grep -rnP '[А-Яа-яЁё]' src/gui/ --include='*.rs'`. Russian *comments* are legitimate in this repo and will match — including trailing comments after code, and the naive `grep -v '//'` filter misses indented comments, so don't filter: go through every hit and confirm the Cyrillic is inside `//`/`//!` comment text, not inside a string literal. If a string literal matches, translate it the same way as Step 1. The complete list of RU string literals before this task is exactly the 10 lines fixed in Step 1 (drums.rs:872, layers.rs:37,38,41,42,124,126,139,148,158), so after the edits every hit must be a comment.

- [x] **Step 3: Commit**

```bash
git add src/gui/layers.rs src/gui/drums.rs
git commit -m "fix: english labels in parts panel and looper progress (audit High 3)"
```

---

### Task 7: Hotkey X goes through the 2-second confirm

**Files:**
- Modify: `src/gui/mod.rs:1551-1553` (LooperClear branch of `process_keybinds`)
- Modify: `src/gui/mod.rs` top (add `mod confirm;` — done in Task 10; this task uses the existing `looper_clear_confirm` field directly with the same 2 s window)

This task deliberately lands BEFORE Task 10 and hardcodes the 2 s window; Task 10 replaces the literal with `confirm::CONFIRM_WINDOW`. (If executing tasks strictly in order, prefer doing Task 10 first and using the constant here — both orders compile.)

- [x] **Step 1: Gate the hotkey**

In `src/gui/mod.rs`, replace:

```rust
                KeyAction::LooperClear => {
                    let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
                }
```

with:

```rust
                KeyAction::LooperClear => {
                    // Тот же 2s-confirm, что у кнопки Clear (audit High п.2):
                    // первое нажатие взводит, второе в течение окна — очищает.
                    let armed = matches!(self.looper_clear_confirm, Some(t)
                        if t.elapsed().as_secs_f32() < 2.0);
                    if armed {
                        let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
                        self.looper_clear_confirm = None;
                    } else {
                        self.looper_clear_confirm = Some(Instant::now());
                    }
                }
```

`Instant` is already imported at `mod.rs:20`. Side effect (intended): after pressing X, the GUI Clear button shows red "Sure?" — shared state gives visible feedback.

- [x] **Step 2: Manual check**

Run: `cargo run` — record a loop, press X once → Clear button flips to "Sure?", looper intact; press X again within 2 s → looper cleared. Press X once, wait >2 s, press X → arms again, looper intact.

- [x] **Step 3: Commit**

```bash
git add src/gui/mod.rs
git commit -m "fix: hotkey X requires the same 2s confirm as Clear button (audit High 2)"
```

---

### Task 8: Drum slider hover shows the value

**Files:**
- Modify: `src/gui/drums.rs:276-292`

- [x] **Step 1: Include current values in the hover text**

The four `format!("…{dsp_hint}")` hover strings in the L/T/D/P slider block become:

```rust
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut level, 0.0..=1.0).show_value(false).text("L")).on_hover_text(format!("Level: {level:.2}{dsp_hint}")).changed() {
```

```rust
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut tune, -24.0..=24.0).show_value(false).text("T")).on_hover_text(format!("Tune: {tune:+.1} st{dsp_hint}")).changed() {
```

```rust
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut decay, 0.1..=4.0).show_value(false).text("D")).on_hover_text(format!("Decay: {decay:.2} s{dsp_hint}")).changed() {
```

```rust
                    if ui.add_enabled(dsp_kit, egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false).text("P")).on_hover_text(format!("Pan: {pan:+.2}{dsp_hint}")).changed() {
```

(The `format!` runs every frame, so the tooltip tracks the value while dragging.)

- [x] **Step 2: Visual check**

Run: `cargo run` — hover over L: shows "Level: 0.80"; drag — the number follows.

- [x] **Step 3: Commit**

```bash
git add src/gui/drums.rs
git commit -m "fix: show value in hover for L/T/D/P drum sliders (audit п.7)"
```

---

### Task 9: ScrollArea for the pitch sequencer branch

**Files:**
- Modify: `src/gui/mod.rs:1169-1172`

- [x] **Step 1: Wrap the WHOLE else-branch content in one ScrollArea**

⚠ Нельзя оборачивать только `draw_pitch_sequencer`: `ScrollArea::auto_shrink(false)`
занимает всю оставшуюся высоту CentralPanel, и параметры ниже получили бы ~ноль
высоты («невидимый секвенсор» превратился бы в «невидимые параметры»). Как и в
соседних ветках (fx/midi/drums), ScrollArea оборачивает содержимое ветки целиком;
вложенный ScrollArea параметров (mod.rs:1212) при этом удаляется — вертикальный
ScrollArea внутри вертикального вырожден.

In `src/gui/mod.rs:1169-1216`, replace the entire else-branch:

```rust
            } else {
                // Лупер переехал в единую docked-панель (#30); здесь остаётся
                // питч-секвенсор и параметры активной части
                self.draw_pitch_sequencer(ui);
                ui.separator();
                ui.add_space(theme::SP_MD);
                let part = self.active_part;
                let is_sf2 = self.parts[part].sf2_mode;
                if is_sf2 && self.sf2_keys_soundfont.is_some() {
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let prog = self.parts[part].sf2_program;
                    let name = GM_PROGRAM_NAMES.get(prog as usize).unwrap_or(&"?");
                    ui.colored_label(
                        theme::TEXT_SF2_STATUS,
                        format!("SF2: {} | {}: {} | Effects chain applies", self.sf2_keys_loaded_name, prog, name),
                    );
                } else {
                    // Header: patch name + Init/Rnd/Undo/Redo
                    ui.horizontal(|ui| {
                        let idx = self.parts[part].patch_idx;
                        let name = self.patches.get(idx).map(|p| p.name.as_str()).unwrap_or("---");
                        ui.strong(name);
                        ui.separator();
                        if ui.small_button("Init").on_hover_text("Reset to default patch").clicked() {
                            self.push_undo_snapshot();
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].edited_params = crate::synth::PatchParams::default().to_map();
                            self.send_edited_params(part);
                        }
                        if ui.small_button("Rnd").on_hover_text("Randomize patch").clicked() {
                            self.push_undo_snapshot();
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].edited_params = crate::synth::PatchParams::random_map();
                            self.send_edited_params(part);
                        }
                        ui.separator();
                        if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("\u{21B6}").small()).on_hover_text("Undo (Ctrl+Z)").clicked() {
                            self.undo();
                        }
                        if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("\u{21B7}").small()).on_hover_text("Redo (Ctrl+Shift+Z)").clicked() {
                            self.redo();
                        }
                    });
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        self.draw_params_editable(ui);
                    });
                }
            }
```

with:

```rust
            } else {
                // Лупер переехал в единую docked-панель (#30); здесь остаётся
                // питч-секвенсор и параметры активной части. ScrollArea как в
                // остальных ветках, на ВСЮ ветку: без него секвенсор молча
                // обрезается docked-панелью (audit High п.1), а оборачивать
                // только секвенсор нельзя — auto_shrink(false) съел бы высоту
                // параметров ниже. Вложенный ScrollArea параметров удалён.
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    self.draw_pitch_sequencer(ui);
                    ui.separator();
                    ui.add_space(theme::SP_MD);
                    let part = self.active_part;
                    let is_sf2 = self.parts[part].sf2_mode;
                    if is_sf2 && self.sf2_keys_soundfont.is_some() {
                        use crate::synth::sampler::GM_PROGRAM_NAMES;
                        let prog = self.parts[part].sf2_program;
                        let name = GM_PROGRAM_NAMES.get(prog as usize).unwrap_or(&"?");
                        ui.colored_label(
                            theme::TEXT_SF2_STATUS,
                            format!("SF2: {} | {}: {} | Effects chain applies", self.sf2_keys_loaded_name, prog, name),
                        );
                    } else {
                        // Header: patch name + Init/Rnd/Undo/Redo
                        ui.horizontal(|ui| {
                            let idx = self.parts[part].patch_idx;
                            let name = self.patches.get(idx).map(|p| p.name.as_str()).unwrap_or("---");
                            ui.strong(name);
                            ui.separator();
                            if ui.small_button("Init").on_hover_text("Reset to default patch").clicked() {
                                self.push_undo_snapshot();
                                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                                self.parts[part].edited_params = crate::synth::PatchParams::default().to_map();
                                self.send_edited_params(part);
                            }
                            if ui.small_button("Rnd").on_hover_text("Randomize patch").clicked() {
                                self.push_undo_snapshot();
                                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                                self.parts[part].edited_params = crate::synth::PatchParams::random_map();
                                self.send_edited_params(part);
                            }
                            ui.separator();
                            if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("\u{21B6}").small()).on_hover_text("Undo (Ctrl+Z)").clicked() {
                                self.undo();
                            }
                            if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("\u{21B7}").small()).on_hover_text("Redo (Ctrl+Shift+Z)").clicked() {
                                self.redo();
                            }
                        });
                        self.draw_params_editable(ui);
                    }
                });
            }
```

(Trade-off, accepted: header Init/Rnd/Undo/Redo скроллится вместе с контентом — как в остальных ветках.)

- [x] **Step 2: Visual check**

Run: `cargo run` at 1920×1036 — the "Pitch Step Sequencer" header is visible between the zone bar and the params; the sequencer scrolls instead of vanishing.

- [x] **Step 3: Commit**

```bash
git add src/gui/mod.rs
git commit -m "fix: pitch sequencer reachable — wrap in ScrollArea like sibling branches (audit High 1)"
```

---

### Task 10: Unified confirm helper; apply to all destructive buttons

**Files:**
- Create: `src/gui/confirm.rs`
- Modify: `src/gui/mod.rs:15` (module decl), `:1619-1625` (unbind), `:1630-1634` (Reset Defaults), struct `:386` area (fields)
- Modify: `src/lib.rs:829-838` (field init)
- Modify: `src/gui/drums.rs:72-74` (drum Clear), `:808-825` (looper Clear refactor)
- Modify: `src/gui/fx_chain.rs:14-18` (FX Reset)

- [x] **Step 1: Create the helper**

`src/gui/confirm.rs` (new file — `rustfmt.toml` applies to new files, no action needed):

```rust
//! Two-stage destructive-action confirmation ("Sure?" timer, 2 s).
//!
//! Первый клик взводит: кнопка превращается в красный «Sure?» на 2 секунды.
//! Второй клик внутри окна — действие выполняется; истечение окна снимает
//! взвод молча. Один механизм для кнопок и хоткеев (audit Medium п.9,
//! Strategic п.3; модалки остаются только для удаления партии).

use std::time::{Duration, Instant};

use eframe::egui;

/// How long the armed "Sure?" state stays valid.
pub const CONFIRM_WINDOW: Duration = Duration::from_secs(2);

/// Draw a destructive button with two-stage confirmation.
/// Returns `true` only on the confirming (second) click.
pub fn confirm_button(
    ui: &mut egui::Ui,
    label: &str,
    armed: &mut Option<Instant>,
    enabled: bool,
    tooltip: &str,
) -> bool {
    // Expire stale arming.
    if armed.map(|t| t.elapsed() >= CONFIRM_WINDOW).unwrap_or(false) {
        *armed = None;
    }
    let pending = armed.is_some();
    let text = if pending {
        egui::RichText::new("Sure?").color(egui::Color32::RED).strong()
    } else {
        egui::RichText::new(label)
    };
    let clicked = ui
        .add_enabled(enabled, egui::Button::new(text))
        .on_hover_text(tooltip)
        .clicked();
    if clicked {
        if pending {
            *armed = None;
            return true;
        }
        *armed = Some(Instant::now());
    }
    false
}
```

- [x] **Step 2: Register the module and add App fields**

In `src/gui/mod.rs`, after line 15 (`pub mod keybinds;`):

```rust
mod confirm;
```

In the `App` struct: after `pub looper_clear_confirm: Option<std::time::Instant>,` (line 370):

```rust
    pub drum_clear_confirm: Option<std::time::Instant>,
    pub fx_reset_confirm: Option<std::time::Instant>,
```

after `pub keybind_capturing: Option<keybinds::KeyAction>,` (line 386):

```rust
    pub keybinds_reset_confirm: Option<std::time::Instant>,
    pub unbind_confirm: Option<(keybinds::KeyAction, std::time::Instant)>,
```

In `src/lib.rs`, after `looper_clear_confirm: None,` (line 829):

```rust
        drum_clear_confirm: None,
        fx_reset_confirm: None,
```

after `keybind_capturing: None,` (line 838):

```rust
        keybinds_reset_confirm: None,
        unbind_confirm: None,
```

- [x] **Step 3: Apply to drum Clear (drums.rs:72-74)**

```rust
            if super::confirm::confirm_button(ui, "Clear", &mut self.drum_clear_confirm, true, "Clear all steps") {
                let _ = self.ctrl_tx.push(ControlEvent::DrumSeqClear);
            }
```

- [x] **Step 4: Refactor looper Clear to the helper (drums.rs:808-825)**

Replace the whole block (from `let clear_pending = …` through the expiry `if` at line 825) with:

```rust
            if super::confirm::confirm_button(
                ui,
                "Clear",
                &mut self.looper_clear_confirm,
                event_count > 0 || state != LooperState::Idle,
                "Clear the whole looper (destructive)",
            ) {
                let _ = self.ctrl_tx.push(ControlEvent::LooperClear);
            }
```

- [x] **Step 5: Apply to FX Reset (fx_chain.rs:14-18)**

```rust
            if super::confirm::confirm_button(ui, "Reset", &mut self.fx_reset_confirm, true, "Reset FX chain to defaults") {
                let chain = FxChain { active: true, ..FxChain::default() };
                flush_fx_chain(&chain, part, &mut self.parts[part].edited_params);
                self.send_edited_params(part);
            }
```

- [x] **Step 6: Reset Defaults via helper (mod.rs:1630-1634)**

```rust
                if confirm::confirm_button(ui, "Reset Defaults", &mut self.keybinds_reset_confirm, true, "Restore default keybindings (custom binds are lost)") {
                    self.keybinds = keybinds::Keybinds::defaults();
                    self.config.ui.keybinds = self.keybinds.to_config();
                    self.keybind_capturing = None;
                }
```

- [x] **Step 7: Unbind gets per-action arming (mod.rs:1619-1625)**

Replace:

```rust
                        if ui.small_button("x").clicked() {
                            // Unbind
                            if let Some(key) = self.keybinds.key_for_action(&action) {
                                self.keybinds.binds_mut().retain(|(k, _)| *k != key);
                                self.config.ui.keybinds = self.keybinds.to_config();
                            }
                        }
```

with:

```rust
                        // Unbind — тот же 2s-confirm, взвод на конкретное действие.
                        // Expire через is_some_and: нельзя мутировать поле под &borrow.
                        if self.unbind_confirm.as_ref().is_some_and(|(_, t)| t.elapsed() >= confirm::CONFIRM_WINDOW) {
                            self.unbind_confirm = None;
                        }
                        let armed = matches!(&self.unbind_confirm, Some((a, _)) if a == &action);
                        let ub_color = if armed { egui::Color32::RED } else { ui.visuals().text_color() };
                        if ui.small_button(egui::RichText::new(if armed { "Sure?" } else { "x" }).color(ub_color)).clicked() {
                            if armed {
                                if let Some(key) = self.keybinds.key_for_action(&action) {
                                    self.keybinds.binds_mut().retain(|(k, _)| *k != key);
                                    self.config.ui.keybinds = self.keybinds.to_config();
                                }
                                self.unbind_confirm = None;
                            } else {
                                self.unbind_confirm = Some((action.clone(), Instant::now()));
                            }
                        }
```

(The unbind path does not use `confirm_button` because arming is keyed by `KeyAction`, one row among many.)

Не покрывается здесь: pad-perf right-click clear (`settings.rs:93`) — см. Out of scope. Косметика: если лупер стал Idle после взвода, красный «Sure?» на задизейбленной кнопке висит до истечения окна (≤2 c, самоликвидируется) — принято.

- [x] **Step 8: Point Task 7's literal at the constant**

In `src/gui/mod.rs` (Task 7's `LooperClear` branch), replace `t.elapsed().as_secs_f32() < 2.0` with `t.elapsed() < confirm::CONFIRM_WINDOW`.

- [x] **Step 9: Build and run full checks**

Run: `cargo clippy --all-targets -- -D warnings` — clean.
Run: `cargo run` — manual matrix:
  - drum Clear: click → "Sure?" 2 s → clears on second click; wait → disarms.
  - looper Clear: same behavior as before (regression).
  - FX Reset: armed flow.
  - keybind unbind "x": per-row "Sure?".
  - Reset Defaults: armed flow.
  - double-click fast: arms then confirms exactly once, third click re-arms.

- [x] **Step 10: Commit**

```bash
git add src/gui/confirm.rs src/gui/mod.rs src/lib.rs src/gui/drums.rs src/gui/fx_chain.rs
git commit -m "fix: one 2s-confirm mechanism for all destructive buttons (audit Medium 9, High 2)"
```

---

### Task 11: Modals → non-modal windows (instrument browser, remap)

**Files:**
- Modify: `src/gui/mod.rs:1233,1381-1383` (instrument browser)
- Modify: `src/gui/drums.rs:444-444,515-517` (pad remap dialog), `:534,566-580` (single-pad remap)

Read the current bodies first — only the container changes; the body content stays identical.

- [x] **Step 1: Instrument browser Modal → Window**

In `src/gui/mod.rs` (`draw_instrument_browser_modal`), replace:

```rust
        let modal = egui::Modal::new(egui::Id::new("inst_browser")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.strong(if is_sf2 {
                format!("GM Instrument — Part {layer_label}")
            } else {
                format!("Patch — Part {layer_label}")
            });
            ui.add_space(theme::SP_XS);
```

with:

```rust
        // Немодальное окно: выбор инструмента обратим (клик по другому патчу
        // перекроет), полноэкранная модалка избыточна (audit Medium п.8).
        let mut win_open = true;
        let title = if is_sf2 {
            format!("GM Instrument — Part {layer_label}")
        } else {
            format!("Patch — Part {layer_label}")
        };
        let win = egui::Window::new(title)
            .open(&mut win_open)
            .resizable(true)
            .default_width(400.0)
            .default_height(540.0)
            .show(ctx, |ui| {
            ui.add_space(theme::SP_XS);
```

The body (search field, category tree, ScrollArea) stays unchanged. At the end, replace:

```rust
        if modal.should_close() {
            close = true;
        }
```

with:

```rust
        if !win_open || win.is_none() {
            close = true;
        }
```

- [x] **Step 2: Pad remap dialog Modal → Window**

In `src/gui/drums.rs` (`draw_drum_remap_modals`), replace:

```rust
            let dialog = egui::Modal::new(egui::Id::new("drum_remap_dialog")).show(ctx, |ui| {
                ui.set_width(900.0);
```

with:

```rust
            let mut win_open = true;
            let win = egui::Window::new("Pad Remap")
                .open(&mut win_open)
                .resizable(true)
                .default_width(920.0)
                .show(ctx, |ui| {
```

and replace the tail:

```rust
            if dialog.should_close() {
                self.show_drum_remap = false;
            }
```

with:

```rust
            if !win_open || win.is_none() {
                self.show_drum_remap = false;
            }
```

(The inner `Close` button already sets `self.show_drum_remap = false` — keep it. Drop the `ui.set_width(900.0);` line; the body's fixed-width grid defines its own size.)

- [x] **Step 3: Single-pad remap Modal → Window**

Same pattern for the `remap_pad` dialog. In `src/gui/drums.rs:534`, replace:

```rust
            let dialog = egui::Modal::new(egui::Id::new("remap_pad_dialog")).show(ctx, |ui| {
                ui.set_width(300.0);
                ui.strong(format!("Pad {}", p + 1));
```

with:

```rust
            let mut win_open = true;
            let win = egui::Window::new(format!("Pad {}", p + 1))
                .open(&mut win_open)
                .resizable(false)
                .default_width(320.0)
                .show(ctx, |ui| {
                ui.strong(format!("Pad {}", p + 1));
```

and replace the tail (`drums.rs:572-574`):

```rust
            if dialog.should_close() {
                self.remap_pad = None;
            }
```

with:

```rust
            if !win_open || win.is_none() {
                self.remap_pad = None;
            }
```

(The inner `Close` button at drums.rs:559-561 already sets `self.remap_pad = None` — keep it. The post-closure `sel != self.drum_remap[idx]` block stays unchanged.)

- [x] **Step 4: Build + visual check**

Run: `cargo clippy --all-targets -- -D warnings` — clean (watch for unused `dialog` variables and leftover `ui.set_width` borrows).
Run: `cargo run` — open instrument browser from a part row: floating window, main UI still visible and interactive behind it; close via ✕ works. Same for Pad Remap and clicking a drawn pad.

- [x] **Step 5: Commit**

```bash
git add src/gui/mod.rs src/gui/drums.rs
git commit -m "fix: instrument browser and remap dialogs are non-modal windows (audit Medium 8)"
```

---

### Task 12: Final verification

- [x] **Step 1: Full check suite (from CLAUDE.md)**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```

Expected: all green. (`rt_safety` must pass — this plan touches only `src/gui/`, but run it as proof.)

- [x] **Step 2: End-to-end manual pass**

Run: `cargo run`, then walk the audit checklist:
1. Pitch sequencer header visible; sequencer scrolls (High 1).
2. X hotkey + Clear both require 2 s confirm (High 2).
3. No Russian strings in the parts panel (High 3).
4. XY-pad axis labels visible (High 4).
5. Hover any L/T/D/P slider → value shown (п.7).
6. Key-zone map: all zones visible, active outlined (п.5).
7. Muted things distinguishable in grayscale: bold M/S/Loop, strikethrough part label (п.6).
8. Pads and grid use the same per-category colors (п.10).
9. Browser/remap are floating windows (п.8).
10. Drum Clear / FX Reset / unbind / Reset Defaults all ask "Sure?" (п.9).

- [x] **Step 3: No commit** (nothing to commit if all green; fix-forward if not)

---

## Out of scope (audit declines + deferred)

- Pad-perf right-click clear (`settings.rs:93-95`, audit п.9) — осознанно без confirm: right-click уже двухшаговый жест, рядом лежит popup с явным пунктом «--- (clear)»; arm-on-right-click потребовал бы двойной right-click — нестандартный и опасный в live-режиме паттерн.
- Looper timeline muted-dot alpha (п.5, timeline dots) — needs the Strategic muted-gray pass with the design decision on dim-vs-hatch; left for the theme-unification follow-up.
- Pad-perf bottom-row contrast (п.5) and FX ↑/↓ hit targets — separate task, not in this audit's action lists.
- «Restart to apply» duplication, toast/inline warning unification, Margin scale cleanup — Low, declined or deferred in the audit.
- Remap of Step Sequencer into the dock (TECHDEBT #30 dependency) — the ScrollArea here is the stopgap the audit prescribes.
