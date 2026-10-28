#![no_main]
//! Surge .wt wavetable files are downloaded by users and dropped into the
//! wavetable directory, so the parser sees bytes nobody vetted. Returning None
//! on garbage is correct; panicking or reading out of bounds is not.

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Some((samples, frames, frame_size)) = mini_midi_synth::preset::parse_wt(data) {
        // A successful parse must describe a self-consistent wavetable, or the
        // oscillator will index past the end of the sample buffer at runtime.
        assert_eq!(
            samples.len(),
            frames * frame_size,
            "parse_wt reported {frames} frames of {frame_size} but returned {} samples",
            samples.len()
        );
    }
});
