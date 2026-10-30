//! MIDI looper: records note events with sample-accurate timing,
//! plays them back in a loop. Overdub support with per-part undo.
//! Zero heap allocation in the audio path.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU16, Ordering};
use std::sync::Mutex;

const MAX_LOOP_EVENTS: usize = 4096;
const MAX_SIMULTANEOUS: usize = 16;
/// Per-part replay mute slots; matches synth::MAX_PARTS.
const MAX_LOOP_PARTS: usize = 16;

/// Shared state for GUI to read without ring buffer spam.
pub struct LooperAtoms {
    pub state: AtomicU8,       // LooperState as u8
    pub position: AtomicU8,    // 0-255 = position fraction
    pub event_count: AtomicU16,
    pub layer_count: AtomicU8,
    /// Real loop length in 16th steps (quantized capture = pattern cycle).
    pub loop_steps: AtomicU16,
}

impl LooperAtoms {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: AtomicU8::new(0),
            position: AtomicU8::new(0),
            event_count: AtomicU16::new(0),
            layer_count: AtomicU8::new(0),
            loop_steps: AtomicU16::new(0),
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

/// Serialisable event snapshot for looper restore across audio reconnects.
#[derive(Clone, Copy)]
pub struct LooperEventSnapshot {
    pub tick: u32,    // sample offset from loop start
    pub note: u8,
    pub velocity: u8, // 0 = note-off
    pub layer_id: u8,
    pub part_id: u8,
}

/// Shared display state for GUI (updated on record/undo/clear, not every sample).
pub struct LooperDisplay {
    pub events: Vec<LooperDisplayEvent>,
    pub current_layer: u8,
    pub loop_length_ticks: u32, // source of truth for GUI snapshot; 0 = none
}

impl LooperDisplay {
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            // Sized for the worst case up front: update_display runs on the
            // audio thread, where growing this Vec means calling malloc.
            events: Vec::with_capacity(MAX_LOOP_EVENTS),
            current_layer: 0,
            loop_length_ticks: 0,
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

#[derive(Clone, Copy, PartialEq, Debug)]
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
    /// BPM the current loop was established at; drives the regrid ratio on
    /// tempo changes. `0.0` = provenance unknown (post-restore): the next
    /// sync_clock adopts the new tempo WITHOUT rescaling the restored loop.
    loop_bpm: f32,
    /// Beats per bar, synced per block from the drum sequencer's current
    /// pattern (REFACTOR_AUDIT S5). Affects loop length only at the next
    /// start_record — an existing loop is never re-bared.
    pub beats_per_bar: u8,
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
    // Per-part replay mute (TECHDEBT #30/#34): a muted part's recorded
    // events are skipped in tick(); sounding notes of that part are flushed
    // by set_part_mute.
    part_mutes: [bool; MAX_LOOP_PARTS],
    atoms: Arc<LooperAtoms>,
    /// Last values published to `atoms` (write-on-change: tick() calls
    /// update_atoms every sample). position is cached as u16 because every
    /// u8 value is legal — u16::MAX is the "never written" sentinel.
    atom_cache: (u8, u16, u16, u8),
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
            loop_bpm: 120.0,
            beats_per_bar: 4,
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
            part_mutes: [false; MAX_LOOP_PARTS],
            atoms: LooperAtoms::new(),
            atom_cache: (u8::MAX, u16::MAX, u16::MAX, u8::MAX),
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
        ((60.0 / self.bpm) * self.beats_per_bar as f32 * self.sample_rate) as u32
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

    /// Per-block clock sync (REFACTOR_AUDIT S5): the drum sequencer BPM is the
    /// single canonical clock (same precedent as arp/pitch_seq in tick_block).
    /// Called once per block, before the per-sample tick() loop.
    ///
    /// A mid-loop tempo change rescales the recorded loop proportionally
    /// (event ticks, loop length, playback position): musical positions
    /// survive and the loop plays back at the new tempo like a DAW clip.
    /// Events are note on/offs, not audio — no time-stretch needed. Rounding
    /// error is bounded by ±1 sample per event per regrid; a BPM drag regrids
    /// every frame, worst case ~2 ms drift at 48 kHz after ~100 steps.
    /// RT-safe: integer math, no allocation.
    pub fn sync_clock(&mut self, bpm: f32, beats_per_bar: u8) {
        if !bpm.is_finite() || bpm <= 0.0 { return; }
        self.beats_per_bar = beats_per_bar.clamp(1, 16);
        if (self.bpm - bpm).abs() < 0.01 { return; }
        if self.loop_length_ticks > 0 && self.loop_bpm > 0.0 {
            self.regrid(bpm);
        }
        self.bpm = bpm;
        self.loop_bpm = bpm;
    }

    /// Rescale loop content and playback position from self.bpm to `new_bpm`.
    /// Per-event scale `t * new_len / old_len` is monotone non-decreasing in
    /// `t`, so the sorted-by-tick invariant is preserved (duplicates legal —
    /// record_event already inserts those). Cursor re-derivation mirrors
    /// undo() (partition_point below).
    fn regrid(&mut self, new_bpm: f32) {
        let old_len = self.loop_length_ticks as u64;
        let new_len = ((old_len as f64) * (self.bpm as f64 / new_bpm as f64)) as u64;
        if new_len == 0 || new_len > u32::MAX as u64 { return; }
        let new_len = new_len as u32;
        let scale = |t: u32| ((t as u64 * new_len as u64) / old_len) as u32;
        for i in 0..self.len as usize {
            self.events[i].tick = scale(self.events[i].tick);
        }
        let new_pos = scale(self.playback_tick);
        self.loop_length_ticks = new_len;
        self.playback_tick = new_pos.min(new_len - 1);
        self.playback_cursor = self.events[..self.len as usize]
            .partition_point(|e| e.tick < self.playback_tick);
        self.update_display();
    }

    /// Start recording. If first time, establishes loop length from bars setting.
    pub fn start_record(&mut self) {
        if self.state == LooperState::Idle {
            // First recording pass
            self.len = 0;
            self.current_layer = 0;
            self.loop_length_ticks = self.bar_length_samples() * self.bars as u32;
            self.loop_bpm = self.bpm;
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

    /// Quantized master rec (#30 sync): set the loop to an arbitrary tick
    /// length (в т.ч. длину цикла паттерна — размеры лупера и секвенсора
    /// совпадают для любых sig/free длин).
    pub fn set_loop_len_ticks(&mut self, ticks: u32) {
        self.loop_length_ticks = ticks.max(1);
        self.update_display();
        self.sync_atoms();
    }

    /// Loop length in ticks (test/GUI introspection).
    pub fn loop_len_ticks(&self) -> u32 {
        self.loop_length_ticks
    }

    /// Drift correction (Mobius-style): jump playback to the loop start so
    /// the loop stays phase-locked to the sequencer grid.
    pub fn seek_start(&mut self) {
        self.playback_tick = 0;
        self.playback_cursor = 0;
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
        self.sync_atoms();
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
        self.loop_bpm = 0.0;
        self.playback_tick = 0;
        self.playback_cursor = 0;
        self.current_layer = 0;
        self.state = LooperState::Idle;
        self.active_notes = [255; 128];
        self.active_note_layers = [0; 128];
        self.needs_note_offs = false;
        self.layer_mute = [false; 256];
        self.solo_layer = None;
        self.part_mutes = [false; MAX_LOOP_PARTS];
        self.update_display();
        self.sync_atoms();
    }

    /// Number of recorded events.
    #[allow(dead_code)]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u16 { self.len }

    /// Restore recorded content from a GUI-side snapshot (audio thread).
    /// Zero-alloc: copies up to MAX_LOOP_EVENTS from the borrowed slice.
    pub fn restore(&mut self, events: &[LooperEventSnapshot], loop_length_ticks: u32) {
        self.len = 0;
        self.playback_tick = 0;
        self.playback_cursor = 0;
        self.needs_note_offs = false;
        self.active_notes = [255; 128];
        self.active_note_layers = [0; 128];
        self.loop_length_ticks = loop_length_ticks;
        // Provenance unknown (ticks were rescaled to the new SR by the GUI):
        // first sync_clock must NOT regrid.
        self.loop_bpm = 0.0;
        self.state = LooperState::Idle;
        for ev in events {
            if self.len as usize >= MAX_LOOP_EVENTS { break; }
            self.events[self.len as usize] = LoopEvent {
                tick: ev.tick,
                note: ev.note,
                velocity: ev.velocity,
                layer_id: ev.layer_id,
                part_id: ev.part_id,
            };
            self.len += 1;
        }
        // Layer bookkeeping: restored events may reference layers of the old
        // engine; undo() targets current_layer, so advance to the max seen.
        self.current_layer = events.iter().map(|e| e.layer_id).max().unwrap_or(0);
        // Fresh engine: mute/solo are at defaults; events keep their old
        // layer_ids, which stay valid because ids are copied 1:1.
        self.sync_atoms();
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

    /// Mute/unmute a part's events in the loop (TECHDEBT #30/#34). Returns
    /// note-offs for currently sounding notes recorded from that part, the
    /// same contract as `set_layer_mute`.
    pub fn set_part_mute(&mut self, part: u8, mute: bool) -> [(u8, u8, u8); MAX_SIMULTANEOUS] {
        let mut out = [(0u8, 0u8, 255u8); MAX_SIMULTANEOUS];
        if (part as usize) < MAX_LOOP_PARTS {
            self.part_mutes[part as usize] = mute;
        }
        if mute {
            // Send note-offs for any active notes recorded from this part
            let mut count = 0;
            for note in 0..128u8 {
                if self.active_notes[note as usize] == part && count < MAX_SIMULTANEOUS {
                    out[count] = (note, 0, self.active_notes[note as usize]);
                    self.active_notes[note as usize] = 255;
                    count += 1;
                }
            }
        }
        out
    }

    /// Check if a part's recorded events should be replayed. Out-of-range
    /// part ids (shouldn't happen) stay audible.
    #[inline]
    fn is_part_audible(&self, part_id: u8) -> bool {
        !self.part_mutes.get(part_id as usize).copied().unwrap_or(false)
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
        if self.needs_note_offs {
            // Close sounding notes once, before the scheduled events at the
            // current tick fill the remaining slots
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
        while self.playback_cursor < self.len as usize
            && self.events[self.playback_cursor].tick == self.playback_tick
            && count < MAX_SIMULTANEOUS
        {
            let e = self.events[self.playback_cursor];
            self.playback_cursor += 1;

            if !self.is_layer_audible(e.layer_id) { continue; }
            // Part mute filter: skips note-ons AND their note-offs (a muted
            // part never sounds, so its offs are redundant); a part muted
            // mid-note got its sounding notes flushed by set_part_mute.
            if !self.is_part_audible(e.part_id) { continue; }

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

    /// Publishes a snapshot of recorded events for the GUI timeline.
    ///
    /// Runs on the audio thread. `try_lock` never blocks and the vector is
    /// pre-sized to MAX_LOOP_EVENTS so `push` cannot allocate. The `unlock`
    /// when the guard drops is still a futex syscall; replacing this Mutex
    /// with a triple buffer is recorded in docs/ARCHITECTURE.md.
    fn update_display(&self) {
        if let Ok(mut d) = self.display.try_lock() {
            debug_assert!(
                d.events.capacity() >= MAX_LOOP_EVENTS,
                "display vector must never grow on the audio thread"
            );
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
            d.loop_length_ticks = self.loop_length_ticks;
        }
    }

    /// Publishes looper state to the GUI atoms, storing only fields whose
    /// value actually changed. Called every sample from tick() — at 48 kHz
    /// the old version did ~48k atomic stores + an f64 divide per second even
    /// while Idle; the GUI reads at <= 60 Hz, so per-change publication is
    /// visually identical.
    fn update_atoms(&mut self) {
        let pos = if self.loop_length_ticks > 0 {
            (self.playback_tick as f64 / self.loop_length_ticks as f64 * 255.0) as u8
        } else { 0 };
        // Реальная длина петли в 16-х шагах (для честного GUI: таймлайн и
        // лейбл Loop, Bars ≠ длина после квантованного захвата)
        let sps = (self.sample_rate as f64 * 60.0) / ((self.bpm.max(1.0) as f64) * 4.0);
        let loop_steps = if sps > 0.0 { (self.loop_length_ticks as f64 / sps).round() as u16 } else { 0 };
        let next = (self.state.to_u8(), pos as u16, self.len, self.current_layer + 1);
        if next.0 != self.atom_cache.0 {
            self.atoms.state.store(next.0, Ordering::Relaxed);
            self.atom_cache.0 = next.0;
        }
        if next.1 != self.atom_cache.1 {
            self.atoms.position.store(next.1 as u8, Ordering::Relaxed);
            self.atom_cache.1 = next.1;
        }
        if next.2 != self.atom_cache.2 {
            self.atoms.event_count.store(next.2, Ordering::Relaxed);
            self.atom_cache.2 = next.2;
        }
        if next.3 != self.atom_cache.3 {
            self.atoms.layer_count.store(next.3, Ordering::Relaxed);
            self.atom_cache.3 = next.3;
        }
        let steps = self.atoms.loop_steps.load(Ordering::Relaxed);
        if steps != loop_steps {
            self.atoms.loop_steps.store(loop_steps, Ordering::Relaxed);
        }
    }

    /// Unconditionally re-publishes: invalidates the cache first. Required in
    /// restore/clear/stop, which mutate several fields at once and may run
    /// while tick() is not driving updates.
    fn sync_atoms(&mut self) {
        self.atom_cache = (u8::MAX, u16::MAX, u16::MAX, u8::MAX);
        self.atoms.loop_steps.store(u16::MAX, Ordering::Relaxed);
        self.update_atoms();
    }

}

#[cfg(test)]
mod restore_tests {
    use super::*;

    fn snap(tick: u32, note: u8, vel: u8) -> LooperEventSnapshot {
        LooperEventSnapshot { tick, note, velocity: vel, layer_id: 0, part_id: 0 }
    }

    #[test]
    fn restore_carries_content_and_length() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[snap(0, 60, 127), snap(24_000, 60, 0)], 48_000);
        assert_eq!(l.len(), 2);
        assert_eq!(l.loop_length_ticks, 48_000);
        assert_eq!(l.state, LooperState::Idle);
    }

    #[test]
    fn restore_sets_current_layer_to_max() {
        let mut l = MidiLooper::new(48_000.0);
        let events = vec![
            LooperEventSnapshot { tick: 0, note: 60, velocity: 127, layer_id: 2, part_id: 0 },
            LooperEventSnapshot { tick: 100, note: 62, velocity: 127, layer_id: 0, part_id: 0 },
        ];
        l.restore(&events, 48_000);
        assert_eq!(l.current_layer, 2);
    }

    #[test]
    fn empty_restore_is_noop() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[], 0);
        assert_eq!(l.len(), 0);
        assert_eq!(l.loop_length_ticks, 0);
    }

    #[test]
    fn restore_caps_at_max_events() {
        let mut l = MidiLooper::new(48_000.0);
        let events: Vec<LooperEventSnapshot> = (0..MAX_LOOP_EVENTS + 100)
            .map(|i| snap(i as u32, 60, 127))
            .collect();
        l.restore(&events, u32::MAX / 2);
        assert_eq!(l.len() as usize, MAX_LOOP_EVENTS);
    }

    #[test]
    fn clear_refreshes_atoms_immediately() {
        let mut l = MidiLooper::new(48_000.0);
        l.state = LooperState::Recording;
        l.record_event(60, 100, 0);
        l.tick(); // publishes event_count = 1
        assert_eq!(l.atoms.event_count.load(Ordering::Relaxed), 1);
        l.clear();
        assert_eq!(l.atoms.event_count.load(Ordering::Relaxed), 0,
            "clear must refresh atoms without waiting for a tick");
        assert_eq!(l.atoms.state.load(Ordering::Relaxed), 0);
        assert_eq!(l.atoms.layer_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stop_publishes_idle_state_without_tick() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[snap(0, 60, 127)], 48_000);
        l.toggle_play();
        l.tick();
        assert_eq!(l.atoms.state.load(Ordering::Relaxed), 2, "expected Playing");
        l.toggle_play(); // -> stop()
        assert_eq!(l.atoms.state.load(Ordering::Relaxed), 0,
            "stop must refresh the state atom immediately");
    }

    #[test]
    fn position_atom_walks_and_wraps() {
        // loop of 256 ticks: pos = playback_tick (max 254 — the u8 quant
        // (t*255/len) as u8 never reaches 255; existing behavior).
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[snap(0, 60, 127)], 256);
        l.toggle_play();
        let mut seen_254 = false;
        let mut saw_zero_after_254 = false;
        for _ in 0..600 {
            l.tick();
            let p = l.atoms.position.load(Ordering::Relaxed);
            if p == 254 { seen_254 = true; }
            if seen_254 && p == 0 { saw_zero_after_254 = true; break; }
        }
        assert!(seen_254, "position never reached 254");
        assert!(saw_zero_after_254, "position atom did not wrap back to 0");
    }

}

#[cfg(test)]
mod timing_tests {
    use super::*;

    const SR: f32 = 48_000.0;

    /// Local copy: `snap` lives in the sibling `restore_tests` module and is
    /// not visible from here (looper.rs:593-595).
    fn snap(tick: u32, note: u8, vel: u8) -> LooperEventSnapshot {
        LooperEventSnapshot { tick, note, velocity: vel, layer_id: 0, part_id: 0 }
    }

    /// Looper with an established loop length (bars x 4 beats), in Recording.
    fn loop_of(bpm: f32, bars: u8) -> MidiLooper {
        let mut l = MidiLooper::new(SR);
        l.bpm = bpm;
        l.bars = bars;
        l.start_record(); // Idle -> Recording, loop_length_ticks established
        l
    }

    #[test]
    fn quantize_grids_round_to_nearest() {
        // 120 BPM @ 48k: beat = 24000 samples; quarter 24000, eighth 12000, 16th 6000
        let mut l = MidiLooper::new(SR);
        l.bpm = 120.0;
        l.quantize = Quantize::Eighth;
        assert_eq!(l.quantize_tick(1000), 0, "far below half-grid -> snaps to 0");
        assert_eq!(l.quantize_tick(5000), 0);
        assert_eq!(l.quantize_tick(7000), 12000, "past half-grid -> snaps up");
        l.quantize = Quantize::Sixteenth;
        assert_eq!(l.quantize_tick(4000), 6000);
        l.quantize = Quantize::Quarter;
        assert_eq!(l.quantize_tick(20000), 24000);
        l.quantize = Quantize::Off;
        assert_eq!(l.quantize_tick(12345), 12345, "Off passes through");
    }

    #[test]
    fn quantize_wraps_into_loop_length() {
        // 120 BPM, 1 bar = 96000 ticks; quarter grid 24000.
        // tick 95000: nearest = ((95000+12000)/24000)*24000 = 96000 -> % len -> 0
        let mut l = loop_of(120.0, 1);
        assert_eq!(l.loop_length_ticks, 96_000);
        l.quantize = Quantize::Quarter;
        assert_eq!(l.quantize_tick(95_000), 0);
    }

    #[test]
    fn degenerate_grid_passes_tick_through() {
        // grid == 0 (absurd bpm) must not divide by zero (looper.rs:219)
        let mut l = MidiLooper::new(SR);
        l.bpm = 4_000_000.0; // beat_samples = 0.72 -> grid 0
        l.quantize = Quantize::Sixteenth;
        assert_eq!(l.quantize_tick(777), 777);
    }

    #[test]
    fn record_event_inserts_sorted_and_shifts_cursor() {
        let mut l = loop_of(120.0, 1);
        l.state = LooperState::Recording;
        l.playback_tick = 24_000;
        l.playback_cursor = 1;
        l.record_event(60, 127, 0); // tick 24000 (quantize Off)
        l.record_event(62, 127, 0); // same tick -> inserted before (stable partition_point)
        l.playback_tick = 12_000;
        l.playback_cursor = 2;
        l.record_event(55, 100, 0); // earlier tick -> inserted at 0
        assert_eq!(l.len(), 3);
        assert_eq!(l.events[0].tick, 12_000);
        assert_eq!(l.events[1].tick, 24_000);
        assert_eq!(l.events[2].tick, 24_000);
        assert_eq!(l.playback_cursor, 3, "insertion before the cursor shifts it right");
    }

    #[test]
    fn record_event_caps_at_max_events() {
        let mut l = loop_of(120.0, 1);
        l.state = LooperState::Overdubbing;
        for i in 0..MAX_LOOP_EVENTS + 10 {
            l.playback_tick = (i as u32) % l.loop_length_ticks;
            l.record_event(60, 100, 0);
        }
        assert_eq!(l.len() as usize, MAX_LOOP_EVENTS, "excess events must be dropped");
    }

    #[test]
    fn playback_fires_events_at_exact_ticks_and_wraps() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(10, 60, 127), snap(10, 64, 127), snap(20, 60, 0)], 100);
        l.toggle_play(); // Playing from tick 0
        let quiet = l.tick(); // playback_tick 0: no events at 0
        assert_eq!(quiet[0], (0, 0, 255), "sentinel triple = no event");
        for _ in 1..10 { let _ = l.tick(); }
        let ev = l.tick(); // playback_tick == 10: BOTH events fire
        assert_eq!(ev[0], (60, 127, 0));
        assert_eq!(ev[1], (64, 127, 0));
        for _ in 11..20 { let _ = l.tick(); }
        let off = l.tick(); // playback_tick == 20
        assert_eq!(off[0], (60, 0, 0));
        // note 64 still active -> forced note-off at wrap (tick 99 -> len 100)
        for _ in 21..99 { let _ = l.tick(); }
        let wrap = l.tick();
        assert_eq!(wrap[0], (64, 0, 0), "wrap must force note-off for held notes");
        let restart = l.tick(); // playback_tick wrapped to 0
        assert_eq!(restart[0], (0, 0, 255), "no events at tick 0");
    }

    #[test]
    fn stop_drains_active_note_offs_on_next_tick() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(0, 60, 127)], 1000);
        l.toggle_play();
        let _ = l.tick(); // fires note-on; active_notes[60] = 0
        l.toggle_play();  // Playing -> stop(): needs_note_offs = true
        let drained = l.tick(); // Idle branch drains pending offs
        assert_eq!(drained[0], (60, 0, 0));
        let again = l.tick();
        assert_eq!(again[0], (0, 0, 255), "drain happens exactly once");
    }

    #[test]
    fn overdub_layers_and_undo_removes_only_top_layer() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[snap(0, 60, 127), snap(10, 60, 0)], 1000); // base layer 0
        l.toggle_play();
        l.start_record(); // Playing -> Overdubbing, layer 1
        assert_eq!(l.current_layer, 1);
        l.playback_tick = 5;
        l.record_event(64, 100, 0); // layer 1
        l.stop_record();
        l.undo(); // removes exactly layer 1
        assert_eq!(l.len(), 2, "base layer must survive");
        assert_eq!(l.current_layer, 0);
        let notes: Vec<u8> = (0..l.len() as usize).map(|i| l.events[i].note).collect();
        assert_eq!(notes, vec![60, 60]);
        l.undo(); // layer 0: no-op
        assert_eq!(l.len(), 2);
        assert_eq!(l.current_layer, 0);
    }

    #[test]
    fn undo_mid_pass_repartitions_cursor() {
        // Undo while playing, PAST the removed event's tick: the cursor must
        // be recomputed (partition_point, looper.rs:304-305) so the removed
        // layer never fires again and the remaining base layer still plays.
        // NOTE: restore() leaves Idle; start_record() from Idle would WIPE the
        // loop — toggle_play() first (Idle -> Playing from tick 0,
        // looper.rs:273-277).
        // Pins current behavior: record_event at playback_tick 1500 does NOT
        // fire the event in this pass — the cursor-adjust condition
        // (pos <= playback_cursor, looper.rs:451) is false because the cursor
        // (0) sits behind the insertion point (2), so tick() keeps scanning
        // from tick 100/150 events. undo()'s partition_point is what actually
        // repartitions the cursor past the removed event.
        let mut l = MidiLooper::new(SR);
        // Base layer 0: note 60 on/off early, note 72 AFTER the layer-1
        // event that will be undone — its firing proves the repartition.
        l.restore(&[snap(100, 60, 127), snap(150, 60, 0), snap(2000, 72, 100)], 3000);
        l.toggle_play();               // Idle -> Playing from tick 0
        l.start_record();              // Playing -> Overdubbing, layer 1
        l.playback_tick = 1500;
        l.record_event(70, 100, 0);    // layer 1 @ tick 1500 (quantize Off)
        l.stop_record();               // -> Playing
        for _ in 0..20 { let _ = l.tick(); } // playback_tick 1520; cursor still 0
        l.undo();                      // removes (1500, note 70), recomputes cursor
        assert_eq!(l.len(), 3);
        assert_eq!(l.current_layer, 0);
        // partition_point over [100,150,2000] at playback_tick 1520 -> 2
        assert_eq!(l.playback_cursor, 2, "cursor repartitioned past the removed event");
        // Finish the pass: 3000 - 1520 = 1480 ticks; note 72 @2000 must fire.
        let mut fired_72 = 0;
        for _ in 0..1480 {
            for &(n, v, p) in &l.tick() {
                if p == 255 { break; }
                if n == 72 && v > 0 { fired_72 += 1; }
            }
        }
        assert_eq!(fired_72, 1, "base-layer event after the undo point still fires this pass");
        let mut fired_70 = 0;
        let mut fired_60 = 0;
        for _ in 0..3000 {             // full second pass
            for &(n, v, p) in &l.tick() {
                if p == 255 { break; }
                if v == 0 { continue; } // note-offs carry note numbers too — count attacks only
                match n { 70 => fired_70 += 1, 60 => fired_60 += 1, _ => {} }
            }
        }
        assert_eq!(fired_70, 0, "removed layer must not fire");
        assert_eq!(fired_60, 1, "base layer fires once per pass (positive control)");
    }

    #[test]
    fn first_pass_recording_switches_to_overdub_at_loop_end() {
        let mut l = loop_of(120.0, 1); // loop 96000
        l.record_event(60, 127, 0);
        for _ in 0..95_999 { let _ = l.tick(); }
        assert_eq!(l.state, LooperState::Recording, "first pass does not play back");
        let _ = l.tick(); // playback_tick reaches loop_length -> switch
        assert_eq!(l.state, LooperState::Overdubbing);
        assert_eq!(l.current_layer, 1);
        assert_eq!(l.playback_tick, 0, "position resets for the overdub pass");
        assert_eq!(l.loop_length_ticks, 96_000, "loop length survives the switch");
    }
}

#[cfg(test)]
mod clock_sync_tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn loop_at(bpm: f32, loop_ticks: u32) -> MidiLooper {
        let mut l = MidiLooper::new(SR);
        l.bpm = bpm;
        l.loop_bpm = bpm;
        l.loop_length_ticks = loop_ticks;
        l
    }

    #[test]
    fn sync_updates_bpm_and_beats_per_bar() {
        let mut l = loop_at(120.0, 96_000);
        l.sync_clock(90.0, 3);
        assert_eq!(l.bpm, 90.0);
        assert_eq!(l.beats_per_bar, 3);
        assert_eq!(l.loop_bpm, 90.0);
    }

    #[test]
    fn regrid_halves_loop_at_double_bpm() {
        let mut l = loop_at(120.0, 96_000);
        l.events[0].tick = 24_000;
        l.events[1].tick = 48_000;
        l.len = 2;
        l.playback_tick = 24_000;
        l.sync_clock(240.0, 4);
        assert_eq!(l.loop_length_ticks, 48_000);
        assert_eq!(l.events[0].tick, 12_000);
        assert_eq!(l.events[1].tick, 24_000);
        assert_eq!(l.playback_tick, 12_000);
        assert_eq!(l.loop_bpm, 240.0);
    }

    #[test]
    fn regrid_doubles_loop_at_half_bpm() {
        let mut l = loop_at(120.0, 48_000);
        l.playback_tick = 12_000;
        l.sync_clock(90.0, 4);
        assert_eq!(l.loop_length_ticks, 64_000); // 48_000 * 120/90
        assert_eq!(l.playback_tick, 16_000);
    }

    #[test]
    fn regrid_preserves_position_fraction_mid_playback() {
        let mut l = loop_at(120.0, 96_000);
        l.playback_tick = 24_000; // exactly 1/4
        l.sync_clock(60.0, 4);    // x2 length
        let frac = l.playback_tick as f64 / l.loop_length_ticks as f64;
        assert!((frac - 0.25).abs() < 0.001, "fraction drifted: {frac}");
    }

    #[test]
    fn events_stay_sorted_after_regrid() {
        let mut l = loop_at(120.0, 96_000);
        l.events[0].tick = 10_000;
        l.events[1].tick = 10_001;
        l.events[2].tick = 10_002;
        l.len = 3;
        l.sync_clock(240.0, 4);
        for w in l.events[..3].windows(2) {
            assert!(w[0].tick <= w[1].tick, "sort broken: {} > {}", w[0].tick, w[1].tick);
            assert!(w[1].tick <= l.loop_length_ticks);
        }
    }

    #[test]
    fn restore_sentinel_adopts_bpm_without_regrid() {
        let mut l = MidiLooper::new(SR);
        l.restore(&[LooperEventSnapshot { tick: 24_000, note: 60, velocity: 127, layer_id: 0, part_id: 0 }], 96_000);
        l.sync_clock(90.0, 4);
        assert_eq!(l.loop_length_ticks, 96_000, "sentinel: no regrid after restore");
        assert_eq!(l.events[0].tick, 24_000);
        assert_eq!(l.bpm, 90.0);
    }

    #[test]
    fn same_bpm_sync_is_noop() {
        let mut l = loop_at(120.0, 96_000);
        l.events[0].tick = 24_000;
        l.len = 1;
        l.playback_tick = 48_000;
        let before = (l.loop_length_ticks, l.events[0].tick, l.playback_tick);
        for _ in 0..10 { l.sync_clock(120.0, 4); }
        assert_eq!(before, (l.loop_length_ticks, l.events[0].tick, l.playback_tick));
    }

    #[test]
    fn bar_length_uses_beats_per_bar() {
        let mut l = MidiLooper::new(SR); // bpm 120
        l.beats_per_bar = 3;
        l.bars = 1;
        l.start_record();
        assert_eq!(l.loop_length_ticks, 72_000); // 0.5s * 3 * 48k
        let mut q = MidiLooper::new(SR);
        q.beats_per_bar = 4;
        q.bars = 1;
        q.start_record();
        assert_eq!(q.loop_length_ticks, 96_000);
    }

    #[test]
    fn non_positive_or_nan_bpm_is_ignored() {
        let mut l = loop_at(120.0, 96_000);
        l.sync_clock(0.0, 4);
        l.sync_clock(-10.0, 4);
        l.sync_clock(f32::NAN, 4);
        l.sync_clock(f32::INFINITY, 4);
        assert_eq!(l.bpm, 120.0);
        assert_eq!(l.loop_length_ticks, 96_000);
        assert_eq!(l.beats_per_bar, 4); // not touched by the NaN call
    }

    #[test]
    fn degenerate_regrid_skips_but_adopts_bpm() {
        let mut l = loop_at(120.0, 8);
        l.events[0].tick = 4;
        l.len = 1;
        l.sync_clock(1.0e7, 4); // new_len == 0
        assert_eq!(l.loop_length_ticks, 8);
        assert_eq!(l.events[0].tick, 4);
        assert_eq!(l.bpm, 1.0e7);
    }

    #[test]
    fn quantize_grid_follows_synced_bpm() {
        // beat_samples @60bpm/48k = 48_000; grid 1/16 = 12_000 (was @120: 6_000).
        // tick 17_000: @120 (grid 6_000) -> 18_000. @60 (grid 12_000) -> 12_000.
        let mut l = loop_at(120.0, 192_000);
        l.quantize = Quantize::Sixteenth;
        assert_eq!(l.quantize_tick(17_000), 18_000);
        l.sync_clock(60.0, 4); // regrid x2, grid is now 12_000
        assert_eq!(l.quantize_tick(17_000), 12_000);
    }
}

#[cfg(test)]
mod part_mute_tests {
    use super::*;

    const SR: f32 = 48_000.0;

    fn snap(tick: u32, note: u8, vel: u8, part_id: u8) -> LooperEventSnapshot {
        LooperEventSnapshot { tick, note, velocity: vel, layer_id: 0, part_id }
    }

    /// Fire events from a full pass over a 1000-tick loop, collecting
    /// (note, velocity, part) triples (sentinel dropped).
    fn fire_pass(l: &mut MidiLooper) -> Vec<(u8, u8, u8)> {
        let mut fired = Vec::new();
        for _ in 0..1000 {
            for &(n, v, p) in &l.tick() {
                if p == 255 { break; }
                fired.push((n, v, p));
            }
        }
        fired
    }

    fn two_part_looper() -> MidiLooper {
        let mut l = MidiLooper::new(SR);
        // Part 0: note 60 held; part 1: note 72 held. Sorted by tick —
        // restore() copies the slice 1:1 without re-sorting.
        l.restore(&[
            snap(0, 60, 127, 0),
            snap(0, 72, 127, 1),
            snap(500, 60, 0, 0),
            snap(500, 72, 0, 1),
        ], 1000);
        l.toggle_play(); // Playing from tick 0
        l
    }

    #[test]
    fn part_mute_skips_only_that_parts_events() {
        let mut l = two_part_looper();
        l.set_part_mute(0, true);
        let fired = fire_pass(&mut l);
        let parts: Vec<u8> = fired.iter().filter(|&&(_, v, _)| v > 0).map(|&(_, _, p)| p).collect();
        assert_eq!(parts, vec![1], "muted part 0 must not fire, part 1 must");
        // Unmute: both parts replay again.
        l.set_part_mute(0, false);
        let fired = fire_pass(&mut l);
        let mut saw_p0 = false;
        let mut saw_p1 = false;
        for &(_, v, p) in &fired {
            if v > 0 {
                match p { 0 => saw_p0 = true, 1 => saw_p1 = true, _ => {} }
            }
        }
        assert!(saw_p0 && saw_p1, "unmute must restore both parts: {fired:?}");
    }

    #[test]
    fn set_part_mute_flushes_sounding_notes_of_that_part() {
        let mut l = two_part_looper();
        // advance 100 ticks: both tick-0 note-ons fired and are still held
        // (their offs sit at tick 500, not reached yet)
        for _ in 0..100 { let _ = l.tick(); }
        let offs = l.set_part_mute(0, true);
        assert_eq!(offs[0], (60, 0, 0), "held note of muted part must be flushed");
        assert!(offs[1..].iter().all(|&t| t == (0, 0, 255)), "only part 0's note flushed: {offs:?}");
        assert_eq!(l.active_notes[60], 255, "part 0 note cleared");
        assert_eq!(l.active_notes[72], 1, "part 1 note untouched");
        // Muting the same part again must not re-flush (notes already gone).
        let again = l.set_part_mute(0, true);
        assert!(again.iter().all(|&t| t == (0, 0, 255)));
    }

    #[test]
    fn unmute_does_not_retrigger_skipped_pass() {
        // Events skipped while muted must not fire retroactively in the same
        // pass — the cursor advanced past them.
        let mut l = two_part_looper();
        let mut fired_60 = 0;
        // advance 100 ticks (past the tick-0 events) BEFORE muting
        for _ in 0..100 { for &(n, v, _) in &l.tick() { if v > 0 && n == 60 { fired_60 += 1; } } }
        assert_eq!(fired_60, 1);
        l.set_part_mute(0, true);
        l.set_part_mute(0, false); // mute + unmute within the same pass
        // rest of the pass: note 60's events are behind the cursor; nothing
        // of it may fire again until the next wrap.
        let mut extra_60 = 0;
        for _ in 0..900 { for &(n, v, _) in &l.tick() { if v > 0 && n == 60 { extra_60 += 1; } } }
        assert_eq!(extra_60, 0, "mute/unmute mid-pass must not retrigger");
    }

    #[test]
    fn clear_resets_part_mutes() {
        let mut l = two_part_looper();
        l.set_part_mute(1, true);
        l.clear();
        assert!(l.part_mutes.iter().all(|&m| !m), "clear must reset part mutes");
        l.restore(&[snap(0, 72, 127, 1)], 1000);
        l.toggle_play();
        let fired = fire_pass(&mut l);
        assert!(fired.contains(&(72, 127, 1)), "after clear the part must be audible again");
    }

    #[test]
    fn out_of_range_part_mute_is_a_safe_noop() {
        let mut l = two_part_looper();
        l.set_part_mute(200, true); // must not panic
        let fired = fire_pass(&mut l);
        assert_eq!(fired.iter().filter(|&&(_, v, _)| v > 0).count(), 2,
            "out-of-range mute must not silence anything");
    }
}

