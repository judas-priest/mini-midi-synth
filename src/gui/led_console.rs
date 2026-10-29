//! Bank-2 pad console state machine (pure logic, GUI thread only).
//! Slots 0..=7 = top-row pads 17..=24 (notes 60..=67). Slot 0 (pad 17) is
//! the always-visible MENU button; targets live on slots 1..=7.
//! Prev/Next arrive from notes 78/79, PAGE back/forward from 80/76 (routed by caller).
//! Engine-side transport notes 81/82/83 (mod.rs:1573-1598) never reach this
//! module; console Undo is note 75 (engine's legacy 64-arm superseded).

use std::time::{Duration, Instant};

const HOLD: Duration = Duration::from_millis(500);

/// Top-row mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Mode {
    #[default]
    Home,
    Pattern,
    Mute,
    Menu,
}

/// Console input events (note -> slot by the caller).
#[derive(Clone, Copy, Debug)]
pub enum PadEvent {
    /// NoteOn on a top-row pad (slot 0..=7).
    Down(usize),
    /// NoteOff on a top-row pad (slot 0..=7).
    Up(usize),
    /// Bottom-row pad 32 (note 75) pressed.
    UndoDown,
    /// Bottom-row pad 32 (note 75) released: tap (< HOLD) = Undo, hold = Redo.
    UndoUp,
    /// Bottom-row nav: Prev (note 78).
    Prev,
    /// Bottom-row nav: Next (note 79).
    Next,
    /// Bottom-row: PAGE back (note 80, pad 28).
    PageBack,
    /// Bottom-row: PAGE forward (note 76, pad 29).
    PageForward,
}

/// Console output: what GUI must do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    LoadPerf(usize),
    SelectPattern(usize),
    ToggleMute(usize),
    /// Menu slot 3: flip seq_target (drums <-> looper). Stays in Menu.
    ToggleTransportTarget,
    Undo,
    /// Pad 32 held >= HOLD: redo the last undone sequencer edit.
    Redo,
    StepPerf(i32),
    StepPattern(i32),
    PageBack,
    PageForward,
}

pub struct LedConsole {
    mode: Mode,
    /// Currently pressed pads (multi-touch: several simultaneous taps each
    /// fire their own action on release). Deduplicated by slot.
    pending: Vec<(usize, Instant)>,
    /// Pad 32 press instant (tap/hold disambiguation on release).
    undo_down: Option<Instant>,
}

impl Default for LedConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl LedConsole {
    pub fn new() -> Self {
        Self { mode: Mode::Home, pending: Vec::new(), undo_down: None }
    }

    /// Poll: fire hold detection. Returns Some(Mode::Menu) on hold trigger
    /// (any pad held >= HOLD opens the menu and clears all pending taps).
    pub fn tick(&mut self, now: Instant) -> Option<Mode> {
        if self.pending.iter().any(|(_, t0)| now.duration_since(*t0) >= HOLD) {
            self.pending.clear();
            self.mode = Mode::Menu;
            return Some(Mode::Menu);
        }
        None
    }

    /// Feed a pad event; returns the action to execute (if any).
    pub fn on_event(&mut self, ev: PadEvent, now: Instant) -> Option<Action> {
        match ev {
            PadEvent::Down(slot) => {
                // refresh if already held (double NoteOn), else register
                match self.pending.iter_mut().find(|(s, _)| *s == slot) {
                    Some((_, t0)) => *t0 = now,
                    None => self.pending.push((slot, now)),
                }
                None
            }
            PadEvent::Up(slot) => {
                let pos = self.pending.iter().position(|(s, _)| *s == slot)?;
                let (_, t0) = self.pending.remove(pos);
                if now.duration_since(t0) < HOLD {
                    self.tap(slot)
                } else {
                    None // hold consumed by tick()
                }
            }
            PadEvent::UndoDown => { self.undo_down = Some(now); None }
            PadEvent::UndoUp => {
                let t0 = self.undo_down.take()?;
                if now.duration_since(t0) >= HOLD { Some(Action::Redo) } else { Some(Action::Undo) }
            }
            PadEvent::Prev => Some(self.step(-1)),
            PadEvent::Next => Some(self.step(1)),
            PadEvent::PageBack => Some(Action::PageBack),
            PadEvent::PageForward => Some(Action::PageForward),
        }
    }

    fn tap(&mut self, slot: usize) -> Option<Action> {
        if self.mode == Mode::Menu {
            return match slot {
                0 => { self.mode = Mode::Home; None }
                1 => { self.mode = Mode::Pattern; None }
                2 => { self.mode = Mode::Mute; None }
                3 => Some(Action::ToggleTransportTarget), // toggle, stays in Menu
                _ => None, // dark slot in menu: no-op
            };
        }
        if slot == 0 {
            // always-visible MENU button (top-left): tap = open menu
            self.mode = Mode::Menu;
            return None;
        }
        // slots 1..=7 are targets, target index = slot - 1
        Some(match self.mode {
            Mode::Home => Action::LoadPerf(slot),
            Mode::Pattern => Action::SelectPattern(slot - 1),
            Mode::Mute => Action::ToggleMute(slot - 1),
            Mode::Menu => unreachable!(),
        })
    }

    fn step(&mut self, dir: i32) -> Action {
        match self.mode {
            Mode::Home => Action::StepPerf(dir),
            Mode::Pattern => Action::StepPattern(dir),
            _ => Action::StepPerf(dir),
        }
    }

    /// LED colors for slots 0..=7 (top row). `w_perf`/`w_pat`/`w_mute` — first
    /// index of the visible window: slot i (1..=7) shows item window + i - 1.
    /// MUTE slots cover all parts via w_mute paging (PAGE pads 28/29).
    /// Slot 0 is the MENU button (white).
    #[allow(clippy::too_many_arguments)] // flat LED snapshot, callers build it inline
    pub fn top_colors(
        &self,
        active_perf: Option<usize>,
        filled_perfs: usize,
        current_pattern: usize,
        mutes: &[bool],
        w_perf: usize,
        w_pat: usize,
        w_mute: usize,
        seq_target: u8,
    ) -> [[u8; 3]; 8] {
        // Palette uses ONLY hues visually validated on this hardware (see
        // PADS_NOTES.md bank-1 kit colors: FF0000/00FF00/FFFF00/0066FF/FF8000/
        // FF00FF/FFFFFF render correctly; untested hues render unpredictably).
        // TOP row = white / magenta / blue / orange; BOTTOM row = green / red /
        // yellow / orange. Off = disabled everywhere.
        const SEL: [u8; 3] = [0xFF, 0x00, 0xFF]; // selected: magenta
        const AVAIL: [u8; 3] = [0x00, 0x66, 0xFF]; // available: blue
        let mut c = [[0u8; 3]; 8];
        if self.mode != Mode::Menu {
            c[0] = [0xFF, 0xFF, 0xFF]; // MENU button (white)
        }
        match self.mode {
            Mode::Home => {
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    let idx = w_perf + i - 1;
                    if Some(idx) == active_perf {
                        *col = SEL;
                    } else if idx < filled_perfs {
                        *col = AVAIL;
                    } else {
                        *col = [0x00, 0x00, 0x00]; // empty: OFF
                    }
                }
            }
            Mode::Pattern => {
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    let idx = w_pat + i - 1;
                    if idx == current_pattern {
                        *col = SEL;
                    } else if idx < 8 {
                        *col = [0xFF, 0xFF, 0x00]; // available: yellow
                    } else {
                        *col = [0x00, 0x00, 0x00];
                    }
                }
            }
            Mode::Mute => {
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    let idx = w_mute + i - 1;
                    match mutes.get(idx) {
                        Some(false) => *col = [0x00, 0xFF, 0x00], // sounding: green
                        _ => *col = [0x00, 0x00, 0x00], // muted / beyond list: OFF
                    }
                }
            }
            Mode::Menu => {
                // menu items are color-coded to what the row becomes after
                // picking them: HOME -> blue perfs, PATTERN -> yellow,
                // MUTE -> green; slot 3 IS the toggle: shows the current
                // target (blue = drums, green = looper)
                for (i, col) in c.iter_mut().enumerate() {
                    *col = match i {
                        0 => AVAIL,              // HOME: blue (perfs)
                        1 => [0xFF, 0xFF, 0x00], // PATTERN: yellow
                        2 => [0x00, 0xFF, 0x00], // MUTE: green
                        3 => if seq_target == 0 { AVAIL } else { [0x00, 0xFF, 0x00] },
                        _ => [0x00, 0x00, 0x00],
                    };
                }
            }
        }
        c
    }

    /// Bottom-row colors. `page_depth` — сколько PAGE-нажатий вглубь (0 = база);
    /// оттенки синего темнеют с глубиной (EXPERIMENTAL — юзер проверяет рендер;
    /// fallback: заменить все shades на [0x00, 0x66, 0xFF]).
    /// Undo (slot 15 = pad 32): розовый FF5080 (EXPERIMENTAL; fallback FF00FF)
    /// когда есть снимок, белый FFFFFF когда armed redo (приоритет), иначе OFF.
    /// Prev/Next — фиксированный оранжевый.
    pub fn bottom_colors(
        &self,
        playing: bool,
        recording: bool,
        page_depth: usize,
        undo_available: bool,
        redo_available: bool,
    ) -> [[u8; 3]; 8] {
        const PAGE_SHADES: [[u8; 3]; 4] = [
            [0x00, 0x66, 0xFF], // depth 0: validated blue
            [0x00, 0x48, 0xB5], // depth 1
            [0x00, 0x32, 0x77], // depth 2
            [0x00, 0x21, 0x4D], // depth 3+
        ];
        let shade = PAGE_SHADES[page_depth.min(PAGE_SHADES.len() - 1)];
        let mut c = [[0x00, 0x00, 0x00]; 8];
        c[0] = if playing { [0x00, 0xFF, 0x00] } else { [0x00, 0x00, 0x00] };
        c[1] = if !playing && !recording { [0xFF, 0xFF, 0x00] } else { [0x00, 0x00, 0x00] };
        c[2] = if recording { [0xFF, 0x00, 0x00] } else { [0x00, 0x00, 0x00] };
        c[3] = shade; // PAGE back (pad 28)
        c[4] = shade; // PAGE forward (pad 29)
        c[5] = [0xFF, 0x80, 0x00]; // Prev: fixed orange
        c[6] = [0xFF, 0x80, 0x00]; // Next: fixed orange
        c[7] = if redo_available {
            [0xFF, 0xFF, 0xFF] // redo armed: white (validated hue)
        } else if undo_available {
            [0xFF, 0x50, 0x80] // undo available: pink (EXPERIMENTAL, fallback FF00FF)
        } else {
            [0x00, 0x00, 0x00]
        };
        c
    }

    /// Current top-row mode (for window paging in the GUI).
    pub fn mode(&self) -> Mode {
        self.mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tap_in_home_loads_perf() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        assert_eq!(c.on_event(PadEvent::Down(3), now), None);
        assert_eq!(c.on_event(PadEvent::Up(3), now + Duration::from_millis(50)),
                   Some(Action::LoadPerf(3)));
    }

    #[test]
    fn hold_opens_menu_and_eats_tap() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(5), now);
        assert_eq!(c.tick(now + Duration::from_millis(510)), Some(Mode::Menu));
        assert_eq!(c.on_event(PadEvent::Up(5), now + Duration::from_millis(600)), None);
    }

    #[test]
    fn menu_tap_selects_mode() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)), None);
        assert_eq!(c.mode, Mode::Pattern);
    }

    #[test]
    fn stray_noteoff_is_noop() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        assert_eq!(c.on_event(PadEvent::Up(2), now), None);
        assert_eq!(c.mode, Mode::Home);
    }

    #[test]
    fn simultaneous_taps_both_fire() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        c.on_event(PadEvent::Down(4), now + Duration::from_millis(10));
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)),
                   Some(Action::LoadPerf(1)));
        assert_eq!(c.on_event(PadEvent::Up(4), now + Duration::from_millis(40)),
                   Some(Action::LoadPerf(4)));
    }

    #[test]
    fn duplicate_down_refreshes_single_pending() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        c.on_event(PadEvent::Down(1), now + Duration::from_millis(10));
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)),
                   Some(Action::LoadPerf(1)));
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(40)), None);
    }

    #[test]
    fn pattern_mode_tap_selects_pattern() {
        let mut c = LedConsole::new();
        c.mode = Mode::Pattern;
        let now = Instant::now();
        c.on_event(PadEvent::Down(6), now);
        assert_eq!(c.on_event(PadEvent::Up(6), now + Duration::from_millis(30)),
                   Some(Action::SelectPattern(5)));
    }

    #[test]
    fn mute_mode_tap_toggles_part() {
        let mut c = LedConsole::new();
        c.mode = Mode::Mute;
        let now = Instant::now();
        c.on_event(PadEvent::Down(2), now);
        assert_eq!(c.on_event(PadEvent::Up(2), now + Duration::from_millis(30)),
                   Some(Action::ToggleMute(1)));
    }

    #[test]
    fn menu_button_tap_opens_menu() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(0), now);
        assert_eq!(c.on_event(PadEvent::Up(0), now + Duration::from_millis(30)), None);
        assert_eq!(c.mode, Mode::Menu);
        // and a second tap on slot 0 inside the menu returns HOME
        c.on_event(PadEvent::Down(0), now);
        assert_eq!(c.on_event(PadEvent::Up(0), now + Duration::from_millis(30)), None);
        assert_eq!(c.mode, Mode::Home);
    }

    #[test]
    fn prev_next_are_mode_dependent() {
        let mut c = LedConsole::new();
        assert_eq!(c.on_event(PadEvent::Prev, Instant::now()), Some(Action::StepPerf(-1)));
        c.mode = Mode::Pattern;
        assert_eq!(c.on_event(PadEvent::Next, Instant::now()), Some(Action::StepPattern(1)));
    }

    #[test]
    fn window_offsets_map_targets_to_slots() {
        let mut c = LedConsole::new();
        let mutes = [false; 8];
        // window 2: slot 1 = perf idx 2 (selected), slot 2 = idx 3 (filled)
        let home = c.top_colors(Some(2), 5, 0, &mutes, 2, 0, 0, 0);
        assert_eq!(home[1], [0xFF, 0x00, 0xFF]); // selected (idx 2)
        assert_eq!(home[2], [0x00, 0x66, 0xFF]); // available (idx 3)
        // slot 4 = idx 5 -> за концом списка (count 5) -> OFF
        assert_eq!(home[4], [0x00, 0x00, 0x00]);
        // pattern window 1: slot 1 = pattern 1 (selected при current=1)
        c.mode = Mode::Pattern;
        let pat = c.top_colors(None, 0, 1, &mutes, 0, 1, 0, 0);
        assert_eq!(pat[1], [0xFF, 0x00, 0xFF]);
        assert_eq!(pat[2], [0xFF, 0xFF, 0x00]); // pattern mode speaks yellow
    }

    #[test]
    fn hold_in_menu_stays_menu() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let now = Instant::now();
        c.on_event(PadEvent::Down(6), now);
        assert_eq!(c.tick(now + Duration::from_millis(510)), Some(Mode::Menu));
        assert_eq!(c.on_event(PadEvent::Up(6), now + Duration::from_millis(600)), None);
        assert_eq!(c.mode, Mode::Menu);
    }

    #[test]
    fn menu_slot3_toggles_target_and_stays_menu() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let now = Instant::now();
        c.on_event(PadEvent::Down(3), now);
        assert_eq!(c.on_event(PadEvent::Up(3), now + Duration::from_millis(30)),
                   Some(Action::ToggleTransportTarget));
        assert_eq!(c.mode, Mode::Menu, "toggle item must not leave the menu");
    }

    #[test]
    fn menu_slot3_led_shows_target() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let mutes = [false; 8];
        let drums = c.top_colors(None, 0, 0, &mutes, 0, 0, 0, 0);
        assert_eq!(drums[3], [0x00, 0x66, 0xFF], "target=0 (drums): blue");
        let looper = c.top_colors(None, 0, 0, &mutes, 0, 0, 0, 1);
        assert_eq!(looper[3], [0x00, 0xFF, 0x00], "target=1 (looper): green");
        // остальные пункты меню не изменились
        assert_eq!(drums[0], [0x00, 0x66, 0xFF]); // HOME: blue
        assert_eq!(drums[1], [0xFF, 0xFF, 0x00]); // PATTERN: yellow
        assert_eq!(drums[2], [0x00, 0xFF, 0x00]); // MUTE: green
    }

    #[test]
    fn mute_mode_windows_over_all_parts() {
        let mut c = LedConsole::new();
        c.mode = Mode::Mute;
        let mutes: [bool; 16] = std::array::from_fn(|i| i == 7 || i == 14);
        // window 7: slot 1 = part 7 (muted -> OFF), slot 2 = part 8 (sound -> green)
        let top = c.top_colors(None, 0, 0, &mutes, 0, 0, 7, 0);
        assert_eq!(top[1], [0x00, 0x00, 0x00], "muted part 7: OFF");
        assert_eq!(top[2], [0x00, 0xFF, 0x00], "sounding part 8: green");
        // window 14 (last page): slot 1 = part 14 (muted -> OFF),
        // slot 2 = part 15 (< 16 -> green)
        let top = c.top_colors(None, 0, 0, &mutes, 0, 0, 14, 0);
        assert_eq!(top[1], [0x00, 0x00, 0x00], "muted part 14: OFF");
        assert_eq!(top[2], [0x00, 0xFF, 0x00], "part 15: green");
        // за концом списка -> OFF (окно 14, slot 3 = part 16 — не существует)
        assert_eq!(top[3], [0x00, 0x00, 0x00]);
    }

    #[test]
    fn pad32_tap_is_undo() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        assert_eq!(c.on_event(PadEvent::UndoDown, now), None);
        assert_eq!(c.on_event(PadEvent::UndoUp, now + Duration::from_millis(50)),
                   Some(Action::Undo));
    }

    #[test]
    fn pad32_hold_is_redo() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::UndoDown, now);
        // hold does NOT open the menu (bottom pads are not top-row pending)
        assert_eq!(c.tick(now + Duration::from_millis(600)), None);
        assert_eq!(c.on_event(PadEvent::UndoUp, now + Duration::from_millis(600)),
                   Some(Action::Redo));
    }

    #[test]
    fn stray_undo_up_is_noop() {
        let mut c = LedConsole::new();
        assert_eq!(c.on_event(PadEvent::UndoUp, Instant::now()), None);
    }

    #[test]
    fn pad32_led_redo_white_precedence() {
        let c = LedConsole::new();
        assert_eq!(c.bottom_colors(false, false, 0, false, false)[7], [0x00, 0x00, 0x00]);
        assert_eq!(c.bottom_colors(false, false, 0, true, false)[7], [0xFF, 0x50, 0x80],
                   "undo: pink");
        assert_eq!(c.bottom_colors(false, false, 0, true, true)[7], [0xFF, 0xFF, 0xFF],
                   "redo armed wins: white");
        assert_eq!(c.bottom_colors(false, false, 0, false, true)[7], [0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn bottom_page_shades_and_undo_pink() {
        let mut c = LedConsole::new();
        let b = c.bottom_colors(false, false, 0, false, false);
        assert_eq!(b[3], [0x00, 0x66, 0xFF]); // page 0: base blue
        assert_eq!(b[4], [0x00, 0x66, 0xFF]);
        assert_eq!(b[7], [0x00, 0x00, 0x00]); // undo off (no snapshot)
        let b = c.bottom_colors(false, false, 2, true, false);
        assert_eq!(b[3], [0x00, 0x32, 0x77], "page 2: darker blue");
        assert_eq!(b[7], [0xFF, 0x50, 0x80]); // undo pink when available
        // nav 30/31 — фиксированный оранжевый, НЕ зависит от режима
        c.mode = Mode::Pattern;
        let b = c.bottom_colors(false, false, 0, true, false);
        assert_eq!(b[5], [0xFF, 0x80, 0x00]);
        assert_eq!(b[6], [0xFF, 0x80, 0x00]);
    }
}
