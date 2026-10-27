//! SF2 sampler engine wrapping rustysynth.
//! Separate synthesizers for keys (ch0/ch1) and drums (ch9),
//! allowing different SF2 files. Arc<SoundFont> deduplicates when same file.

use std::sync::Arc;
use rustysynth::{SoundFont, Synthesizer, SynthesizerSettings};

const MAX_BLOCK_SIZE: usize = 64;
const DEFAULT_BLOCK_SIZE: usize = 8;

/// GM program names (General MIDI Level 1).
pub const GM_PROGRAM_NAMES: [&str; 128] = [
    // Piano (0-7)
    "Acoustic Grand Piano", "Bright Acoustic Piano", "Electric Grand Piano", "Honky-tonk Piano",
    "Electric Piano 1", "Electric Piano 2", "Harpsichord", "Clavinet",
    // Chromatic Percussion (8-15)
    "Celesta", "Glockenspiel", "Music Box", "Vibraphone",
    "Marimba", "Xylophone", "Tubular Bells", "Dulcimer",
    // Organ (16-23)
    "Drawbar Organ", "Percussive Organ", "Rock Organ", "Church Organ",
    "Reed Organ", "Accordion", "Harmonica", "Tango Accordion",
    // Guitar (24-31)
    "Acoustic Guitar (nylon)", "Acoustic Guitar (steel)", "Electric Guitar (jazz)", "Electric Guitar (clean)",
    "Electric Guitar (muted)", "Overdriven Guitar", "Distortion Guitar", "Guitar Harmonics",
    // Bass (32-39)
    "Acoustic Bass", "Electric Bass (finger)", "Electric Bass (pick)", "Fretless Bass",
    "Slap Bass 1", "Slap Bass 2", "Synth Bass 1", "Synth Bass 2",
    // Strings (40-47)
    "Violin", "Viola", "Cello", "Contrabass",
    "Tremolo Strings", "Pizzicato Strings", "Orchestral Harp", "Timpani",
    // Ensemble (48-55)
    "String Ensemble 1", "String Ensemble 2", "Synth Strings 1", "Synth Strings 2",
    "Choir Aahs", "Voice Oohs", "Synth Choir", "Orchestra Hit",
    // Brass (56-63)
    "Trumpet", "Trombone", "Tuba", "Muted Trumpet",
    "French Horn", "Brass Section", "Synth Brass 1", "Synth Brass 2",
    // Reed (64-71)
    "Soprano Sax", "Alto Sax", "Tenor Sax", "Baritone Sax",
    "Oboe", "English Horn", "Bassoon", "Clarinet",
    // Pipe (72-79)
    "Piccolo", "Flute", "Recorder", "Pan Flute",
    "Blown Bottle", "Shakuhachi", "Whistle", "Ocarina",
    // Synth Lead (80-87)
    "Lead 1 (square)", "Lead 2 (sawtooth)", "Lead 3 (calliope)", "Lead 4 (chiff)",
    "Lead 5 (charang)", "Lead 6 (voice)", "Lead 7 (fifths)", "Lead 8 (bass+lead)",
    // Synth Pad (88-95)
    "Pad 1 (new age)", "Pad 2 (warm)", "Pad 3 (polysynth)", "Pad 4 (choir)",
    "Pad 5 (bowed)", "Pad 6 (metallic)", "Pad 7 (halo)", "Pad 8 (sweep)",
    // Synth Effects (96-103)
    "FX 1 (rain)", "FX 2 (soundtrack)", "FX 3 (crystal)", "FX 4 (atmosphere)",
    "FX 5 (brightness)", "FX 6 (goblins)", "FX 7 (echoes)", "FX 8 (sci-fi)",
    // Ethnic (104-111)
    "Sitar", "Banjo", "Shamisen", "Koto",
    "Kalimba", "Bagpipe", "Fiddle", "Shanai",
    // Percussive (112-119)
    "Tinkle Bell", "Agogo", "Steel Drums", "Woodblock",
    "Taiko Drum", "Melodic Tom", "Synth Drum", "Reverse Cymbal",
    // Sound Effects (120-127)
    "Guitar Fret Noise", "Breath Noise", "Seashore", "Bird Tweet",
    "Telephone Ring", "Helicopter", "Applause", "Gunshot",
];

/// What a soundfont swap displaced. Must be dropped off the audio thread.
#[derive(Default)]
pub struct Displaced {
    pub soundfont: Option<Arc<SoundFont>>,
    pub synth: Option<Synthesizer>,
}

impl Displaced {
    /// True when there is actually something to free — lets callers skip the
    /// trash queue for no-op unloads.
    pub fn is_empty(&self) -> bool {
        self.soundfont.is_none() && self.synth.is_none()
    }
}

/// Internal sub-synth: one rustysynth Synthesizer + its render buffers.
struct SubSynth {
    soundfont: Option<Arc<SoundFont>>,
    synth: Option<Synthesizer>,
    buf_left: [f32; MAX_BLOCK_SIZE],
    buf_right: [f32; MAX_BLOCK_SIZE],
    buf_pos: usize,
}

impl SubSynth {
    fn new() -> Self {
        Self {
            soundfont: None,
            synth: None,
            buf_left: [0.0; MAX_BLOCK_SIZE],
            buf_right: [0.0; MAX_BLOCK_SIZE],
            buf_pos: DEFAULT_BLOCK_SIZE,
        }
    }

    /// Swaps in a new soundfont and returns whatever it displaced.
    ///
    /// The caller must move the returned `Displaced` off the audio thread —
    /// the last `Arc` to a soundfont can be tens of megabytes, and dropping a
    /// `Synthesizer` frees its voice pool. See `crate::trash`.
    ///
    /// Note: `Synthesizer::new` still *allocates* here, on the audio thread.
    /// Moving construction to the GUI thread is tracked separately; this
    /// change removes the frees, which are the larger and more frequent cost.
    #[must_use = "the displaced soundfont and synth must be freed off the audio thread"]
    fn rebuild(&mut self, sf: Option<Arc<SoundFont>>, sample_rate: i32, block_size: usize) -> Displaced {
        let displaced = Displaced { soundfont: self.soundfont.take(), synth: self.synth.take() };

        let sf = match sf {
            Some(s) => s,
            None => return displaced,
        };
        let mut settings = SynthesizerSettings::new(sample_rate);
        settings.block_size = block_size;
        settings.enable_reverb_and_chorus = false;
        match Synthesizer::new(&sf, &settings) {
            Ok(synth) => {
                self.synth = Some(synth);
                self.soundfont = Some(sf);
                self.buf_pos = block_size;
            }
            Err(_) => {
                self.synth = None;
                self.soundfont = None;
            }
        }
        displaced
    }

    #[allow(dead_code)]
    fn is_loaded(&self) -> bool {
        self.synth.is_some()
    }

    fn midi(&mut self, ch: i32, cmd: i32, d1: i32, d2: i32) {
        if let Some(synth) = &mut self.synth {
            synth.process_midi_message(ch, cmd, d1, d2);
        }
    }

    /// Render and discard `count` samples to advance internal state.
    fn skip_samples(&mut self, count: usize, block_size: usize) {
        let synth = match &mut self.synth {
            Some(s) => s,
            None => return,
        };
        let bs = block_size;
        let mut remaining = count;
        // First consume any remaining samples in the current buffer
        let buffered = bs.saturating_sub(self.buf_pos);
        if buffered > 0 {
            let skip = remaining.min(buffered);
            self.buf_pos += skip;
            remaining -= skip;
        }
        // Render and discard full blocks
        while remaining >= bs {
            synth.render(&mut self.buf_left[..bs], &mut self.buf_right[..bs]);
            remaining -= bs;
        }
        // Render one more block and position within it
        if remaining > 0 {
            synth.render(&mut self.buf_left[..bs], &mut self.buf_right[..bs]);
            self.buf_pos = remaining;
        }
    }

    #[inline]
    fn tick(&mut self, block_size: usize) -> (f32, f32) {
        let synth = match &mut self.synth {
            Some(s) => s,
            None => return (0.0, 0.0),
        };
        let bs = block_size;
        if self.buf_pos >= bs {
            synth.render(&mut self.buf_left[..bs], &mut self.buf_right[..bs]);
            self.buf_pos = 0;
        }
        let l = self.buf_left[self.buf_pos];
        let r = self.buf_right[self.buf_pos];
        self.buf_pos += 1;
        (l, r)
    }
}

pub struct SamplerEngine {
    keys: SubSynth,
    drums: SubSynth,
    pub block_size: usize,
    part_sf2: [bool; 8],
    part_program: [u8; 8],
    part_bank: [u8; 8],
    drums_sf2: bool,
    sample_rate: i32,
    pub drum_volume: f32,
    /// Last GM program sent per MIDI channel for the sequencer (255 = never sent).
    seq_channel_program: [u8; 16],
    /// Sample start offset in samples — skip this many samples after note-on
    /// to compensate for slow SF2 attack envelopes.
    sample_offset: usize,
    /// Last sustain state, re-applied after SF2 rebuilds (fresh rustysynth
    /// channels start with hold_pedal=false).
    sustain: bool,
    /// Global SF2 release multiplier, re-applied after SF2 rebuilds (fresh
    /// rustysynth instances start at scale 1.0).
    release_scale: f32,
}

impl SamplerEngine {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            keys: SubSynth::new(),
            drums: SubSynth::new(),
            block_size: DEFAULT_BLOCK_SIZE,
            part_sf2: [false; 8],
            part_program: [0; 8],
            part_bank: [0; 8],
            drums_sf2: false,
            sample_rate: sample_rate as i32,
            drum_volume: 1.0,
            seq_channel_program: [255; 16], // 255 = not yet initialised
            sample_offset: 0,
            sustain: false,
            release_scale: 1.0,
        }
    }

    /// Load SF2 for keys (parts A/B). The returned `Displaced` holds the
    /// previous soundfont and synth and must be freed off the audio thread.
    #[must_use = "the displaced soundfont must be freed off the audio thread"]
    pub fn load_keys_soundfont(&mut self, sf: Arc<SoundFont>) -> Displaced {
        self.seq_channel_program = [255; 16]; // new synth instance — reset program cache
        let displaced = self.keys.rebuild(Some(sf), self.sample_rate, self.block_size);
        // Restore programs on new synth
        if let Some(synth) = &mut self.keys.synth {
            for i in 0..8 {
                if self.part_sf2[i] {
                    synth.process_midi_message(i as i32, 0xB0, 0, self.part_bank[i] as i32);
                    synth.process_midi_message(i as i32, 0xC0, self.part_program[i] as i32, 0);
                }
            }
        }
        self.apply_sustain();
        if let Some(synth) = &mut self.keys.synth {
            synth.set_release_time_scale(self.release_scale);
        }
        displaced
    }

    /// Load SF2 for drums (ch9).
    #[must_use = "the displaced soundfont must be freed off the audio thread"]
    pub fn load_drums_soundfont(&mut self, sf: Arc<SoundFont>) -> Displaced {
        self.drums.rebuild(Some(sf), self.sample_rate, self.block_size)
    }

    #[must_use = "the displaced soundfont must be freed off the audio thread"]
    pub fn unload_keys(&mut self) -> Displaced {
        self.keys.rebuild(None, self.sample_rate, self.block_size)
    }

    #[must_use = "the displaced soundfont must be freed off the audio thread"]
    pub fn unload_drums(&mut self) -> Displaced {
        self.drums.rebuild(None, self.sample_rate, self.block_size)
    }

    #[allow(dead_code)]
    pub fn keys_loaded(&self) -> bool { self.keys.is_loaded() }
    #[allow(dead_code)]
    pub fn drums_loaded(&self) -> bool { self.drums.is_loaded() }

    /// Returns both displaced sub-synths (keys, drums) for deferred freeing.
    #[must_use = "the displaced soundfonts must be freed off the audio thread"]
    pub fn set_block_size(&mut self, size: usize) -> (Displaced, Displaced) {
        let size = size.clamp(8, MAX_BLOCK_SIZE);
        self.block_size = size;
        let mut keys_displaced = Displaced::default();
        if let Some(sf) = self.keys.soundfont.clone() {
            keys_displaced = self.keys.rebuild(Some(sf), self.sample_rate, self.block_size);
            // Restore programs
            if let Some(synth) = &mut self.keys.synth {
                for i in 0..8 {
                    if self.part_sf2[i] {
                        synth.process_midi_message(i as i32, 0xB0, 0, self.part_bank[i] as i32);
                        synth.process_midi_message(i as i32, 0xC0, self.part_program[i] as i32, 0);
                    }
                }
                // Rebuild made a fresh Synthesizer — re-apply the release scale
                synth.set_release_time_scale(self.release_scale);
            }
        }
        let mut drums_displaced = Displaced::default();
        if let Some(sf) = self.drums.soundfont.clone() {
            drums_displaced = self.drums.rebuild(Some(sf), self.sample_rate, self.block_size);
        }
        self.apply_sustain();
        (keys_displaced, drums_displaced)
    }

    #[must_use = "the displaced soundfonts must be freed off the audio thread"]
    pub fn set_sample_rate(&mut self, sample_rate: f32) -> (Displaced, Displaced) {
        self.sample_rate = sample_rate as i32;
        let mut keys_displaced = Displaced::default();
        if let Some(sf) = self.keys.soundfont.clone() {
            keys_displaced = self.load_keys_soundfont(sf);
        }
        let mut drums_displaced = Displaced::default();
        if let Some(sf) = self.drums.soundfont.clone() {
            drums_displaced = self.load_drums_soundfont(sf);
        }
        self.apply_sustain();
        (keys_displaced, drums_displaced)
    }

    pub fn set_part_mode(&mut self, part: usize, enabled: bool) {
        if part < 8 { self.part_sf2[part] = enabled; }
    }

    pub fn set_part_program(&mut self, part: usize, program: u8, bank: u8) {
        if part >= 8 { return; }
        self.part_program[part] = program;
        self.part_bank[part] = bank;
        self.keys.midi(part as i32, 0xB0, 0, bank as i32);
        self.keys.midi(part as i32, 0xC0, program as i32, 0);
    }

    pub fn set_drums_enabled(&mut self, enabled: bool) {
        self.drums_sf2 = enabled;
    }

    pub fn drums_enabled(&self) -> bool {
        self.drums_sf2
    }

    pub fn set_layer_volume(&mut self, part: usize, volume: f32) {
        if part >= 8 { return; }
        let ch = (part as i32) % 16;
        let cc_val = (volume * 127.0).round().clamp(0.0, 127.0) as i32;
        self.keys.midi(ch, 0xB0, 7, cc_val);
    }

    /// Set sample start offset in milliseconds (0-50).
    /// Skips this many ms of audio after each note-on to cut through slow SF2 attacks.
    pub fn set_sample_offset_ms(&mut self, ms: f32) {
        let ms = ms.clamp(0.0, 50.0);
        self.sample_offset = ((ms / 1000.0) * self.sample_rate as f32) as usize;
    }

    pub fn note_on(&mut self, part: usize, note: u8, velocity: u8) {
        // rustysynth uses MIDI channels 0-15; map part to channel
        let ch = (part as i32) % 16;
        self.keys.midi(ch, 0x90, note as i32, velocity as i32);
        // Skip initial samples to reduce perceived latency from slow SF2 attacks
        if self.sample_offset > 0 {
            self.keys.skip_samples(self.sample_offset, self.block_size);
        }
    }

    pub fn note_off(&mut self, part: usize, note: u8) {
        let ch = (part as i32) % 16;
        self.keys.midi(ch, 0x80, note as i32, 0);
    }

    /// Sustain pedal (CC64) for all part channels of the keys synth.
    /// rustysynth implements hold-pedal per channel: note-offs received while
    /// the pedal is down keep sounding until it is released. Drum channel 9
    /// is intentionally not sustained.
    pub fn sustain_pedal(&mut self, on: bool) {
        self.sustain = on;
        let val = if on { 127 } else { 0 };
        for ch in 0..8usize {
            self.keys.midi(ch as i32, 0xB0, 64, val);
        }
    }

    /// Global SF2 release multiplier (GM2 CC72 semantics). 1.0 = file defaults.
    pub fn set_release_time_scale(&mut self, scale: f32) {
        self.release_scale = scale;
        if let Some(synth) = &mut self.keys.synth {
            synth.set_release_time_scale(scale);
        }
    }

    /// Re-send CC64 to the part channels after a rustysynth rebuild.
    fn apply_sustain(&mut self) {
        let on = self.sustain;
        let val = if on { 127 } else { 0 };
        for ch in 0..8usize {
            self.keys.midi(ch as i32, 0xB0, 64, val);
        }
    }

    /// Kill every sounding SF2 voice of one part. Clears the channel's hold
    /// pedal first — `note_off_all_channel` respects it and would leave the
    /// notes hanging otherwise. Used when a part switches patch or is disabled.
    pub fn part_all_notes_off(&mut self, part: usize) {
        let ch = (part as i32) % 16;
        self.keys.midi(ch, 0xB0, 64, 0);
        if let Some(synth) = &mut self.keys.synth {
            synth.note_off_all_channel(ch, false);
        }
    }

    pub fn drum_note_on(&mut self, note: u8, velocity: u8) {
        self.drums.midi(9, 0x90, note as i32, velocity as i32);
    }

    pub fn drum_note_off(&mut self, note: u8) {
        self.drums.midi(9, 0x80, note as i32, 0);
    }

    /// MIDI sequencer: note-on on any channel of the keys synth.
    pub fn seq_note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        self.keys.midi(channel as i32, 0x90, note as i32, velocity as i32);
    }

    /// MIDI sequencer: note-off on any channel of the keys synth.
    pub fn seq_note_off(&mut self, channel: u8, note: u8) {
        self.keys.midi(channel as i32, 0x80, note as i32, 0);
    }

    /// MIDI sequencer: set GM program for a channel of the keys synth.
    /// Skips the MIDI message if the program is already set (avoids per-note overhead).
    pub fn seq_program_set(&mut self, channel: u8, program: u8) {
        let ch = channel as usize;
        if ch < 16 && self.seq_channel_program[ch] == program {
            return; // already set, no-op
        }
        self.keys.midi(channel as i32, 0xC0, program as i32, 0);
        if ch < 16 {
            self.seq_channel_program[ch] = program;
        }
    }

    pub fn pitch_bend(&mut self, part: usize, value: f32) {
        if part >= 8 { return; }
        let ch = (part as i32) % 16;
        let raw = ((value + 1.0) * 0.5 * 16383.0).round().clamp(0.0, 16383.0) as i32;
        let lsb = raw & 0x7F;
        let msb = (raw >> 7) & 0x7F;
        self.keys.midi(ch, 0xE0, lsb, msb);
    }

    pub fn set_pitch_bend_range(&mut self, semitones: u8) {
        if let Some(synth) = &mut self.keys.synth {
            synth.set_pitch_bend_range(semitones as i32);
        }
    }

    pub fn mod_wheel(&mut self, part: usize, value: f32) {
        if part >= 8 { return; }
        let ch = (part as i32) % 16;
        let cc_val = (value * 127.0).round().clamp(0.0, 127.0) as i32;
        self.keys.midi(ch, 0xB0, 1, cc_val);
    }

    pub fn all_notes_off(&mut self) {
        // rustysynth defers note-offs while a channel hold pedal is down
        // (voice.rs RELEASE_REQUESTED); lift CC64 everywhere before CC123.
        self.sustain = false;
        for ch in 0..16 {
            self.keys.midi(ch, 0xB0, 64, 0);
            self.keys.midi(ch, 0xB0, 123, 0);
            self.drums.midi(ch, 0xB0, 123, 0);
        }
    }

    /// Get next stereo sample from both keys and drums synths.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        let bs = self.block_size;
        let (kl, kr) = self.keys.tick(bs);
        let (dl, dr) = self.drums.tick(bs);
        let dv = self.drum_volume * self.drum_volume; // perceptual curve
        (kl + dl * dv, kr + dr * dv)
    }
}
