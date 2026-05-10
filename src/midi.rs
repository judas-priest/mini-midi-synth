/// MIDI input handling via midir + wmidi.

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

/// Connect to a MIDI input port by index using a shared producer.
pub fn connect(
    port_index: usize,
    tx: SharedMidiTx,
    note_state: NoteState,
    pad_state: PadState,
) -> Result<MidiInputConnection<()>> {
    let midi_in = MidiInput::new("mini_midi_synth")?;
    let ports = midi_in.ports();
    let port = ports
        .get(port_index)
        .context("Invalid MIDI port index")?;

    let conn = midi_in
        .connect(
            port,
            "mini_midi_synth_in",
            move |_timestamp, data, _| {
                if let Ok(msg) = MidiMessage::try_from(data) {
                    let event = match msg {
                        MidiMessage::NoteOn(ch, note, velocity) => {
                            let channel = ch.index();
                            let n = u8::from(note);
                            let v = u8::from(velocity);
                            let state = if channel == 9 { &pad_state } else { &note_state };
                            if v == 0 {
                                state[n as usize].store(0, Ordering::Relaxed);
                            } else {
                                state[n as usize].store(v, Ordering::Relaxed);
                            }
                            Some(MidiEvent::NoteOn {
                                channel,
                                note: n,
                                velocity: v,
                            })
                        }
                        MidiMessage::NoteOff(ch, note, _) => {
                            let channel = ch.index();
                            let n = u8::from(note);
                            let state = if channel == 9 { &pad_state } else { &note_state };
                            state[n as usize].store(0, Ordering::Relaxed);
                            Some(MidiEvent::NoteOff { channel, note: n })
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
                            } else if cc_num >= 1 && cc_num <= 119
                                && cc_num != 0 && cc_num != 32
                            {
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
                            // SMK-37 Pro: F0 35 59 10 00 [7F|00] F7
                            // 7F = press, 00 = release
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
                    };
                    if let Some(ev) = event {
                        // try_lock: never block the MIDI RT thread.
                        // Lock is only contended during reconnection (GUI thread), so
                        // dropping a single event during reconnect is acceptable.
                        if let Ok(mut tx) = tx.try_lock() {
                            let _ = tx.push(ev);
                        }
                    }
                }
            },
            (),
        )
        .map_err(|e| anyhow::anyhow!("MIDI connect failed: {e}"))?;

    Ok(conn)
}
