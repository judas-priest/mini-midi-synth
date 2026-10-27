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
            } else if cc_num == 0 || cc_num == 32 {
                // Bank select. Nothing downstream uses it and letting it
                // through would collide with mapped parameters.
                None
            } else {
                // Everything else, including channel mode messages 120-127.
                // CC 120 (All Sound Off) and CC 123 (All Notes Off) are the
                // panic button on hardware controllers and must reach the
                // engine — the old 1..=119 filter swallowed them.
                Some(MidiEvent::ControlChange { channel, cc: cc_num, value: v })
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
    let Some((event, consumed)) = parse_midi_message(data) else {
        log::debug!("[midi-in] unparsed bytes: {:02X?}", data);
        return 0;
    };
    if let MidiEvent::ControlChange { cc, value, .. } = &event {
        log::info!("[midi-in] CC{} = {}", cc, value);
    }

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

    // A lost event is a stuck note with no trace anywhere, so count both ways
    // of losing one: a producer held by another thread and a full ring.
    match tx.try_lock() {
        Ok(mut tx) => {
            if tx.push(event).is_err() {
                crate::metrics::METRICS.note_midi_drop();
            }
        }
        Err(_) => crate::metrics::METRICS.note_midi_drop(),
    }

    consumed
}

/// Rejects Standard MIDI Files whose header would make the parser panic.
///
/// `midly` 0.5.3 panics with "attempt to negate with overflow" on an SMPTE
/// timing field whose frame rate is not one of the four legal values — found by
/// fuzzing `import_midi_drums`. Since the release profile is `panic = "abort"`,
/// `catch_unwind` cannot save us: a user picking a malformed .mid would kill
/// the process outright. So the header is checked before `midly` sees it.
///
/// Returns `Err` with a human-readable reason, `Ok(())` when the file is at
/// least structurally plausible.
pub fn check_smf_header(data: &[u8]) -> Result<(), &'static str> {
    if data.len() < 14 {
        return Err("file is too short to be a MIDI file");
    }
    if &data[0..4] != b"MThd" {
        return Err("not a MIDI file (missing MThd header)");
    }

    // The MThd chunk length is 6 in every real file. A different value makes
    // midly read the timing field from somewhere else and panic on it — the
    // second crash the fuzzer found, with a declared length of 11.
    if u32::from_be_bytes([data[4], data[5], data[6], data[7]]) != 6 {
        return Err("MIDI header chunk has a non-standard length");
    }

    // Bytes 12..14 are the division field. Bit 15 clear means ticks per quarter
    // note; set means SMPTE, where the high byte is the negated frame rate.
    let division = i16::from_be_bytes([data[12], data[13]]);
    if division >= 0 {
        if division == 0 {
            return Err("MIDI file declares zero ticks per beat");
        }
        return Ok(());
    }

    let fps = data[12] as i8;
    match fps {
        -24 | -25 | -29 | -30 => {
            if data[13] == 0 {
                Err("MIDI file declares zero ticks per SMPTE frame")
            } else {
                Ok(())
            }
        }
        _ => Err("MIDI file declares an invalid SMPTE frame rate"),
    }
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
