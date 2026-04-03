/// SF2 sampler engine wrapping rustysynth.
/// Block-buffered rendering for efficiency (64 samples at a time).

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

pub struct SamplerEngine {
    soundfont: Option<Arc<SoundFont>>,
    synth: Option<Synthesizer>,
    buf_left: [f32; MAX_BLOCK_SIZE],
    buf_right: [f32; MAX_BLOCK_SIZE],
    buf_pos: usize,
    pub block_size: usize,
    // Per-layer: channel 0 = layer A, channel 1 = layer B, channel 9 = drums
    layer_sf2: [bool; 2],
    layer_program: [u8; 2],
    layer_bank: [u8; 2],
    drums_sf2: bool,
    sample_rate: i32,
}

impl SamplerEngine {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            soundfont: None,
            synth: None,
            buf_left: [0.0; MAX_BLOCK_SIZE],
            buf_right: [0.0; MAX_BLOCK_SIZE],
            buf_pos: DEFAULT_BLOCK_SIZE, // force render on first tick
            block_size: DEFAULT_BLOCK_SIZE,
            layer_sf2: [false; 2],
            layer_program: [0; 2],
            layer_bank: [0; 2],
            drums_sf2: false,
            sample_rate: sample_rate as i32,
        }
    }

    /// Load an SF2 soundfont. Creates the internal Synthesizer.
    pub fn load_soundfont(&mut self, sf: Arc<SoundFont>) {
        self.rebuild_synth(Some(sf));
    }

    /// Set block size and recreate synth if loaded.
    pub fn set_block_size(&mut self, size: usize) {
        let size = size.clamp(8, MAX_BLOCK_SIZE);
        self.block_size = size;
        if let Some(sf) = self.soundfont.clone() {
            self.rebuild_synth(Some(sf));
        }
    }

    fn rebuild_synth(&mut self, sf: Option<Arc<SoundFont>>) {
        let sf = match sf {
            Some(s) => s,
            None => { self.synth = None; self.soundfont = None; return; }
        };
        let mut settings = SynthesizerSettings::new(self.sample_rate);
        settings.block_size = self.block_size;
        settings.enable_reverb_and_chorus = false;
        match Synthesizer::new(&sf, &settings) {
            Ok(mut synth) => {
                for i in 0..2 {
                    if self.layer_sf2[i] {
                        synth.process_midi_message(i as i32, 0xC0, self.layer_program[i] as i32, 0);
                    }
                }
                self.synth = Some(synth);
                self.soundfont = Some(sf);
                self.buf_pos = self.block_size;
            }
            Err(_) => {
                self.synth = None;
                self.soundfont = None;
            }
        }
    }

    /// Unload current soundfont.
    #[allow(dead_code)]
    pub fn unload(&mut self) {
        self.synth = None;
        self.soundfont = None;
    }

    #[allow(dead_code)]
    pub fn is_loaded(&self) -> bool {
        self.synth.is_some()
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate as i32;
        if let Some(sf) = self.soundfont.clone() {
            self.load_soundfont(sf);
        }
    }

    pub fn set_layer_mode(&mut self, layer: usize, enabled: bool) {
        if layer < 2 {
            self.layer_sf2[layer] = enabled;
        }
    }

    #[allow(dead_code)]
    pub fn layer_sf2(&self, layer: usize) -> bool {
        if layer < 2 { self.layer_sf2[layer] } else { false }
    }

    pub fn set_layer_program(&mut self, layer: usize, program: u8, bank: u8) {
        if layer >= 2 { return; }
        self.layer_program[layer] = program;
        self.layer_bank[layer] = bank;
        if let Some(synth) = &mut self.synth {
            let ch = layer as i32;
            // Bank select MSB
            synth.process_midi_message(ch, 0xB0, 0, bank as i32);
            // Program change
            synth.process_midi_message(ch, 0xC0, program as i32, 0);
        }
    }

    #[allow(dead_code)]
    pub fn layer_program(&self, layer: usize) -> u8 {
        if layer < 2 { self.layer_program[layer] } else { 0 }
    }

    pub fn set_drums_enabled(&mut self, enabled: bool) {
        self.drums_sf2 = enabled;
    }

    pub fn drums_enabled(&self) -> bool {
        self.drums_sf2
    }

    /// Set layer volume via MIDI CC7 on the layer's channel.
    pub fn set_layer_volume(&mut self, layer: usize, volume: f32) {
        if layer >= 2 { return; }
        if let Some(synth) = &mut self.synth {
            let cc_val = (volume * 127.0).round().clamp(0.0, 127.0) as i32;
            synth.process_midi_message(layer as i32, 0xB0, 7, cc_val);
        }
    }

    pub fn note_on(&mut self, layer: usize, note: u8, velocity: u8) {
        if layer >= 2 { return; }
        if let Some(synth) = &mut self.synth {
            synth.process_midi_message(layer as i32, 0x90, note as i32, velocity as i32);
        }
    }

    pub fn note_off(&mut self, layer: usize, note: u8) {
        if layer >= 2 { return; }
        if let Some(synth) = &mut self.synth {
            synth.process_midi_message(layer as i32, 0x80, note as i32, 0);
        }
    }

    pub fn drum_note_on(&mut self, note: u8, velocity: u8) {
        if let Some(synth) = &mut self.synth {
            synth.process_midi_message(9, 0x90, note as i32, velocity as i32);
        }
    }

    pub fn drum_note_off(&mut self, note: u8) {
        if let Some(synth) = &mut self.synth {
            synth.process_midi_message(9, 0x80, note as i32, 0);
        }
    }

    pub fn pitch_bend(&mut self, layer: usize, value: f32) {
        if layer >= 2 { return; }
        if let Some(synth) = &mut self.synth {
            // Convert -1..1 to 0..16383 (center=8192)
            let raw = ((value + 1.0) * 0.5 * 16383.0).round().clamp(0.0, 16383.0) as i32;
            let lsb = raw & 0x7F;
            let msb = (raw >> 7) & 0x7F;
            synth.process_midi_message(layer as i32, 0xE0, lsb, msb);
        }
    }

    pub fn all_notes_off(&mut self) {
        if let Some(synth) = &mut self.synth {
            for ch in 0..16 {
                // CC 123 = All Notes Off
                synth.process_midi_message(ch, 0xB0, 123, 0);
            }
        }
    }

    /// Get next stereo sample. Renders a new block when buffer is exhausted.
    #[inline]
    pub fn tick(&mut self) -> (f32, f32) {
        let synth = match &mut self.synth {
            Some(s) => s,
            None => return (0.0, 0.0),
        };
        let bs = self.block_size;
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
