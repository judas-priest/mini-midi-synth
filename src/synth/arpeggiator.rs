/// Per-engine arpeggiator: generates note-on/note-off events from held notes.
/// Zero-alloc on the audio thread — uses fixed-size arrays throughout.

const MAX_HELD: usize = 16;
const MAX_PATTERN: usize = 64; // 16 notes * 4 octaves

pub struct Arpeggiator {
    held_notes: [(u8, u8); MAX_HELD],  // (note, velocity)
    held_count: usize,
    pattern: [(u8, u8); MAX_PATTERN],  // (note, velocity)
    pattern_len: usize,
    position: usize,
    tick_counter: u32,
    step_samples: u32,
    gate_samples: u32,
    current_note: Option<u8>,
    pub enabled: bool,
    mode: u8,      // 0=Up, 1=Down, 2=UpDown, 3=Random, 4=Order
    rate: u8,      // 0=1/4, 1=1/8, 2=1/16, 3=1/32, 4=1/4T, 5=1/8T
    octaves: u8,   // 1-4
    gate: f32,     // 0.1..1.0
    bpm: f32,
    sample_rate: f32,
    going_up: bool,     // for UpDown mode direction tracking
    rng_state: u32,     // xorshift32 for Random mode
}

impl Arpeggiator {
    pub fn new(sample_rate: f32) -> Self {
        let mut arp = Self {
            held_notes: [(0, 0); MAX_HELD],
            held_count: 0,
            pattern: [(0, 0); MAX_PATTERN],
            pattern_len: 0,
            position: 0,
            tick_counter: 0,
            step_samples: 0,
            gate_samples: 0,
            current_note: None,
            enabled: false,
            mode: 0,
            rate: 1,
            octaves: 1,
            gate: 0.5,
            bpm: 120.0,
            sample_rate,
            going_up: true,
            rng_state: 0xDEAD_BEEF,
        };
        arp.recalc_timing();
        arp
    }

    pub fn set_params(&mut self, enabled: bool, mode: u8, rate: u8, octaves: u8, gate: f32) {
        self.enabled = enabled;
        self.mode = mode.min(4);
        self.rate = rate.min(5);
        self.octaves = octaves.clamp(1, 4);
        self.gate = gate.clamp(0.1, 1.0);
        self.recalc_timing();
        if !enabled {
            self.clear();
        }
    }

    pub fn set_bpm(&mut self, bpm: f32) {
        if (self.bpm - bpm).abs() > 0.01 {
            self.bpm = bpm;
            self.recalc_timing();
        }
    }

    pub fn note_on(&mut self, note: u8, velocity: u8) {
        // Don't add duplicates
        for i in 0..self.held_count {
            if self.held_notes[i].0 == note {
                self.held_notes[i].1 = velocity;
                self.rebuild_pattern();
                return;
            }
        }
        if self.held_count < MAX_HELD {
            self.held_notes[self.held_count] = (note, velocity);
            self.held_count += 1;
            self.rebuild_pattern();
        }
    }

    pub fn note_off(&mut self, note: u8) {
        let mut found = false;
        for i in 0..self.held_count {
            if self.held_notes[i].0 == note {
                // Shift remaining notes down
                for j in i..self.held_count.saturating_sub(1) {
                    self.held_notes[j] = self.held_notes[j + 1];
                }
                self.held_count -= 1;
                found = true;
                break;
            }
        }
        if found {
            if self.held_count == 0 {
                // Reset when all notes released
                self.position = 0;
                self.tick_counter = 0;
                self.pattern_len = 0;
            } else {
                self.rebuild_pattern();
            }
        }
    }

    /// Called per-sample. Returns (note_on, note_off).
    #[inline]
    pub fn tick(&mut self) -> (Option<(u8, u8)>, Option<u8>) {
        if !self.enabled || self.pattern_len == 0 || self.step_samples == 0 {
            return (None, None);
        }

        let mut note_on_out: Option<(u8, u8)> = None;
        let mut note_off_out: Option<u8> = None;

        if self.tick_counter == 0 {
            // Step boundary: send note-off for previous, note-on for current
            if let Some(prev) = self.current_note {
                note_off_out = Some(prev);
                self.current_note = None;
            }

            let (note, vel) = self.pattern[self.position];
            note_on_out = Some((note, vel));
            self.current_note = Some(note);

            // Advance position
            self.advance_position();
        } else if self.tick_counter == self.gate_samples {
            // Gate off within step
            if let Some(prev) = self.current_note {
                note_off_out = Some(prev);
                self.current_note = None;
            }
        }

        self.tick_counter += 1;
        if self.tick_counter >= self.step_samples {
            self.tick_counter = 0;
        }

        (note_on_out, note_off_out)
    }

    /// Returns note-off for any currently sounding arp note (used when arp is disabled or all notes off).
    pub fn flush_current_note(&mut self) -> Option<u8> {
        let note = self.current_note;
        self.current_note = None;
        note
    }

    pub fn clear(&mut self) {
        self.held_count = 0;
        self.pattern_len = 0;
        self.position = 0;
        self.tick_counter = 0;
        self.current_note = None;
        self.going_up = true;
    }

    fn recalc_timing(&mut self) {
        let beats_per_step = match self.rate {
            0 => 1.0,        // 1/4 note
            1 => 0.5,        // 1/8
            2 => 0.25,       // 1/16
            3 => 0.125,      // 1/32
            4 => 2.0 / 3.0,  // 1/4 triplet
            5 => 1.0 / 3.0,  // 1/8 triplet
            _ => 0.5,
        };
        let bpm = if self.bpm > 1.0 { self.bpm } else { 120.0 };
        self.step_samples = (beats_per_step * 60.0 / bpm * self.sample_rate) as u32;
        if self.step_samples == 0 { self.step_samples = 1; }
        self.gate_samples = (self.step_samples as f32 * self.gate) as u32;
        if self.gate_samples == 0 { self.gate_samples = 1; }
    }

    fn rebuild_pattern(&mut self) {
        if self.held_count == 0 {
            self.pattern_len = 0;
            return;
        }

        // Sort held notes by pitch for Up/Down/UpDown modes
        let mut sorted = self.held_notes;
        let count = self.held_count;

        // Simple insertion sort (max 16 elements)
        for i in 1..count {
            let key = sorted[i];
            let mut j = i;
            while j > 0 && sorted[j - 1].0 > key.0 {
                sorted[j] = sorted[j - 1];
                j -= 1;
            }
            sorted[j] = key;
        }

        let mut len = 0;

        match self.mode {
            0 => {
                // Up
                for oct in 0..self.octaves {
                    for i in 0..count {
                        if len >= MAX_PATTERN { break; }
                        let note = (sorted[i].0 as u16 + oct as u16 * 12).min(127) as u8;
                        self.pattern[len] = (note, sorted[i].1);
                        len += 1;
                    }
                }
            }
            1 => {
                // Down
                for oct in (0..self.octaves).rev() {
                    for i in (0..count).rev() {
                        if len >= MAX_PATTERN { break; }
                        let note = (sorted[i].0 as u16 + oct as u16 * 12).min(127) as u8;
                        self.pattern[len] = (note, sorted[i].1);
                        len += 1;
                    }
                }
            }
            2 => {
                // UpDown: up then down (without repeating top/bottom)
                // Up part
                for oct in 0..self.octaves {
                    for i in 0..count {
                        if len >= MAX_PATTERN { break; }
                        let note = (sorted[i].0 as u16 + oct as u16 * 12).min(127) as u8;
                        self.pattern[len] = (note, sorted[i].1);
                        len += 1;
                    }
                }
                // Down part (skip top note, skip bottom note)
                let total_up = count * self.octaves as usize;
                if total_up > 2 {
                    for idx in (1..total_up - 1).rev() {
                        if len >= MAX_PATTERN { break; }
                        // Reconstruct from sorted
                        let oct = idx / count;
                        let note_idx = idx % count;
                        let note = (sorted[note_idx].0 as u16 + oct as u16 * 12).min(127) as u8;
                        self.pattern[len] = (note, sorted[note_idx].1);
                        len += 1;
                    }
                }
            }
            3 => {
                // Random: fill pattern with all available notes, position picked randomly in advance_position
                for oct in 0..self.octaves {
                    for i in 0..count {
                        if len >= MAX_PATTERN { break; }
                        let note = (sorted[i].0 as u16 + oct as u16 * 12).min(127) as u8;
                        self.pattern[len] = (note, sorted[i].1);
                        len += 1;
                    }
                }
            }
            4 => {
                // Order (as played) — use held_notes unsorted
                for oct in 0..self.octaves {
                    for i in 0..count {
                        if len >= MAX_PATTERN { break; }
                        let note = (self.held_notes[i].0 as u16 + oct as u16 * 12).min(127) as u8;
                        self.pattern[len] = (note, self.held_notes[i].1);
                        len += 1;
                    }
                }
            }
            _ => {}
        }

        self.pattern_len = len;

        // Clamp position to valid range
        if self.position >= self.pattern_len {
            self.position = 0;
        }
    }

    fn advance_position(&mut self) {
        if self.pattern_len == 0 { return; }

        if self.mode == 3 {
            // Random: xorshift32
            self.rng_state ^= self.rng_state << 13;
            self.rng_state ^= self.rng_state >> 17;
            self.rng_state ^= self.rng_state << 5;
            self.position = (self.rng_state as usize) % self.pattern_len;
        } else {
            self.position += 1;
            if self.position >= self.pattern_len {
                self.position = 0;
            }
        }
    }
}
