//! Centralized color palette and layout constants for the GUI.

use eframe::egui::Color32;

// ── Spacing scale ───────────────────────────────────────────────────────────

/// Tight: between closely related items.
pub const SP_XS: f32 = 2.0;
/// Small: default gap.
pub const SP_SM: f32 = 4.0;
/// Medium: between sections.
pub const SP_MD: f32 = 8.0;
/// Large: between major areas.
pub const SP_LG: f32 = 12.0;

// ── Slider widths ───────────────────────────────────────────────────────────

/// Main params: Cutoff, Volume, Attack, Release, envelope controls.
pub const SLIDER_PRIMARY: f32 = 250.0;
/// Secondary: Detune, Pan, Velocity, LFO Depth, FM Index.
pub const SLIDER_SECONDARY: f32 = 180.0;
/// Compact: Fine tune, Portamento, Pitch Bend Range, misc utility.
pub const SLIDER_COMPACT: f32 = 120.0;
/// Drum per-instrument sliders.
pub const SLIDER_DRUM: f32 = 40.0;

// ── Backgrounds ──────────────────────────────────────────────────────────────

/// Main dark background for scope, filter response, ADSR curve.
pub const BG_WIDGET: Color32 = Color32::from_rgb(20, 20, 30);

/// ADSR / envelope curve background (slightly lighter).
pub const BG_WIDGET_ALT: Color32 = Color32::from_rgb(25, 25, 35);

/// Panel / section background (XY pad, pitch-seq, filter response).
pub const BG_PANEL: Color32 = Color32::from_gray(30);

/// Subtle frame background for parameter section grouping.
pub const BG_SECTION: Color32 = Color32::from_gray(22);

/// Key-zone map / timeline background.
pub const BG_TIMELINE: Color32 = Color32::from_gray(25);

/// Black piano key idle color.
pub const BG_BLACK_KEY: Color32 = Color32::from_rgb(30, 30, 30);

/// Drum-grid beat-1 background.
pub const BG_BEAT_PRIMARY: Color32 = Color32::from_gray(50);

/// Drum-grid even-step background.
pub const BG_BEAT_SECONDARY: Color32 = Color32::from_gray(40);

/// Drum-grid odd-step background.
pub const BG_BEAT_OFF: Color32 = Color32::from_gray(32);

/// Pad-perf empty pad background.
pub const BG_PAD_EMPTY: Color32 = Color32::from_rgb(50, 50, 58);

/// Pad-perf pressed-but-empty background.
pub const BG_PAD_PRESSED_EMPTY: Color32 = Color32::from_rgb(90, 90, 100);

// ── Text ─────────────────────────────────────────────────────────────────────

/// Secondary info text (restart notice, SF2 path hints).
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 140, 140);

/// Settings JACK label / mild emphasis.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(180, 180, 180);

/// SF2-mode active label.
pub const TEXT_SF2_STATUS: Color32 = Color32::from_rgb(160, 160, 160);

/// Pitch step label, XY pad axis label.
pub const TEXT_OVERLAY: Color32 = Color32::from_gray(180);

/// Something is wrong but the app still runs — dropped events, stream errors.
pub const TEXT_WARN: Color32 = Color32::from_rgb(220, 90, 80);

// ── Strokes / decorative lines ───────────────────────────────────────────────

/// Subtle border on cells, pads (was gray(55/60), bumped for contrast).
pub const STROKE_CELL_BORDER: Color32 = Color32::from_gray(90);

/// XY pad / panel border.
pub const STROKE_PANEL_BORDER: Color32 = Color32::from_gray(80);

/// Grid subdivision lines (XY pad, pitch-seq separators) — bumped for WCAG.
pub const STROKE_GRID: Color32 = Color32::from_gray(80);

/// Filter response grid lines.
pub const STROKE_GRID_DARK: Color32 = Color32::from_gray(40);

/// 0dB reference line / pitch-seq center line, pitch-seq border.
pub const STROKE_GUIDE: Color32 = Color32::from_gray(90);

/// Scope / ADSR sustain level line, subtle internal strokes.
pub const STROKE_SUBTLE: Color32 = Color32::from_rgb(60, 60, 80);

/// Timeline bar-line color.
pub const STROKE_BAR_LINE: Color32 = Color32::from_gray(70);

/// Piano key border.
pub const STROKE_KEY_BORDER: Color32 = Color32::from_rgb(120, 120, 120);

/// Pad border color.
pub const STROKE_PAD_BORDER: Color32 = Color32::from_rgb(60, 60, 60);

// ── Keyboard / Pads ──────────────────────────────────────────────────────────

/// White piano key idle.
pub const KEY_WHITE_IDLE: Color32 = Color32::from_rgb(220, 220, 220);

/// White piano key active / pressed.
pub const KEY_WHITE_ACTIVE: Color32 = Color32::from_rgb(80, 180, 255);

/// Black piano key active / pressed.
pub const KEY_BLACK_ACTIVE: Color32 = Color32::from_rgb(60, 140, 220);

/// Cyan pad idle color (top row).
#[allow(dead_code)] // palette reference
pub const PAD_CYAN: Color32 = Color32::from_rgb(0, 200, 210);

/// Pink pad idle color (bottom row).
#[allow(dead_code)] // palette reference
pub const PAD_PINK: Color32 = Color32::from_rgb(220, 60, 150);

/// Pad-perf top-row accent.
pub const PAD_PERF_TOP: Color32 = Color32::from_rgb(0, 160, 180);

/// Pad-perf bottom-row accent.
pub const PAD_PERF_BOT: Color32 = Color32::from_rgb(180, 50, 120);

// ── Waveform / scope ─────────────────────────────────────────────────────────

/// Scope waveform color.
pub const SCOPE_WAVE: Color32 = Color32::from_rgb(0, 200, 100);

// ── Filter / envelope curves ─────────────────────────────────────────────────

/// Filter response / XY dot / parameter accent (cyan-ish).
pub const CURVE_FILTER: Color32 = Color32::from_rgb(100, 200, 255);

/// Cutoff frequency marker.
pub const CURVE_CUTOFF_MARKER: Color32 = Color32::from_rgb(255, 200, 50);

/// Amp envelope curve color.
pub const CURVE_AMP_ENV: Color32 = Color32::from_rgb(100, 180, 255);

/// Filter / mod envelope curve color.
pub const CURVE_MOD_ENV: Color32 = Color32::from_rgb(255, 160, 80);

/// XY pad crosshair (semi-transparent).
pub const XY_CROSSHAIR: Color32 = Color32::from_rgba_premultiplied(100, 200, 255, 60);

// ── Pitch / step sequencer ───────────────────────────────────────────────────

/// Pitch bar: gate off / muted.
pub const SEQ_BAR_MUTED: Color32 = Color32::from_gray(90);

/// Pitch bar: current step (active).
pub const SEQ_BAR_CURRENT: Color32 = Color32::from_rgb(0, 220, 180);

/// Pitch bar: idle step.
pub const SEQ_BAR_IDLE: Color32 = Color32::from_rgb(0, 160, 220);

/// Current step highlight stroke.
pub const SEQ_STEP_HIGHLIGHT: Color32 = Color32::from_rgb(0, 255, 200);

/// Gate-on indicator color.
pub const SEQ_GATE_ON: Color32 = Color32::from_rgb(0, 200, 120);

/// Gate-off indicator color (non-text fill, ≥3:1 on BG_PANEL — WCAG 1.4.11;
/// was gray(50) = 1.30:1).
pub const SEQ_GATE_OFF: Color32 = Color32::from_gray(110);

// ── MIDI sequencer ───────────────────────────────────────────────────────────

/// Loop button active.
pub const MIDI_SEQ_LOOP_ON: Color32 = Color32::from_rgb(80, 210, 100);

/// Mute button inactive (text — ≥4.5:1 on BG_PANEL, WCAG 1.4.3; was gray(80) = 2.1:1).
pub const MIDI_SEQ_MUTE_INACTIVE: Color32 = Color32::from_gray(140);

/// Loop button inactive (text — ≥4.5:1; was egui DARK_GRAY = gray(96) ≈ 2.6:1).
pub const MIDI_SEQ_LOOP_OFF: Color32 = Color32::from_gray(140);

// ── Looper ───────────────────────────────────────────────────────────────────

/// Overdub state indicator.
pub const LOOPER_OVERDUB: Color32 = Color32::from_rgb(255, 140, 0);

/// Muted layer label (text — ≥4.5:1; was gray(80) = 2.1:1).
pub const LOOPER_LAYER_MUTED: Color32 = Color32::from_gray(140);

// ── Part colors (8 zones) ────────────────────────────────────────────────────

pub const PART_COLORS: [Color32; 8] = [
    Color32::from_rgb(70, 130, 200),  // A1 blue
    Color32::from_rgb(70, 190, 100),  // A2 green
    Color32::from_rgb(220, 150, 50),  // A3 orange
    Color32::from_rgb(200, 70, 70),   // A4 red
    Color32::from_rgb(160, 80, 200),  // B1 purple
    Color32::from_rgb(50, 190, 190),  // B2 teal
    Color32::from_rgb(220, 200, 50),  // B3 yellow
    Color32::from_rgb(190, 100, 150), // B4 rose
];

// ── Drum colors (16 instruments) ─────────────────────────────────────────────

pub const DRUM_COLORS: [Color32; 16] = [
    Color32::from_rgb(220, 60, 60),   // Kick
    Color32::from_rgb(180, 140, 80),  // Side Stick
    Color32::from_rgb(220, 140, 40),  // Snare
    Color32::from_rgb(200, 100, 200), // Clap
    Color32::from_rgb(220, 160, 40),  // E-Snare
    Color32::from_rgb(120, 200, 80),  // Lo Floor Tom
    Color32::from_rgb(60, 180, 220),  // Closed HH
    Color32::from_rgb(100, 180, 60),  // Hi Floor Tom
    Color32::from_rgb(80, 160, 200),  // Pedal HH
    Color32::from_rgb(80, 160, 60),   // Low Tom
    Color32::from_rgb(60, 200, 240),  // Open HH
    Color32::from_rgb(60, 140, 60),   // Lo-Mid Tom
    Color32::from_rgb(60, 120, 60),   // Hi-Mid Tom
    Color32::from_rgb(200, 200, 60),  // Crash
    Color32::from_rgb(60, 100, 60),   // High Tom
    Color32::from_rgb(180, 180, 60),  // Ride
];

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

    /// Audit Medium п.5: muted/inactive state tokens must meet WCAG — 3:1
    /// non-text (1.4.11) для заливки, 4.5:1 текста (1.4.3) для букв-лейблов.
    #[test]
    fn muted_state_tokens_meet_non_text_contrast() {
        let non_text: [(&str, Color32, Color32, f64); 4] = [
            ("SEQ_GATE_OFF", SEQ_GATE_OFF, BG_PANEL, 3.0),
            ("LOOPER_LAYER_MUTED", LOOPER_LAYER_MUTED, BG_PANEL, 4.5),
            ("MIDI_SEQ_MUTE_INACTIVE", MIDI_SEQ_MUTE_INACTIVE, BG_PANEL, 4.5),
            ("MIDI_SEQ_LOOP_OFF", MIDI_SEQ_LOOP_OFF, BG_PANEL, 4.5),
        ];
        for (name, fg, bg, min) in non_text {
            let c = contrast(fg, bg);
            assert!(c >= min, "{name}: {c:.2}:1 < {min}:1 (WCAG)");
        }
    }

    /// Audit Medium п.5 → фидбек: цвета PART_COLORS теперь имена партов в
    /// списке настроек — должны быть читаемы на фоне панели (BG_PANEL).
    #[test]
    fn key_zone_colors_meet_non_text_contrast() {
        for (i, c) in PART_COLORS.iter().enumerate() {
            let cr = contrast(*c, BG_PANEL);
            assert!(cr >= 3.0, "PART_COLORS[{i}]: {cr:.2}:1 < 3:1 on BG_PANEL");
        }
    }

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
}
