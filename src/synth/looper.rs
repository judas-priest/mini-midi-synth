#![allow(dead_code)]
/// MIDI looper: records note events with sample-accurate timing,
/// plays them back in a loop. Overdub support with per-part undo.
/// Zero heap allocation in the audio path.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};

const MAX_LOOP_EVENTS: usize = 4096;
const MAX_SIMULTANEOUS: usize = 16;

/// Shared state for GUI to read without ring buffer spam.
pub struct LooperAtoms {
    pub state: AtomicU8,       // LooperState as u8
    pub position: AtomicU8,    // 0-255 = position fraction
    pub event_count: AtomicU16,
    pub layer_count: AtomicU8,
}

impl LooperAtoms {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(0),
            position: AtomicU8::new(0),
            event_count: AtomicU16::new(0),
            layer_count: AtomicU8::new(0),
        })
    }
}

impl LooperState {
    pub fn to_u8(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Recording => 1,
            Self::Playing => 2,
            Self::Overdubbing => 3,
        }
    }
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Recording,
            2 => Self::Playing,
            3 => Self::Overdubbing,
            _ => Self::Idle,
        }
    }
}

/// Compact recorded event: 8 bytes.
#[derive(Clone, Copy)]
struct LoopEvent {
    tick: u32,      // sample offset from loop start
    note: u8,       // MIDI note 0-127
    velocity: u8,   // 0 = note-off, 1-127 = note-on
    layer_id: u8,   // overdub part (for undo)
    _pad: u8,
}

impl Default for LoopEvent {
    fn default() -> Self {
        Self { tick: 0, note: 0, velocity: 0, layer_id: 0, _pad: 0 }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum LooperState {
    Idle,
    Recording,   // first pass: establishing loop length
    Playing,
    Overdubbing, // playing + recording
}

#[derive(Clone, Copy, PartialEq)]
pub enum Quantize {
    Off,
    Quarter,    // 1/4
    Eighth,     // 1/8
    Sixteenth,  // 1/16
}

impl Quantize {
    pub fn from_index(i: u8) -> Self {
        match i {
            1 => Self::Quarter,
            2 => Self::Eighth,
            3 => Self::Sixteenth,
            _ => Self::Off,
        }
    }

    pub fn index(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Quarter => 1,
            Self::Eighth => 2,
            Self::Sixteenth => 3,
        }
    }

    fn subdivisions(self) -> Option<f32> {
        match self {
            Self::Off => None,
            Self::Quarter => Some(1.0),
            Self::Eighth => Some(2.0),
            Self::Sixteenth => Some(4.0),
        }
    }
}

pub struct MidiLooper {
    events: Box<[LoopEvent; MAX_LOOP_EVENTS]>,
    len: u16,
    pub loop_length_ticks: u32,
    playback_tick: u32,
    playback_cursor: usize,
    current_layer: u8,
    pub state: LooperState,
    pub bpm: f32,
    pub bars: u8,           // loop length in bars (1,2,4,8)
    pub quantize: Quantize,
    pub sample_rate: f32,
    // Track active notes for boundary note-off
    active_notes: [bool; 128],
    needs_note_offs: bool,  // true when stop() was called with active notes
    recording_start_tick: u64, // global tick when recording started
    global_tick: u64,
    atoms: Arc<LooperAtoms>,
}

impl MidiLooper {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            events: Box::new([LoopEvent::default(); MAX_LOOP_EVENTS]),
            len: 0,
            loop_length_ticks: 0,
            playback_tick: 0,
            playback_cursor: 0,
            current_layer: 0,
            state: LooperState::Idle,
            bpm: 120.0,
            bars: 4,
            quantize: Quantize::Off,
            sample_rate,
            active_notes: [false; 128],
            needs_note_offs: false,
            recording_start_tick: 0,
            global_tick: 0,
            atoms: LooperAtoms::new(),
        }
    }

    pub fn atoms(&self) -> Arc<LooperAtoms> {
        self.atoms.clone()
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.clear();
    }

    fn bar_length_samples(&self) -> u32 {
        ((60.0 / self.bpm) * 4.0 * self.sample_rate) as u32 // 4 beats per bar
    }

    fn quantize_tick(&self, tick: u32) -> u32 {
        if let Some(subdivs) = self.quantize.subdivisions() {
            let beat_samples = (60.0 / self.bpm) * self.sample_rate;
            let grid = (beat_samples / subdivs) as u32;
            if grid == 0 { return tick; }
            let nearest = ((tick + grid / 2) / grid) * grid;
            if self.loop_length_ticks > 0 {
                nearest % self.loop_length_ticks
            } else {
                nearest
            }
        } else {
            tick
        }
    }

    /// Start recording. If first time, establishes loop length from bars setting.
    pub fn start_record(&mut self) {
        if self.state == LooperState::Idle {
            // First recording pass
            self.len = 0;
            self.current_layer = 0;
            self.loop_length_ticks = self.bar_length_samples() * self.bars as u32;
            self.recording_start_tick = self.global_tick;
            self.playback_tick = 0;
            self.playback_cursor = 0;
            self.state = LooperState::Recording;
        } else if self.state == LooperState::Playing {
            // Start overdub
            self.current_layer += 1;
            self.state = LooperState::Overdubbing;
        }
    }

    /// Toggle record: idle→record, playing→overdub, recording/overdubbing→play.
    pub fn toggle_record(&mut self) {
        match self.state {
            LooperState::Idle | LooperState::Playing => self.start_record(),
            LooperState::Recording | LooperState::Overdubbing => self.stop_record(),
        }
    }

    /// Stop recording → switch to playback.
    pub fn stop_record(&mut self) {
        match self.state {
            LooperState::Recording | LooperState::Overdubbing => {
                self.state = LooperState::Playing;
            }
            _ => {}
        }
    }

    /// Toggle play/stop.
    pub fn toggle_play(&mut self) {
        match self.state {
            LooperState::Playing | LooperState::Overdubbing => {
                self.stop();
            }
            LooperState::Idle => {
                if self.len > 0 {
                    self.state = LooperState::Playing;
                    self.playback_tick = 0;
                    self.playback_cursor = 0;
                }
            }
            _ => {}
        }
    }

    fn stop(&mut self) {
        self.state = LooperState::Idle;
        // Signal tick() to emit note-offs for any still-active notes
        if self.active_notes.iter().any(|&a| a) {
            self.needs_note_offs = true;
        }
    }

    /// Undo last overdub part.
    pub fn undo(&mut self) {
        if self.current_layer == 0 { return; }
        let target = self.current_layer;
        let mut write = 0usize;
        for read in 0..self.len as usize {
            if self.events[read].layer_id != target {
                self.events[write] = self.events[read];
                write += 1;
            }
        }
        self.len = write as u16;
        self.current_layer -= 1;
        // Reset cursor to safe position
        self.playback_cursor = self.events[..self.len as usize]
            .partition_point(|e| e.tick < self.playback_tick);
    }

    /// Clear everything.
    pub fn clear(&mut self) {
        self.len = 0;
        self.loop_length_ticks = 0;
        self.playback_tick = 0;
        self.playback_cursor = 0;
        self.current_layer = 0;
        self.state = LooperState::Idle;
        self.active_notes = [false; 128];
        self.needs_note_offs = false;
    }

    /// Record a MIDI event. Called from audio thread when a note event arrives.
    pub fn record_event(&mut self, note: u8, velocity: u8) {
        if self.state != LooperState::Recording && self.state != LooperState::Overdubbing {
            return;
        }
        if self.len >= MAX_LOOP_EVENTS as u16 { return; }
        if note >= 128 { return; }

        let raw_tick = self.playback_tick;
        let tick = self.quantize_tick(raw_tick);

        let event = LoopEvent {
            tick,
            note,
            velocity,
            layer_id: self.current_layer,
            _pad: 0,
        };

        // Insert sorted by tick
        let pos = self.events[..self.len as usize]
            .partition_point(|e| e.tick < tick);
        let len = self.len as usize;
        // Shift right
        if pos < len {
            self.events.copy_within(pos..len, pos + 1);
        }
        self.events[pos] = event;
        self.len += 1;

        // Adjust cursor if insertion was before it
        if pos <= self.playback_cursor && self.playback_cursor < self.len as usize {
            self.playback_cursor += 1;
        }
    }

    /// Advance one sample. Returns events to fire: (note, velocity) pairs.
    /// velocity=0 means note-off.
    #[inline]
    pub fn tick(&mut self) -> [(u8, u8); MAX_SIMULTANEOUS] {
        let mut out = [(0u8, 0u8); MAX_SIMULTANEOUS];
        self.global_tick += 1;

        if self.state == LooperState::Idle || self.loop_length_ticks == 0 {
            // Drain pending note-offs from a stop() call
            if self.needs_note_offs {
                let mut count = 0;
                for note in 0..128u8 {
                    if self.active_notes[note as usize] && count < MAX_SIMULTANEOUS {
                        out[count] = (note, 0);
                        self.active_notes[note as usize] = false;
                        count += 1;
                    }
                }
                self.needs_note_offs = false;
            }
            self.update_atoms();
            return out;
        }

        // Not playing back during initial recording (only after first pass)
        if self.state == LooperState::Recording {
            self.playback_tick += 1;
            if self.playback_tick >= self.loop_length_ticks {
                // First pass done → switch to overdub automatically
                self.playback_tick = 0;
                self.playback_cursor = 0;
                self.state = LooperState::Overdubbing;
                self.current_layer += 1;
            }
            self.update_atoms();
            return out;
        }

        // Playback / Overdub: emit events at current tick
        let mut count = 0;
        while self.playback_cursor < self.len as usize
            && self.events[self.playback_cursor].tick == self.playback_tick
            && count < MAX_SIMULTANEOUS
        {
            let e = self.events[self.playback_cursor];
            out[count] = (e.note, e.velocity);
            // Track active notes
            if e.velocity > 0 {
                self.active_notes[e.note as usize] = true;
            } else {
                self.active_notes[e.note as usize] = false;
            }
            count += 1;
            self.playback_cursor += 1;
        }

        self.playback_tick += 1;

        // Loop wrap
        if self.playback_tick >= self.loop_length_ticks {
            // Force note-off for any still-active notes
            for note in 0..128u8 {
                if self.active_notes[note as usize] && count < MAX_SIMULTANEOUS {
                    out[count] = (note, 0);
                    self.active_notes[note as usize] = false;
                    count += 1;
                }
            }
            self.playback_tick = 0;
            self.playback_cursor = 0;
        }

        self.update_atoms();
        out
    }

    fn update_atoms(&self) {
        self.atoms.state.store(self.state.to_u8(), Ordering::Relaxed);
        let pos = if self.loop_length_ticks > 0 {
            (self.playback_tick as f64 / self.loop_length_ticks as f64 * 255.0) as u8
        } else { 0 };
        self.atoms.position.store(pos, Ordering::Relaxed);
        self.atoms.event_count.store(self.len, Ordering::Relaxed);
        self.atoms.layer_count.store(self.current_layer + 1, Ordering::Relaxed);
    }

    pub fn is_recording(&self) -> bool {
        self.state == LooperState::Recording || self.state == LooperState::Overdubbing
    }

    pub fn event_count(&self) -> u16 { self.len }
    pub fn layer_count(&self) -> u8 { self.current_layer + 1 }

    /// Current position as fraction 0..1 for GUI progress indicator.
    pub fn position_fraction(&self) -> f32 {
        if self.loop_length_ticks == 0 { return 0.0; }
        self.playback_tick as f32 / self.loop_length_ticks as f32
    }
}
