//! Centralized color palette and layout constants for the GUI.

use eframe::egui::Color32;

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

/// Dim / inactive text.
pub const TEXT_DIM: Color32 = Color32::from_rgb(100, 100, 100);

/// Secondary info text (restart notice, SF2 path hints).
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(140, 140, 140);

/// Settings JACK label / mild emphasis.
pub const TEXT_MUTED: Color32 = Color32::from_rgb(180, 180, 180);

/// SF2-mode active label.
pub const TEXT_SF2_STATUS: Color32 = Color32::from_rgb(160, 160, 160);

/// Pitch step label, XY pad axis label.
pub const TEXT_OVERLAY: Color32 = Color32::from_gray(180);

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

/// Key-zone octave separators, pitch-seq separator.
pub const STROKE_ZONE_DIV: Color32 = Color32::from_gray(90);

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
pub const PAD_CYAN: Color32 = Color32::from_rgb(0, 200, 210);

/// Pink pad idle color (bottom row).
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

/// Gate-off indicator color.
pub const SEQ_GATE_OFF: Color32 = Color32::from_gray(50);

// ── MIDI sequencer ───────────────────────────────────────────────────────────

/// Loop button active.
pub const MIDI_SEQ_LOOP_ON: Color32 = Color32::from_rgb(80, 210, 100);

/// Status: error text.
pub const MIDI_SEQ_ERROR: Color32 = Color32::from_rgb(220, 70, 70);

/// Status: success text.
pub const MIDI_SEQ_OK: Color32 = Color32::from_rgb(100, 200, 100);

/// Mute button inactive.
pub const MIDI_SEQ_MUTE_INACTIVE: Color32 = Color32::from_gray(80);

// ── Looper ───────────────────────────────────────────────────────────────────

/// Overdub state indicator.
pub const LOOPER_OVERDUB: Color32 = Color32::from_rgb(255, 140, 0);

/// Muted layer label.
pub const LOOPER_LAYER_MUTED: Color32 = Color32::from_gray(80);

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
