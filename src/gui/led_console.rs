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
    /// Выбор активной папки-трека (#57): слоты 1..=7 = папки, слот 0 = выход.
    Tracks,
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
    Undo,
    /// Pad 32 held >= HOLD: redo the last undone sequencer edit.
    Redo,
    PageBack,
    PageForward,
    /// Пэд 30 (ВЛЕВО): шаг назад — Home: соседний перф, Pattern: соседний
    /// паттерн. РОДНАЯ семантика (e5ad801) — восстановлена 2026-11-05
    /// после ошибочного удаления в #72/#73.
    StepPerf(i32),
    StepPattern(i32),
    /// Tracks-режим (#57): выбрать папку (window-local 0-based, канон SelectPattern).
    TrackSelect(usize),
}

/// Видимых слотов верхней строки (слоты 0..=6, 7-й — MENU).
pub(crate) const CONSOLE_VISIBLE: usize = 7;

/// Максимальный ЛЕГАЛЬНЫЙ старт окна для списка длиной len.
pub(crate) fn window_max_start(len: usize) -> usize {
    len.saturating_sub(CONSOLE_VISIBLE)
}

/// Auto-follow: если idx вне видимого окна [start, start+7) — вернуть новый
/// старт (зажатый легальными страницами), иначе None (не двигать).
/// Регрессия #50: паттерн 7 ставил окно в 7 — вырожденная страница.
pub(crate) fn follow_start(idx: usize, start: usize, len: usize) -> Option<usize> {
    if (start..start + CONSOLE_VISIBLE).contains(&idx) {
        None
    } else {
        // СНАП НА ГРАНИЦУ СТРАНИЦЫ: сырой idx.min(max) давал off-grid окна
        // (idx 6 → окно 6 → вся раскладка пэдов сдвинута на 1 — «pad 20 даёт
        // Halloween», аудит #50). Страница = кратная 7, зажатая последней.
        let page = (idx / CONSOLE_VISIBLE) * CONSOLE_VISIBLE;
        Some(page.min(window_max_start(len)))
    }
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
                // Menu-слот 3 (#57): Tracks — выбор активной папки-трека
                3 => { self.mode = Mode::Tracks; None }
                _ => None, // dark slot in menu: no-op
            };
        }
        if self.mode == Mode::Tracks {
            // слот 0 = MENU (белый) — тап возвращает в HOME
            if slot == 0 { self.mode = Mode::Home; return None; }
            return Some(Action::TrackSelect(slot - 1)); // window-local, канон SelectPattern
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
            // Tracks обрабатывается выше (ранний return), Menu — выше
            Mode::Tracks | Mode::Menu => unreachable!(),
        })
    }

    fn step(&mut self, dir: i32) -> Action {
        // РОДНАЯ семантика пэдов 30/31 (ВЛЕВО/ВПРАВО, e5ad801): шаг по
        // списку активного режима. Страницы — пэды 28/29 (PageBack/Forward).
        // Ошибочно заменено на paging в #72/#73 — восстановлено 2026-11-05;
        // следом follow теперь снапится на страницы (#50), так что
        // degenerate-window баг больше не сопровождает StepPattern.
        match self.mode {
            Mode::Home => Action::StepPerf(dir),
            Mode::Pattern => Action::StepPattern(dir),
            _ => Action::StepPerf(dir),
        }
    }

    /// LED colors for slots 0..=7 (top row). `w_perf`/`w_pat`/`w_mute` — first
    /// index of the visible window: slot i (1..=7) shows item window + i - 1.
    /// MUTE slots cover all parts via w_mute paging (PAGE pads 28/29).
    /// Slot 0 is the MENU button (white). No transport-target item since
    /// #32 фаза 4: transport is always master, menu slot 3 is dark.
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
                // MUTE -> green; slot 3 is dark (target toggle removed)
                for (i, col) in c.iter_mut().enumerate() {
                    *col = match i {
                        0 => AVAIL,              // HOME: blue (perfs)
                        1 => [0xFF, 0xFF, 0x00], // PATTERN: yellow
                        2 => [0x00, 0xFF, 0x00], // MUTE: green
                        _ => [0x00, 0x00, 0x00],
                    };
                }
            }
            // Tracks рисует mod.rs через folder_colors — сюда не доходит
            Mode::Tracks => {}
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

    /// Программная смена режима (TrackSelect → Home, #57). Не через tap().
    pub fn force_mode(&mut self, m: Mode) {
        self.pending.clear();
        self.mode = m;
    }

    /// Пэды режима Tracks (#57): слот 0 = MENU белый, 1..=7 = папки (активная
    /// FF00FF пурпур, остальные 0066FF синий, пустые — чёрный).
    pub fn folder_colors(
        &self, active_folder: Option<usize>, filled: usize, w: usize,
    ) -> [[u8; 3]; 8] {
        let mut out = [[0x00, 0x00, 0x00]; 8];
        out[0] = [0xFF, 0xFF, 0xFF]; // MENU
        for (i, col) in out.iter_mut().enumerate().skip(1) {
            let idx = w + i - 1;
            if idx >= filled { break; }
            *col = if active_folder == Some(idx) {
                [0xFF, 0x00, 0xFF] // selected: magenta
            } else {
                [0x00, 0x66, 0xFF] // available: blue
            };
        }
        out
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
    fn prev_next_step_semantics() {
        // РОДНАЯ семантика 30/31 (ВЛЕВО/ВПРАВО): Home = StepPerf,
        // Pattern = StepPattern. Страницы — пэды 28/29.
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
        let home = c.top_colors(Some(2), 5, 0, &mutes, 2, 0, 0);
        assert_eq!(home[1], [0xFF, 0x00, 0xFF]); // selected (idx 2)
        assert_eq!(home[2], [0x00, 0x66, 0xFF]); // available (idx 3)
        // slot 4 = idx 5 -> за концом списка (count 5) -> OFF
        assert_eq!(home[4], [0x00, 0x00, 0x00]);
        // pattern window 1: slot 1 = pattern 1 (selected при current=1)
        c.mode = Mode::Pattern;
        let pat = c.top_colors(None, 0, 1, &mutes, 0, 1, 0);
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
    fn menu_slot3_tap_opens_tracks() {
        // #57 TC5: MENU-слот 3 — новый режим Tracks (был тёмный no-op)
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let now = Instant::now();
        c.on_event(PadEvent::Down(3), now);
        assert_eq!(c.on_event(PadEvent::Up(3), now + Duration::from_millis(30)), None);
        assert_eq!(c.mode, Mode::Tracks);
    }

    #[test]
    fn tracks_slot_tap_selects_folder() {
        // #57 TC5: слоты 1..=7 = папки, window-local 0-based
        let mut c = LedConsole::new();
        c.mode = Mode::Tracks;
        let now = Instant::now();
        c.on_event(PadEvent::Down(1), now);
        assert_eq!(c.on_event(PadEvent::Up(1), now + Duration::from_millis(30)),
                   Some(Action::TrackSelect(0)));
    }

    #[test]
    fn tracks_menu_slot_tap_returns_home() {
        // #57: слот 0 в Tracks = возврат в HOME (не открыть меню)
        let mut c = LedConsole::new();
        c.mode = Mode::Tracks;
        let now = Instant::now();
        c.on_event(PadEvent::Down(0), now);
        assert_eq!(c.on_event(PadEvent::Up(0), now + Duration::from_millis(30)), None);
        assert_eq!(c.mode, Mode::Home);
    }

    #[test]
    fn menu_slot3_led_is_off() {
        let mut c = LedConsole::new();
        c.mode = Mode::Menu;
        let mutes = [false; 8];
        let menu = c.top_colors(None, 0, 0, &mutes, 0, 0, 0);
        assert_eq!(menu[3], [0x00, 0x00, 0x00], "slot 3 (ex-target toggle): OFF");
        // остальные пункты меню не изменились
        assert_eq!(menu[0], [0x00, 0x66, 0xFF]); // HOME: blue
        assert_eq!(menu[1], [0xFF, 0xFF, 0x00]); // PATTERN: yellow
        assert_eq!(menu[2], [0x00, 0xFF, 0x00]); // MUTE: green
    }

    #[test]
    fn mute_mode_windows_over_all_parts() {
        let mut c = LedConsole::new();
        c.mode = Mode::Mute;
        let mutes: [bool; 16] = std::array::from_fn(|i| i == 7 || i == 14);
        // window 7: slot 1 = part 7 (muted -> OFF), slot 2 = part 8 (sound -> green)
        let top = c.top_colors(None, 0, 0, &mutes, 0, 0, 7);
        assert_eq!(top[1], [0x00, 0x00, 0x00], "muted part 7: OFF");
        assert_eq!(top[2], [0x00, 0xFF, 0x00], "sounding part 8: green");
        // window 14 (last page): slot 1 = part 14 (muted -> OFF),
        // slot 2 = part 15 (< 16 -> green)
        let top = c.top_colors(None, 0, 0, &mutes, 0, 0, 14);
        assert_eq!(top[1], [0x00, 0x00, 0x00], "muted part 14: OFF");
        assert_eq!(top[2], [0x00, 0xFF, 0x00], "part 15: green");
        // за концом списка -> OFF (окно 14, slot 3 = part 16 — не существует)
        assert_eq!(top[3], [0x00, 0x00, 0x00]);
    }

    #[test]
    fn tc1_window_max_start() {
        assert_eq!(window_max_start(8), 1);   // паттерны: 2 страницы
        assert_eq!(window_max_start(16), 9);  // мьюты: 16 партов
        assert_eq!(window_max_start(30), 23); // перфы
        assert_eq!(window_max_start(3), 0);   // короче окна — одна страница
    }

    #[test]
    fn tc2_follow_clamps_to_last_page_start() {
        // Регрессия finding 2: выбор паттерна 7 ставил окно на вырожденную
        // страницу 7; обязан — на старт последней легальной страницы
        assert_eq!(follow_start(7, 0, 8), Some(1));
    }

    #[test]
    fn tc3_follow_inside_view_noop() {
        assert_eq!(follow_start(3, 0, 8), None);
        assert_eq!(follow_start(6, 0, 8), None);
    }

    #[test]
    fn tc4_follow_left_escape() {
        assert_eq!(follow_start(0, 1, 8), Some(0));
    }

    #[test]
    fn tc5_follow_right_edge_visible() {
        assert_eq!(follow_start(7, 1, 8), None);
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
