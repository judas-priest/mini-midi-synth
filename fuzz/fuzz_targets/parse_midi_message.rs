#![no_main]
//! Raw MIDI bytes arrive from USB, Bluetooth and JNI. Anything that panics
//! here takes down the MIDI callback thread, or the audio thread on Android
//! where AMidi is polled inside the callback.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Mirrors how callers loop over a packet: parse, advance, repeat.
    let mut offset = 0;
    while offset < data.len() {
        match mini_midi_synth::midi::parse_midi_message(&data[offset..]) {
            Some((_event, consumed)) => {
                // A zero-length consume would spin the caller's loop forever.
                assert!(consumed > 0, "parse_midi_message consumed 0 bytes");
                offset += consumed;
            }
            None => break,
        }
    }
});
