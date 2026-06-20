//! MIDI looper: records note events with sample-accurate timing,
//! plays them back in a loop. Overdub support with per-part undo.
//! Zero heap allocation in the audio path.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use std::sync::Mutex;

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

/// Compact event snapshot for GUI timeline visualization.
#[derive(Clone, Copy)]
pub struct LooperDisplayEvent {
    pub position: f32,   // 0.0..1.0 fraction of loop
    pub note: u8,
    pub velocity: u8,    // 0 = note-off
    pub layer_id: u8,
    pub part_id: u8,
}

/// Shared display state for GUI (updated on record/undo/clear, not every sample).
pub struct LooperDisplay {
    pub events: Vec<LooperDisplayEvent>,
    pub current_layer: u8,
}

impl LooperDisplay {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            events: Vec::new(),
            current_layer: 0,
        }))
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
#[derive(Clone, Copy, Default)]
struct LoopEvent {
    tick: u32,      // sample offset from loop start
    note: u8,       // MIDI note 0-127
    velocity: u8,   // 0 = note-off, 1-127 = note-on
    layer_id: u8,   // overdub layer (for undo)
    part_id: u8,    // which synth part this was recorded on
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

    #[allow(dead_code)]
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
    // Track active notes for boundary note-off (255 = inactive, 0-7 = part_id)
    active_notes: [u8; 128],
    // Track which layer produced each active note (for mute note-off)
    active_note_layers: [u8; 128],
    needs_note_offs: bool,  // true when stop() was called with active notes
    recording_start_tick: u64, // global tick when recording started
    global_tick: u64,
    // Per-layer mute/solo
    layer_mute: [bool; 256],
    solo_layer: Option<u8>,
    atoms: Arc<LooperAtoms>,
    display: Arc<Mutex<LooperDisplay>>,
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
            active_notes: [255; 128],
            active_note_layers: [0; 128],
            needs_note_offs: false,
            recording_start_tick: 0,
            global_tick: 0,
            layer_mute: [false; 256],
            solo_layer: None,
            atoms: LooperAtoms::new(),
            display: LooperDisplay::new(),
        }
    }

    pub fn atoms(&self) -> Arc<LooperAtoms> {
        self.atoms.clone()
    }

    pub fn display(&self) -> Arc<Mutex<LooperDisplay>> {
        self.display.clone()
    }

    #[allow(dead_code)]
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
            LooperState::Idle if self.len > 0 => {
                self.state = LooperState::Playing;
                self.playback_tick = 0;
                self.playback_cursor = 0;
            }
            _ => {}
        }
    }

    fn stop(&mut self) {
        self.state = LooperState::Idle;
        // Signal tick() to emit note-offs for any still-active notes
        if self.active_notes.iter().any(|&a| a != 255) {
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
        self.update_display();
    }

    /// Clear everything.
    pub fn clear(&mut self) {
        self.len = 0;
        self.loop_length_ticks = 0;
        self.playback_tick = 0;
        self.playback_cursor = 0;
        self.current_layer = 0;
        self.state = LooperState::Idle;
        self.active_notes = [255; 128];
        self.active_note_layers = [0; 128];
        self.needs_note_offs = false;
        self.layer_mute = [false; 256];
        self.solo_layer = None;
        self.update_display();
    }

    /// Mute/unmute a layer. Returns note-offs for currently sounding notes from that layer.
    pub fn set_layer_mute(&mut self, layer: u8, mute: bool) -> [(u8, u8, u8); MAX_SIMULTANEOUS] {
        let mut out = [(0u8, 0u8, 255u8); MAX_SIMULTANEOUS];
        self.layer_mute[layer as usize] = mute;
        if mute {
            // Send note-offs for any active notes from this layer
            let mut count = 0;
            for note in 0..128u8 {
                if self.active_notes[note as usize] != 255
                    && self.active_note_layers[note as usize] == layer
                    && count < MAX_SIMULTANEOUS
                {
                    out[count] = (note, 0, self.active_notes[note as usize]);
                    self.active_notes[note as usize] = 255;
                    count += 1;
                }
            }
        }
        out
    }

    /// Set solo layer. None = no solo (all unmuted layers play).
    pub fn set_solo(&mut self, layer: Option<u8>) -> [(u8, u8, u8); MAX_SIMULTANEOUS] {
        let mut out = [(0u8, 0u8, 255u8); MAX_SIMULTANEOUS];
        self.solo_layer = layer;
        // Send note-offs for active notes NOT in the solo layer
        if let Some(solo) = layer {
            let mut count = 0;
            for note in 0..128u8 {
                if self.active_notes[note as usize] != 255
                    && self.active_note_layers[note as usize] != solo
                    && count < MAX_SIMULTANEOUS
                {
                    out[count] = (note, 0, self.active_notes[note as usize]);
                    self.active_notes[note as usize] = 255;
                    count += 1;
                }
            }
        }
        out
    }

    /// Check if a layer should be audible (considering mute + solo).
    #[inline]
    fn is_layer_audible(&self, layer_id: u8) -> bool {
        if let Some(solo) = self.solo_layer {
            return layer_id == solo;
        }
        !self.layer_mute[layer_id as usize]
    }

    /// Record a MIDI event. Called from audio thread when a note event arrives.
    /// `part` is the synth part index that was active when this note was played.
    pub fn record_event(&mut self, note: u8, velocity: u8, part: u8) {
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
            part_id: part,
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
        self.update_display();
    }

    /// Advance one sample. Returns events to fire: (note, velocity, part_id) triples.
    /// velocity=0 means note-off.
    #[inline]
    pub fn tick(&mut self) -> [(u8, u8, u8); MAX_SIMULTANEOUS] {
        let mut out = [(0u8, 0u8, 255u8); MAX_SIMULTANEOUS];
        self.global_tick += 1;

        if self.state == LooperState::Idle || self.loop_length_ticks == 0 {
            // Drain pending note-offs from a stop() call
            if self.needs_note_offs {
                let mut count = 0;
                for note in 0..128u8 {
                    let part = self.active_notes[note as usize];
                    if part != 255 && count < MAX_SIMULTANEOUS {
                        out[count] = (note, 0, part);
                        self.active_notes[note as usize] = 255;
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
            self.playback_cursor += 1;

            if !self.is_layer_audible(e.layer_id) { continue; }

            out[count] = (e.note, e.velocity, e.part_id);
            // Track active notes + which layer produced them
            if e.velocity > 0 {
                self.active_notes[e.note as usize] = e.part_id;
                self.active_note_layers[e.note as usize] = e.layer_id;
            } else {
                self.active_notes[e.note as usize] = 255;
            }
            count += 1;
        }

        self.playback_tick += 1;

        // Loop wrap
        if self.playback_tick >= self.loop_length_ticks {
            // Force note-off for any still-active notes
            for note in 0..128u8 {
                let part = self.active_notes[note as usize];
                if part != 255 && count < MAX_SIMULTANEOUS {
                    out[count] = (note, 0, part);
                    self.active_notes[note as usize] = 255;
                    count += 1;
                }
            }
            self.playback_tick = 0;
            self.playback_cursor = 0;
        }

        self.update_atoms();
        out
    }

    fn update_display(&self) {
        if let Ok(mut d) = self.display.try_lock() {
            d.events.clear();
            if self.loop_length_ticks > 0 {
                let inv = 1.0 / self.loop_length_ticks as f32;
                for i in 0..self.len as usize {
                    let e = &self.events[i];
                    d.events.push(LooperDisplayEvent {
                        position: e.tick as f32 * inv,
                        note: e.note,
                        velocity: e.velocity,
                        layer_id: e.layer_id,
                        part_id: e.part_id,
                    });
                }
            }
            d.current_layer = self.current_layer;
        }
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

}
