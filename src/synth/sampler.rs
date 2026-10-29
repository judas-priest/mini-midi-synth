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
    part_sf2: [bool; 16],
    part_program: [u8; 16],
    part_bank: [u8; 16],
    /// Melodic SF2 channel per part (skips percussion ch9). None = DSP part
    /// or no free channel. 16 parts, 15 melodic channels — the 16th SF2 part
    /// stays DSP until a channel frees up.
    part_channel: [Option<usize>; 16],
    drums_sf2: bool,
    sample_rate: i32,
    pub drum_volume: f32,
    /// Last GM program sent per MIDI channel for the sequencer (255 = never sent).
    seq_channel_program: [u8; 16],
    /// Channels whose GM program the MIDI-file sequencer currently owns
    /// (claimed by `seq_program_set`, released by `seq_channels_release`).
    /// A rustysynth rebuild must NOT restore part programs onto these — the
    /// sequencer re-asserts its own program on the next note-on because
    /// `seq_channel_program` resets to 255 on rebuild.
    seq_channel_used: [bool; 16],
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
            part_sf2: [false; 16],
            part_program: [0; 16],
            part_bank: [0; 16],
            part_channel: [None; 16],
            drums_sf2: false,
            sample_rate: sample_rate as i32,
            drum_volume: 1.0,
            seq_channel_program: [255; 16], // 255 = not yet initialised
            seq_channel_used: [false; 16],
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
        // Restore programs on the new synth — but not on channels the
        // sequencer currently owns (it re-asserts per note-on; the cache
        // reset above makes that happen).
        self.send_part_programs(true);
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
        // A rebuild (even a deferred/no-op one with no soundfont loaded)
        // invalidates the sequencer's dedup cache: reset it so seq note-ons
        // re-assert their programs. Hoisted outside the soundfont guard so
        // the reset happens unconditionally (review fix).
        self.seq_channel_program = [255; 16];
        let mut keys_displaced = Displaced::default();
        if let Some(sf) = self.keys.soundfont.clone() {
            keys_displaced = self.keys.rebuild(Some(sf), self.sample_rate, self.block_size);
            // Restore part programs only on channels the sequencer does not
            // own — the seq re-asserts per note-on after the reset above.
            self.send_part_programs(true);
            // Rebuild made a fresh Synthesizer — re-apply the release scale
            if let Some(synth) = &mut self.keys.synth {
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

    /// Melodic channels 0..=8, 10..=15 — percussion channel 9 reserved.
    const MELODIC_CHANNELS: [usize; 15] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15];

    fn alloc_channel(&self) -> Option<usize> {
        Self::MELODIC_CHANNELS
            .iter()
            .find(|c| !self.part_channel.contains(&Some(**c)))
            .copied()
    }

    pub fn set_part_mode(&mut self, part: usize, enabled: bool) {
        if part >= 16 { return; }
        if enabled && self.part_channel[part].is_none() {
            self.part_channel[part] = self.alloc_channel();
        } else if !enabled {
            if let Some(ch) = self.part_channel[part].take() {
                if let Some(synth) = &mut self.keys.synth {
                    synth.note_off_all_channel(ch as i32, false);
                }
            }
        }
        self.part_sf2[part] = enabled;
    }

    /// Channel of an SF2-enabled part (None for DSP parts / unallocated).
    fn part_channel_of(&self, part: usize) -> Option<usize> {
        self.part_channel.get(part).copied().flatten()
    }

    /// Re-send bank+program for every SF2-enabled part that owns a channel.
    /// With `skip_seq_channels`, channels currently owned by the MIDI-file
    /// sequencer are left alone: the sequencer owns the program there while
    /// it plays, and re-asserts it per note-on after a rebuild (its dedup
    /// cache resets to 255). RT-safe: fixed loops, existing `keys.midi`.
    fn send_part_programs(&mut self, skip_seq_channels: bool) {
        for (i, ch) in self.part_channel.iter().enumerate() {
            let Some(ch) = *ch else { continue };
            if !self.part_sf2[i] { continue; }
            if skip_seq_channels && self.seq_channel_used[ch] { continue; }
            self.keys.midi(ch as i32, 0xB0, 0, self.part_bank[i] as i32);
            self.keys.midi(ch as i32, 0xC0, self.part_program[i] as i32, 0);
        }
    }

    /// MIDI-file playback stopped or a new file loaded: the sequencer no
    /// longer owns any channel's program. Reset both seq caches and re-assert
    /// every part's program so live keys get their own timbre back (a seq
    /// program change on a shared channel used to desync `part_program[]`
    /// until an explicit SetPartSf2Program). RT-safe: array writes + the
    /// fixed `send_part_programs` loop.
    pub fn seq_channels_release(&mut self) {
        self.seq_channel_program = [255; 16];
        self.seq_channel_used = [false; 16];
        self.send_part_programs(false);
    }

    pub fn set_part_program(&mut self, part: usize, program: u8, bank: u8) {
        if part >= 16 { return; }
        self.part_program[part] = program;
        self.part_bank[part] = bank;
        if let Some(ch) = self.part_channel_of(part) {
            self.keys.midi(ch as i32, 0xB0, 0, bank as i32);
            self.keys.midi(ch as i32, 0xC0, program as i32, 0);
        }
    }

    pub fn set_drums_enabled(&mut self, enabled: bool) {
        self.drums_sf2 = enabled;
    }

    pub fn drums_enabled(&self) -> bool {
        self.drums_sf2
    }

    /// Per-channel pan (CC10) for SF2 parts — rustysynth applies it per voice.
    pub fn set_channel_pan(&mut self, part: usize, cc_value: i32) {
        if let Some(ch) = self.part_channel_of(part) {
            self.keys.midi(ch as i32, 0xB0, 10, cc_value.clamp(0, 127));
        }
    }

    pub fn set_layer_volume(&mut self, part: usize, volume: f32) {
        if part >= 16 { return; }
        let Some(ch) = self.part_channel_of(part) else { return; };
        let cc_val = (volume * 127.0).round().clamp(0.0, 127.0) as i32;
        self.keys.midi(ch as i32, 0xB0, 7, cc_val);
    }

    /// Set sample start offset in milliseconds (0-50).
    /// Skips this many ms of audio after each note-on to cut through slow SF2 attacks.
    pub fn set_sample_offset_ms(&mut self, ms: f32) {
        let ms = ms.clamp(0.0, 50.0);
        self.sample_offset = ((ms / 1000.0) * self.sample_rate as f32) as usize;
    }

    pub fn note_on(&mut self, part: usize, note: u8, velocity: u8) {
        let Some(ch) = self.part_channel_of(part) else { return; };
        self.keys.midi(ch as i32, 0x90, note as i32, velocity as i32);
        // Skip initial samples to reduce perceived latency from slow SF2 attacks
        if self.sample_offset > 0 {
            self.keys.skip_samples(self.sample_offset, self.block_size);
        }
    }

    pub fn note_off(&mut self, part: usize, note: u8) {
        let Some(ch) = self.part_channel_of(part) else { return; };
        self.keys.midi(ch as i32, 0x80, note as i32, 0);
    }

    /// Sustain pedal (CC64) for all part channels of the keys synth.
    /// rustysynth implements hold-pedal per channel: note-offs received while
    /// the pedal is down keep sounding until it is released. Drum channel 9
    /// is intentionally not sustained.
    pub fn sustain_pedal(&mut self, on: bool) {
        self.sustain = on;
        let val = if on { 127 } else { 0 };
        for ch in self.part_channel.iter().flatten() {
            self.keys.midi(*ch as i32, 0xB0, 64, val);
        }
    }

    /// Global SF2 release multiplier (GM2 CC72 semantics). 1.0 = file defaults.
    pub fn set_release_time_scale(&mut self, scale: f32) {
        self.release_scale = scale;
        if let Some(synth) = &mut self.keys.synth {
            synth.set_release_time_scale(scale);
        }
    }

    /// Current release-time scale (persists across rustysynth rebuilds).
    pub fn release_scale(&self) -> f32 { self.release_scale }

    /// Re-send CC64 to the part channels after a rustysynth rebuild.
    fn apply_sustain(&mut self) {
        let on = self.sustain;
        let val = if on { 127 } else { 0 };
        for ch in self.part_channel.iter().flatten() {
            self.keys.midi(*ch as i32, 0xB0, 64, val);
        }
    }

    /// Kill every sounding SF2 voice of one part. Clears the channel's hold
    /// pedal first — `note_off_all_channel` respects it and would leave the
    /// notes hanging otherwise. Used when a part switches patch or is disabled.
    pub fn part_all_notes_off(&mut self, part: usize) {
        let Some(ch) = self.part_channel_of(part) else { return; };
        self.keys.midi(ch as i32, 0xB0, 64, 0);
        if let Some(synth) = &mut self.keys.synth {
            synth.note_off_all_channel(ch as i32, false);
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
    /// Skips the MIDI message if the program is already set (avoids per-note
    /// overhead). Marks the channel as seq-owned until `seq_channels_release`
    /// so a rebuild's part-restore skips it.
    pub fn seq_program_set(&mut self, channel: u8, program: u8) {
        let ch = channel as usize;
        if ch < 16 {
            self.seq_channel_used[ch] = true;
            if self.seq_channel_program[ch] == program {
                return; // already set, no-op
            }
            self.seq_channel_program[ch] = program;
        }
        self.keys.midi(channel as i32, 0xC0, program as i32, 0);
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

    /// All Sound Off (CC120): kill every rustysynth voice immediately, no
    /// release tail. Pedal state is irrelevant (voices are cleared) but is
    /// still cleared so a later note-off is not swallowed.
    pub fn all_sound_off(&mut self) {
        self.sustain = false;
        if let Some(synth) = &mut self.keys.synth { synth.note_off_all(true); }
        if let Some(synth) = &mut self.drums.synth { synth.note_off_all(true); }
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

// ── Program-cache ownership tests (no .sf2: keys.synth is None, every
//    keys.midi send is a no-op; the arrays below ARE the contract) ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synth::{ControlEvent, SynthEngine};

    #[test]
    fn seq_program_set_marks_channel_and_leaves_part_cache() {
        let mut s = SamplerEngine::new(48000.0);
        s.set_part_program(0, 5, 0);
        assert_eq!(s.part_program[0], 5);
        s.seq_program_set(0, 40);
        assert_eq!(s.seq_channel_program[0], 40, "seq cache not updated");
        assert!(s.seq_channel_used[0], "seq channel 0 not marked as used");
        assert_eq!(s.part_program[0], 5, "seq program overwrote the part cache");
        // dedup: an identical program must not re-send / change anything
        s.seq_program_set(0, 40);
        assert_eq!(s.seq_channel_program[0], 40);
        // out-of-range channel: no cache write, no panic
        s.seq_program_set(16, 7);
        assert!(s.seq_channel_used.iter().skip(1).all(|&u| !u),
            "out-of-range channel polluted the used mask");
    }

    #[test]
    fn block_size_change_resets_seq_program_cache() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(2, 60);
        let _ = e.handle_control(ControlEvent::SetSf2BlockSize { size: 16 });
        assert_eq!(e.sampler.seq_channel_program[2], 255,
            "keys rebuild left a stale seq dedup cache — the sequencer never re-asserts");
        assert!(e.sampler.seq_channel_used[2],
            "used mask must survive a rebuild so a later stop re-asserts part programs");
        // Same guarantee through the sample-rate path (routes through load_keys_soundfont)
        e.sampler.seq_program_set(5, 33);
        let _ = e.handle_control(ControlEvent::SetSf2BlockSize { size: 32 });
        assert_eq!(e.sampler.seq_channel_program[5], 255,
            "second rebuild kept a stale seq dedup cache");
    }

    #[test]
    fn seq_stop_resets_seq_program_ownership() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(3, 90);
        let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
        assert_eq!(e.sampler.seq_channel_program[3], 255,
            "stop did not reset the seq program cache");
        assert!(!e.sampler.seq_channel_used[3],
            "stop did not clear the seq used mask");
    }

    #[test]
    fn seq_load_resets_seq_program_ownership() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(3, 90);
        let data = Box::new(crate::synth::midi_player::MidiSeqData {
            ticks_per_beat: 480,
            total_ticks: 480,
            events: Vec::new(),
            track_names: Vec::new(),
            track_channels: Vec::new(),
        });
        let _ = e.handle_control(ControlEvent::MidiSeqLoad { data });
        assert_eq!(e.sampler.seq_channel_program[3], 255,
            "load did not reset the seq program cache");
        assert!(!e.sampler.seq_channel_used[3],
            "load did not clear the seq used mask");
    }

    #[test]
    fn seq_ownership_survives_rebuild_but_not_stop() {
        let mut e = SynthEngine::new(48000.0);
        e.sampler.seq_program_set(1, 70);
        let _ = e.handle_control(ControlEvent::SetSf2BlockSize { size: 24 });
        assert!(e.sampler.seq_channel_used[1], "rebuild must not release ownership");
        let _ = e.handle_control(ControlEvent::MidiSeqPlay { playing: false });
        assert!(!e.sampler.seq_channel_used[1], "stop must release ownership");
    }
}
