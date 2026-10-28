#![no_main]
//! Drum pattern import parses arbitrary .mid files chosen by the user.

use libfuzzer_sys::fuzz_target;
use std::io::Write;

fuzz_target!(|data: &[u8]| {
    // import_midi_drums takes a path, so the input has to reach the filesystem.
    // One reused temp file keeps this fast enough to be worth running.
    let path = std::env::temp_dir().join("mms-fuzz-drums.mid");
    if std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(data))
        .is_err()
    {
        return;
    }

    if let Ok((patterns, bpm)) = mini_midi_synth::preset::import_midi_drums(&path) {
        assert!(patterns.len() <= 8, "import produced {} patterns, cap is 8", patterns.len());
        assert!(bpm.is_finite() && bpm > 0.0, "import produced bpm {bpm}");
    }
});
