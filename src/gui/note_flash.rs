//! Затухающие следы нот для клавиатуры. Held — из атомиков note_state,
//! здесь только хвост после NoteOff (быстрые ноты: атомик on→off в кадре).

use super::super::midi::NoteFlash;
use std::time::{Duration, Instant};

pub const FLASH_DECAY_MS: u64 = 120;

pub struct NoteFlashState {
    until: [Instant; 128],
}

impl Default for NoteFlashState {
    fn default() -> Self {
        Self { until: std::array::from_fn(|_| Instant::now()) }
    }
}

impl NoteFlashState {
    pub fn new() -> Self { Self::default() }

    pub fn flash(&mut self, note: u8) {
        // clamp как в brightness: нота ≥128 не должна заворачиваться на другую
        // клавишу (200 & 127 == 72), а должна попадать на последнюю.
        let n = (note.min(127)) as usize;
        self.until[n] = Instant::now() + Duration::from_millis(FLASH_DECAY_MS);
    }

    /// Яркость хвоста 0..1; held (по атомику) всегда 1.0.
    /// Используется отрисовкой клавиатуры (gui/keyboard.rs).
    #[allow(dead_code)]
    pub fn brightness(&self, note: usize, atom_held: bool) -> f32 {
        if atom_held { return 1.0; }
        let left = self.until[note.min(127)]
            .saturating_duration_since(Instant::now());
        ((left.as_millis() as f32) / FLASH_DECAY_MS as f32).clamp(0.0, 1.0)
    }

    pub fn any_active(&self) -> bool {
        self.until.iter().any(|&t| t > Instant::now())
    }

    /// Дрен кольца. Возвращает true, если были события (repaint-хинт).
    pub fn drain(&mut self, rx: &mut rtrb::Consumer<NoteFlash>) -> bool {
        let mut seen = false;
        while let Ok(ev) = rx.pop() {
            self.flash(ev.note);
            seen = true;
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc1_off_sets_decay_tail() {
        let mut st = NoteFlashState::new();
        st.flash(60);
        assert!(st.brightness(60, false) > 0.99, "сразу после note-off след полный");
    }

    #[test]
    fn tc2_held_atom_wins() {
        let st = NoteFlashState::new();
        assert_eq!(st.brightness(60, true), 1.0);
    }

    #[test]
    fn tc5_decay_reaches_zero() {
        let mut st = NoteFlashState::new();
        st.flash(60);
        // Сдвиг в прошлое — имитация истечения
        st.until[60] -= Duration::from_millis(200);
        assert_eq!(st.brightness(60, false), 0.0);
    }

    #[test]
    fn tc3_ring_overflow_no_panic() {
        let (mut tx, mut rx) = rtrb::RingBuffer::<NoteFlash>::new(4);
        for _ in 0..16 { let _ = tx.push(NoteFlash { note: 60 }); }
        let mut st = NoteFlashState::new();
        assert!(st.drain(&mut rx));
        assert!(!st.drain(&mut rx));
    }

    #[test]
    fn tc4_note_clamped() {
        let mut st = NoteFlashState::new();
        st.flash(200);
        assert!(st.brightness(127, false) > 0.9);
    }
}
