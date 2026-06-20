//! MIDI file player for the sequencer tab.
//! Parses Standard MIDI Files (.mid) and fires events on the audio thread.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Timestamped event data in the merged event stream.
#[derive(Clone, Debug)]
pub enum SeqEventData {
    NoteOn { note: u8, velocity: u8 },
    NoteOff { note: u8 },
    TempoChange { us_per_beat: u32 },
    ProgramChange { program: u8 },
}

/// One event in the merged (all-tracks) event stream.
pub struct MidiSeqEvent {
    pub tick: u64,
    pub channel: u8,
    pub data: SeqEventData,
}

/// Parsed MIDI file, ready for playback.
pub struct MidiSeqData {
    pub ticks_per_beat: u64,
    /// End tick (last event tick + a short tail).
    pub total_ticks: u64,
    /// All events merged and sorted by tick.
    pub events: Vec<MidiSeqEvent>,
    /// Track names (from MIDI metadata, one per track in the .mid file).
    pub track_names: Vec<String>,
    /// First MIDI channel used by each track (0-indexed).
    pub track_channels: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Instrument assignment
// ---------------------------------------------------------------------------

/// How a track's notes are played.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum TrackInstrument {
    /// Use the DSP synth part 0 voice pool.
    #[default]
    DspLayer0,
    /// Use the SF2 keys sampler with this GM program.
    Sf2 { program: u8 },
    /// Route to the drum engine (DSP or SF2 drums depending on current mode).
    Drums,
}

// ---------------------------------------------------------------------------
// Track state (owned by the audio-thread MidiPlayer)
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[allow(dead_code)]
pub struct TrackState {
    pub name: String,
    pub channel: u8,
    pub instrument: TrackInstrument,
    pub muted: bool,
}

// ---------------------------------------------------------------------------
// MidiPlayer — runs entirely on the audio thread
// ---------------------------------------------------------------------------

pub struct MidiPlayer {
    pub data: Option<Box<MidiSeqData>>,
    pub tracks: Vec<TrackState>,
    pub playing: bool,
    pub looping: bool,
    /// Override tempo (BPM). None = use embedded file tempo.
    pub bpm_override: Option<f32>,
    /// Current file tempo in microseconds per beat (updated by TempoChange events).
    pub us_per_beat: u32,
    /// Fractional tick position.
    tick_pos: f64,
    /// Index of the next event to fire.
    event_idx: usize,
    /// Shared atom: 0 = stopped, 1 = playing (read by GUI).
    pub play_atom: Arc<AtomicU8>,
    /// Shared atom: playback position 0–1000 (per-mille of total ticks).
    pub position_atom: Arc<AtomicU32>,
}

impl MidiPlayer {
    pub fn new() -> Self {
        Self {
            data: None,
            tracks: Vec::new(),
            playing: false,
            looping: true,
            bpm_override: None,
            us_per_beat: 500_000,
            tick_pos: 0.0,
            event_idx: 0,
            play_atom: Arc::new(AtomicU8::new(0)),
            position_atom: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Load new MIDI data and reset playback to the beginning.
    /// Takes `Box<MidiSeqData>` to avoid an extra heap alloc/dealloc on the audio thread.
    pub fn load(&mut self, data: Box<MidiSeqData>) {
        let n = data.track_names.len();
        self.tracks = (0..n).map(|i| {
            let ch = data.track_channels.get(i).copied().unwrap_or(0);
            TrackState {
                name: data.track_names[i].clone(),
                channel: ch,
                instrument: if ch == 9 { TrackInstrument::Drums } else { TrackInstrument::DspLayer0 },
                muted: false,
            }
        }).collect();
        self.data = Some(data); // reuse the Box, no extra alloc
        self.reset_position();
    }

    /// Seek to the beginning and reset file tempo.
    pub fn reset_position(&mut self) {
        self.tick_pos = 0.0;
        self.event_idx = 0;
        self.us_per_beat = 500_000;
        self.position_atom.store(0, Ordering::Relaxed);
    }

    /// Stop playback and seek to beginning.
    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.playing = false;
        self.play_atom.store(0, Ordering::Relaxed);
        self.reset_position();
    }

    /// Return the instrument assigned to a MIDI channel.
    pub fn instrument_for_channel(&self, channel: u8) -> TrackInstrument {
        for t in &self.tracks {
            if t.channel == channel { return t.instrument; }
        }
        if channel == 9 { TrackInstrument::Drums } else { TrackInstrument::DspLayer0 }
    }

    /// Return true if a channel's track is muted.
    fn is_muted(&self, channel: u8) -> bool {
        self.tracks.iter().any(|t| t.channel == channel && t.muted)
    }

    /// Process one audio block. Fires due events into `out` (cleared first).
    /// Call this once per `tick_block` call, before the sample render loop.
    pub fn process_block(
        &mut self,
        n_samples: usize,
        sample_rate: f32,
        out: &mut Vec<(u8, SeqEventData)>,
    ) {
        out.clear();
        if !self.playing { return; }
        let data = match &self.data { Some(d) => d, None => return };
        if data.events.is_empty() { return; }

        let tpb = data.ticks_per_beat as f64;
        let total = data.total_ticks as f64;

        // Effective us-per-beat
        let us = if let Some(bpm) = self.bpm_override {
            60_000_000.0 / bpm as f64
        } else {
            self.us_per_beat as f64
        };

        // Ticks to advance this block
        let ticks_per_sample = tpb * 1_000_000.0 / (us * sample_rate as f64);
        let new_tick = self.tick_pos + n_samples as f64 * ticks_per_sample;

        // Fire all events whose tick falls within [tick_pos, new_tick)
        while self.event_idx < data.events.len() {
            let evt_tick = data.events[self.event_idx].tick as f64;
            if evt_tick >= new_tick { break; }

            let ch = data.events[self.event_idx].channel;
            match &data.events[self.event_idx].data {
                SeqEventData::TempoChange { us_per_beat } => {
                    if self.bpm_override.is_none() {
                        self.us_per_beat = *us_per_beat;
                    }
                }
                d => {
                    if !self.is_muted(ch) {
                        out.push((ch, d.clone()));
                    }
                }
            }
            self.event_idx += 1;
        }

        self.tick_pos = new_tick;

        // End-of-file handling
        if self.event_idx >= data.events.len() || self.tick_pos >= total {
            if self.looping {
                self.reset_position();
            } else {
                self.playing = false;
                self.play_atom.store(0, Ordering::Relaxed);
            }
        }

        // Update GUI position atom
        if total > 0.0 {
            let pos = ((self.tick_pos / total) * 1000.0).clamp(0.0, 1000.0) as u32;
            self.position_atom.store(pos, Ordering::Relaxed);
        }
    }
}

// ---------------------------------------------------------------------------
// MIDI file parser
// ---------------------------------------------------------------------------

/// Parse raw Standard MIDI File bytes into a `MidiSeqData`.
pub fn parse_midi(raw: &[u8]) -> Result<MidiSeqData, String> {
    use midly::{Smf, TrackEventKind, MidiMessage};

    let smf = Smf::parse(raw).map_err(|e| format!("MIDI parse error: {e}"))?;

    let ticks_per_beat = match smf.header.timing {
        midly::Timing::Metrical(t) => t.as_int() as u64,
        midly::Timing::Timecode(fps, sub) => {
            // Approximate: frames-per-second × sub-frames = ticks-per-second
            // Treat as if BPM=60 → us_per_beat=1_000_000 → ticks_per_beat=fps*sub
            (fps.as_f32() * sub as f32) as u64
        }
    };

    let mut all_events: Vec<MidiSeqEvent> = Vec::new();
    let mut track_names: Vec<String> = Vec::new();
    let mut track_channels: Vec<u8> = Vec::new();

    for (tidx, track) in smf.tracks.iter().enumerate() {
        let mut abs_tick: u64 = 0;
        let mut tname = format!("Track {}", tidx + 1);
        let mut tchan: u8 = (tidx % 16) as u8;
        let mut first_chan = false;

        for event in track {
            abs_tick = abs_tick.saturating_add(event.delta.as_int() as u64);

            match event.kind {
                TrackEventKind::Midi { channel, message } => {
                    let ch = channel.as_int();
                    if !first_chan { tchan = ch; first_chan = true; }

                    let d: Option<SeqEventData> = match message {
                        MidiMessage::NoteOn { key, vel } => {
                            if vel.as_int() > 0 {
                                Some(SeqEventData::NoteOn { note: key.as_int(), velocity: vel.as_int() })
                            } else {
                                Some(SeqEventData::NoteOff { note: key.as_int() })
                            }
                        }
                        MidiMessage::NoteOff { key, .. } =>
                            Some(SeqEventData::NoteOff { note: key.as_int() }),
                        MidiMessage::ProgramChange { program } =>
                            Some(SeqEventData::ProgramChange { program: program.as_int() }),
                        _ => None,
                    };

                    if let Some(d) = d {
                        all_events.push(MidiSeqEvent { tick: abs_tick, channel: ch, data: d });
                    }
                }

                TrackEventKind::Meta(meta) => match meta {
                    midly::MetaMessage::Tempo(t) => {
                        all_events.push(MidiSeqEvent {
                            tick: abs_tick,
                            channel: 0,
                            data: SeqEventData::TempoChange { us_per_beat: t.as_int() },
                        });
                    }
                    midly::MetaMessage::TrackName(n) => {
                        let s = String::from_utf8_lossy(n).trim().to_string();
                        if !s.is_empty() { tname = s; }
                    }
                    midly::MetaMessage::InstrumentName(n) => {
                        let s = String::from_utf8_lossy(n).trim().to_string();
                        if !s.is_empty() && tname.starts_with("Track ") { tname = s; }
                    }
                    _ => {}
                },

                _ => {}
            }
        }

        track_names.push(tname);
        track_channels.push(tchan);
    }

    // Sort by tick (stable so same-tick events keep their original order)
    all_events.sort_by_key(|e| e.tick);

    // Add two beats of silence at the end so the last notes can decay
    let total_ticks = all_events.last()
        .map(|e| e.tick + ticks_per_beat * 2)
        .unwrap_or(ticks_per_beat * 4);

    Ok(MidiSeqData {
        ticks_per_beat,
        total_ticks,
        events: all_events,
        track_names,
        track_channels,
    })
}
