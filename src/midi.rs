/// MIDI input handling via midir + wmidi.

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
                        MidiMessage::NoteOn(_, note, velocity) => {
                            let n = u8::from(note);
                            let v = u8::from(velocity);
                            if v == 0 {
                                note_state[n as usize].store(0, Ordering::Relaxed);
                            } else {
                                note_state[n as usize].store(v, Ordering::Relaxed);
                            }
                            Some(MidiEvent::NoteOn {
                                note: n,
                                velocity: v,
                            })
                        }
                        MidiMessage::NoteOff(_, note, _) => {
                            let n = u8::from(note);
                            note_state[n as usize].store(0, Ordering::Relaxed);
                            Some(MidiEvent::NoteOff { note: n })
                        }
                        _ => None,
                    };
                    if let Some(ev) = event {
                        if let Ok(mut tx) = tx.lock() {
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
