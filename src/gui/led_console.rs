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
    /// Bottom-row: Undo (note 75).
    Undo,
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
    Undo,
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
}

impl Default for LedConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl LedConsole {
    pub fn new() -> Self {
        Self { mode: Mode::Home, pending: Vec::new() }
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
            PadEvent::Undo => Some(Action::Undo),
            PadEvent::Prev => Some(self.step(-1)),
            PadEvent::Next => Some(self.step(1)),
            PadEvent::PageBack => Some(Action::PageBack),
            PadEvent::PageForward => Some(Action::PageForward),
        }
    }

    fn tap(&mut self, slot: usize) -> Option<Action> {
        if self.mode == Mode::Menu {
            let m = match slot {
                0 => Mode::Home,
                1 => Mode::Pattern,
                2 => Mode::Mute,
                _ => return None, // dark slot in menu: no-op
            };
            self.mode = m;
            return None; // mode switch only; GUI repaints via snapshot
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

    /// LED colors for slots 0..=7 (top row). `w_perf`/`w_pat` — first index of
    /// the visible window: slot i (1..=7) shows item window + i - 1.
    /// Slot 0 is the MENU button (white).
    pub fn top_colors(
        &self,
        active_perf: Option<usize>,
        filled_perfs: usize,
        current_pattern: usize,
        mutes: &[bool; 8],
        w_perf: usize,
        w_pat: usize,
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
                    if mutes[i - 1] {
                        *col = [0x00, 0x00, 0x00]; // muted: OFF
                    } else {
                        *col = [0x00, 0xFF, 0x00]; // sounding: green
                    }
                }
            }
            Mode::Menu => {
                // menu items are color-coded to what the row becomes after
                // picking them: HOME -> blue perfs, PATTERN -> yellow, MUTE -> green
                for (i, col) in c.iter_mut().enumerate() {
                    *col = match i {
                        0 => AVAIL,              // HOME: blue (perfs)
                        1 => [0xFF, 0xFF, 0x00], // PATTERN: yellow
                        2 => [0x00, 0xFF, 0x00], // MUTE: green
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
    /// когда есть снимок, иначе OFF. Prev/Next — фиксированный оранжевый.
    pub fn bottom_colors(
        &self,
        playing: bool,
        recording: bool,
        page_depth: usize,
        undo_available: bool,
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
        c[7] = if undo_available { [0xFF, 0x50, 0x80] } else { [0x00, 0x00, 0x00] };
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
        let home = c.top_colors(Some(2), 5, 0, &mutes, 2, 0);
        assert_eq!(home[1], [0xFF, 0x00, 0xFF]); // selected (idx 2)
        assert_eq!(home[2], [0x00, 0x66, 0xFF]); // available (idx 3)
        // slot 4 = idx 5 -> за концом списка (count 5) -> OFF
        assert_eq!(home[4], [0x00, 0x00, 0x00]);
        // pattern window 1: slot 1 = pattern 1 (selected при current=1)
        c.mode = Mode::Pattern;
        let pat = c.top_colors(None, 0, 1, &mutes, 0, 1);
        assert_eq!(pat[1], [0xFF, 0x00, 0xFF]);
        assert_eq!(pat[2], [0xFF, 0xFF, 0x00]); // pattern mode speaks yellow
    }

    #[test]
    fn bottom_page_shades_and_undo_pink() {
        let mut c = LedConsole::new();
        let b = c.bottom_colors(false, false, 0, false);
        assert_eq!(b[3], [0x00, 0x66, 0xFF]); // page 0: base blue
        assert_eq!(b[4], [0x00, 0x66, 0xFF]);
        assert_eq!(b[7], [0x00, 0x00, 0x00]); // undo off (no snapshot)
        let b = c.bottom_colors(false, false, 2, true);
        assert_eq!(b[3], [0x00, 0x32, 0x77], "page 2: darker blue");
        assert_eq!(b[7], [0xFF, 0x50, 0x80]); // undo pink when available
        // nav 30/31 — фиксированный оранжевый, НЕ зависит от режима
        c.mode = Mode::Pattern;
        let b = c.bottom_colors(false, false, 0, true);
        assert_eq!(b[5], [0xFF, 0x80, 0x00]);
        assert_eq!(b[6], [0xFF, 0x80, 0x00]);
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
    fn undo_action_passes_through() {
        let mut c = LedConsole::new();
        assert_eq!(c.on_event(PadEvent::Undo, Instant::now()), Some(Action::Undo));
    }
}
