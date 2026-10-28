//! Bank-2 pad console state machine (pure logic, GUI thread only).
//! Slots 0..=7 = top-row pads 17..=24 (notes 60..=67). Slot 0 (pad 17) is
//! the always-visible MENU button; targets live on slots 1..=7.
//! Prev/Next/Rewind arrive as actions from notes 78/79/80 (routed by caller).
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
}

pub struct LedConsole {
    mode: Mode,
    pending: Option<(usize, Instant)>,
}

impl Default for LedConsole {
    fn default() -> Self {
        Self::new()
    }
}

impl LedConsole {
    pub fn new() -> Self {
        Self { mode: Mode::Home, pending: None }
    }

    /// Poll: fire hold detection. Returns Some(Mode::Menu) on hold trigger.
    pub fn tick(&mut self, now: Instant) -> Option<Mode> {
        if let Some((_, t0)) = self.pending {
            if now.duration_since(t0) >= HOLD {
                self.pending = None;
                self.mode = Mode::Menu;
                return Some(Mode::Menu);
            }
        }
        None
    }

    /// Feed a pad event; returns the action to execute (if any).
    pub fn on_event(&mut self, ev: PadEvent, now: Instant) -> Option<Action> {
        match ev {
            PadEvent::Down(slot) => {
                self.pending = Some((slot, now));
                None
            }
            PadEvent::Up(slot) => {
                let (down_slot, t0) = self.pending?;
                if down_slot != slot {
                    return None; // not our press; keep pending
                }
                let elapsed = now.duration_since(t0);
                self.pending = None;
                if elapsed < HOLD {
                    self.tap(slot)
                } else {
                    None // hold consumed by tick()
                }
            }
            PadEvent::Undo => Some(Action::Undo),
            PadEvent::Prev => Some(self.step(-1)),
            PadEvent::Next => Some(self.step(1)),
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

    /// LED colors for slots 0..=7 (top row) given current state.
    pub fn top_colors(
        &self,
        active_perf: Option<usize>,
        filled_perfs: usize,
        current_pattern: usize,
        mutes: &[bool; 8],
    ) -> [[u8; 3]; 8] {
        // Palette (Novation/Push canon): bright orange = selected, dim white =
        // available, bright green = running/sounding, dim red = muted/off,
        // bright white = system button, off = empty.
        let mut c = [[0u8; 3]; 8];
        if self.mode != Mode::Menu {
            // slot 0 is the always-visible MENU button (bright white)
            c[0] = [0xC8, 0xC8, 0xC8];
        }
        match self.mode {
            Mode::Home => {
                // slot i (1..=7) <-> perf_list index i ("(Init)" at 0 has no slot)
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    if Some(i) == active_perf {
                        *col = [0xFF, 0x80, 0x00]; // selected: bright orange
                    } else if i < filled_perfs {
                        *col = [0x50, 0x50, 0x50]; // available: dim white
                    } else {
                        *col = [0x00, 0x00, 0x00]; // empty: off
                    }
                }
            }
            Mode::Pattern => {
                // slot i (1..=7) <-> pattern i-1 (pattern 8 via Prev/Next)
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    if i - 1 == current_pattern {
                        *col = [0xFF, 0x80, 0x00]; // selected: bright orange
                    } else {
                        *col = [0x50, 0x50, 0x50]; // available: dim white
                    }
                }
            }
            Mode::Mute => {
                // slot i (1..=7) <-> part i-1 (part 8 via GUI)
                for (i, col) in c.iter_mut().enumerate().skip(1) {
                    if mutes[i - 1] {
                        *col = [0x50, 0x00, 0x00]; // muted: dim red (disabled)
                    } else {
                        *col = [0x00, 0xC0, 0x00]; // sounding: bright green
                    }
                }
            }
            Mode::Menu => {
                for (i, col) in c.iter_mut().enumerate() {
                    *col = if i < 3 { [0x50, 0x50, 0x50] } else { [0x00, 0x00, 0x00] };
                }
            }
        }
        c
    }

    /// Bottom-row (transport) colors: slots 8..=15 = pads 25..=32.
    pub fn bottom_colors(&self, playing: bool, recording: bool) -> [[u8; 3]; 8] {
        let sys = [0x40, 0x40, 0x40]; // system buttons: dim white
        let mut c = [sys; 8];
        c[0] = if playing { [0x00, 0xFF, 0x00] } else { [0x00, 0x40, 0x00] }; // green lit/dim
        c[1] = if !playing && !recording { [0xFF, 0xDD, 0x00] } else { [0x40, 0x38, 0x00] }; // amber lit/dim
        c[2] = if recording { [0xFF, 0x00, 0x00] } else { [0x40, 0x00, 0x00] }; // red lit/dim
        c[3] = [0x50, 0x40, 0x00]; // Rew: dim amber
        // slots 4..=7 (unused, Prev, Next, Undo) stay dim white (system)
        c
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
    fn duplicate_down_last_wins() {
        let mut c = LedConsole::new();
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        c.on_event(PadEvent::Down(4), now + Duration::from_millis(10));
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)), None);
        assert_eq!(c.on_event(PadEvent::Up(4), now + Duration::from_millis(40)),
                   Some(Action::LoadPerf(4)));
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
    fn snapshot_colors_home_and_menu() {
        let mut c = LedConsole::new();
        let mut mutes = [false; 8];
        mutes[2] = true;
        let home = c.top_colors(Some(1), 3, 0, &mutes);
        assert_eq!(home[0], [0xC8, 0xC8, 0xC8]); // MENU button (bright white)
        assert_eq!(home[1], [0xFF, 0x80, 0x00]); // selected (bright orange)
        assert_eq!(home[2], [0x50, 0x50, 0x50]); // available (dim white)
        assert_eq!(home[7], [0x00, 0x00, 0x00]); // empty -> LED off
        c.mode = Mode::Menu;
        let menu = c.top_colors(None, 0, 0, &mutes);
        assert_eq!(menu[0], [0x50, 0x50, 0x50]);
        assert_eq!(menu[4], [0x00, 0x00, 0x00]);
    }

    #[test]
    fn bottom_transport_colors() {
        let c = LedConsole::new();
        let b = c.bottom_colors(true, false);
        assert_eq!(b[0], [0x00, 0xFF, 0x00]); // play lit
        assert_eq!(b[4], [0x40, 0x40, 0x40]); // system buttons dim white
        let b = c.bottom_colors(false, true);
        assert_eq!(b[2], [0xFF, 0x00, 0x00]); // rec lit
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
