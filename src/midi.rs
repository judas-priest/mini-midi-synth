//! MIDI input handling via midir + wmidi.

const PITCH_BEND_CENTER: f32 = 8192.0;
const MIDI_MAX_VAL: f32 = 127.0;

use anyhow::{Context, Result};
use midir::{MidiInput, MidiInputConnection};
use rtrb::Producer;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use wmidi::MidiMessage;

use crate::synth::MidiEvent;

/// Shared note state: 128 notes, 0 = off, >0 = velocity.
pub type NoteState = Arc<[AtomicU8; 128]>;

pub fn new_note_state() -> NoteState {
    Arc::new(std::array::from_fn(|_| AtomicU8::new(0)))
}

/// Separate pad state for channel 10 (index 9) display.
pub type PadState = Arc<[AtomicU8; 128]>;

pub fn new_pad_state() -> PadState {
    Arc::new(std::array::from_fn(|_| AtomicU8::new(0)))
}

/// Shared MIDI producer — allows reconnecting MIDI without restarting audio.
pub type SharedMidiTx = Arc<Mutex<Producer<MidiEvent>>>;

/// Parse a single MIDI message from raw bytes.
/// Returns (MidiEvent, bytes_consumed) or None on parse failure.
/// Shared by parse_and_push (ring buffer path) and audio callback (direct path).
pub fn parse_midi_message(data: &[u8]) -> Option<(MidiEvent, usize)> {
    let msg = MidiMessage::try_from(data).ok()?;
    let consumed = msg.bytes_size();

    let event = match msg {
        MidiMessage::NoteOn(ch, note, velocity) => {
            let channel = ch.index();
            let n = u8::from(note);
            let v = u8::from(velocity);
            Some(MidiEvent::NoteOn { channel, note: n, velocity: v })
        }
        MidiMessage::NoteOff(ch, note, _) => {
            Some(MidiEvent::NoteOff { channel: ch.index(), note: u8::from(note) })
        }
        MidiMessage::PitchBendChange(ch, bend) => {
            let raw = u16::from(bend) as f32;
            let normalized = (raw - PITCH_BEND_CENTER) / PITCH_BEND_CENTER;
            Some(MidiEvent::PitchBend { channel: ch.index(), value: normalized })
        }
        MidiMessage::ControlChange(ch, cc, val) => {
            let channel = ch.index();
            let cc_num = u8::from(cc);
            let v = u8::from(val);
            if cc_num == 1 {
                Some(MidiEvent::ModWheel { channel, value: v as f32 / MIDI_MAX_VAL })
            } else if (1..=119).contains(&cc_num) && cc_num != 32 {
                Some(MidiEvent::ControlChange { channel, cc: cc_num, value: v })
            } else {
                None
            }
        }
        MidiMessage::ProgramChange(ch, program) => {
            Some(MidiEvent::ProgramChange { channel: ch.index(), program: u8::from(program) })
        }
        MidiMessage::ChannelPressure(ch, pressure) => {
            Some(MidiEvent::Aftertouch { channel: ch.index(), value: u8::from(pressure) as f32 / MIDI_MAX_VAL })
        }
        MidiMessage::PolyphonicKeyPressure(ch, note, pressure) => {
            Some(MidiEvent::PolyAftertouch {
                channel: ch.index(),
                note: u8::from(note),
                pressure: u8::from(pressure) as f32 / MIDI_MAX_VAL,
            })
        }
        MidiMessage::SysEx(payload) => {
            if payload.len() == 5
                && u8::from(payload[0]) == 0x35
                && u8::from(payload[1]) == 0x59
                && u8::from(payload[2]) == 0x10
            {
                let pressed = u8::from(payload[4]) == 0x7F;
                Some(MidiEvent::Navigate { pressed })
            } else {
                None
            }
        }
        _ => None,
    }?;

    Some((event, consumed))
}

/// Parse a single MIDI message from `data`, update note/pad state, and push
/// the resulting event into the ring buffer.  Returns the number of bytes
/// consumed, or 0 on parse failure.
pub fn parse_and_push(
    data: &[u8],
    tx: &SharedMidiTx,
    note_state: &NoteState,
    pad_state: &PadState,
) -> usize {
    let Some((event, consumed)) = parse_midi_message(data) else { return 0 };

    // Update GUI note display
    match &event {
        MidiEvent::NoteOn { channel, note, velocity } => {
            let state = if *channel == 9 { pad_state } else { note_state };
            if *velocity == 0 {
                state[*note as usize].store(0, Ordering::Relaxed);
            } else {
                state[*note as usize].store(*velocity, Ordering::Relaxed);
            }
        }
        MidiEvent::NoteOff { channel, note } => {
            let state = if *channel == 9 { pad_state } else { note_state };
            state[*note as usize].store(0, Ordering::Relaxed);
        }
        _ => {}
    }

    if let Ok(mut tx) = tx.try_lock() {
        let _ = tx.push(event);
    }

    consumed
}

/// List available MIDI input ports.
pub fn list_ports() -> Result<Vec<String>> {
    let midi_in = MidiInput::new("mini_midi_synth_list")?;
    let ports = midi_in.ports();
    let names: Vec<String> = ports
        .iter()
        .filter_map(|p| midi_in.port_name(p).ok())
        .collect();
    Ok(names)
}

/// Connect to a MIDI input port by index or name using a shared producer.
/// Tries by index first; if the name at that index doesn't match, searches by name.
pub fn connect(
    port_index: usize,
    tx: SharedMidiTx,
    note_state: NoteState,
    pad_state: PadState,
) -> Result<MidiInputConnection<()>> {
    connect_by_name(None, port_index, tx, note_state, pad_state)
}

/// Connect to a MIDI input port by name (with index hint).
pub fn connect_by_name(
    port_name: Option<&str>,
    port_index_hint: usize,
    tx: SharedMidiTx,
    note_state: NoteState,
    pad_state: PadState,
) -> Result<MidiInputConnection<()>> {
    let midi_in = MidiInput::new("mini_midi_synth")?;
    let ports = midi_in.ports();

    // Find port: try by name first, fall back to index
    let port = if let Some(name) = port_name {
        // Try index hint first (fast path)
        let by_hint = ports.get(port_index_hint).and_then(|p| {
            midi_in.port_name(p).ok().filter(|n| n == name).map(|_| p)
        });
        if let Some(p) = by_hint {
            p
        } else {
            // Search all ports by name
            ports.iter()
                .find(|p| midi_in.port_name(p).ok().as_deref() == Some(name))
                .context(format!("MIDI port not found: {name}"))?
        }
    } else {
        ports.get(port_index_hint).context("Invalid MIDI port index")?
    };

    let port_display = midi_in.port_name(port).unwrap_or_default();
    log::info!("[midi] Connecting to port: {port_display}");

    let conn = midi_in
        .connect(
            port,
            "mini_midi_synth_in",
            move |_timestamp, data, _| {
                parse_and_push(data, &tx, &note_state, &pad_state);
            },
            (),
        )
        .map_err(|e| anyhow::anyhow!("MIDI connect failed: {e}"))?;

    Ok(conn)
}
