/// Modulation Matrix — sparse slot-based routing from any source to any destination.
/// Max 16 slots per layer, evaluated once per sample (control rate).

/// Number of modulation slots per layer.
pub const MOD_SLOTS: usize = 16;

#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum ModSource {
    None = 0,
    Lfo1 = 1,
    Lfo2 = 2,
    Lfo3 = 3,
    Lfo4 = 4,
    AmpEnv = 5,
    FilterEnv = 6,
    Mseg1 = 7,
    Mseg2 = 8,
    ModWheel = 9,
    Aftertouch = 10,
    Velocity = 11,
    KeyTrack = 12,
    StepSeq = 13,
    RandomBipolar = 14,   // -1..+1 randomized per note-on
    RandomUnipolar = 15,  // 0..1 randomized per note-on
    AltBipolar = 16,      // alternates +1/-1 each note-on
    AltUnipolar = 17,     // alternates 0/1 each note-on
    ReleaseVel = 18,      // note-off velocity 0..1
    PitchBend = 19,       // pitch bend wheel -1..+1
    Cc1 = 20,             // MIDI CC assignable 1
    Cc2 = 21,             // MIDI CC assignable 2
    Cc3 = 22,             // MIDI CC assignable 3
    Cc4 = 23,             // MIDI CC assignable 4
    Breath = 24,          // CC2, 0..1
    Expression = 25,      // CC11, 0..1
    SustainPedal = 26,    // CC64: 0.0 or 1.0
    LowestKey = 27,       // lowest held note, -1..+1 centered on C4
    HighestKey = 28,      // highest held note, -1..+1 centered on C4
    LatestKey = 29,       // most recent note, -1..+1 centered on C4
    PolyAftertouch = 30,  // per-note pressure, 0..1
    SceneLfo1 = 31,       // free-running Scene LFO 1 (never resets on note-on)
    SceneLfo2 = 32,       // free-running Scene LFO 2
}

impl ModSource {
    pub fn from_param(v: f32) -> Self {
        match v as u8 {
            1 => Self::Lfo1,
            2 => Self::Lfo2,
            3 => Self::Lfo3,
            4 => Self::Lfo4,
            5 => Self::AmpEnv,
            6 => Self::FilterEnv,
            7 => Self::Mseg1,
            8 => Self::Mseg2,
            9 => Self::ModWheel,
            10 => Self::Aftertouch,
            11 => Self::Velocity,
            12 => Self::KeyTrack,
            13 => Self::StepSeq,
            14 => Self::RandomBipolar,
            15 => Self::RandomUnipolar,
            16 => Self::AltBipolar,
            17 => Self::AltUnipolar,
            18 => Self::ReleaseVel,
            19 => Self::PitchBend,
            20 => Self::Cc1,
            21 => Self::Cc2,
            22 => Self::Cc3,
            23 => Self::Cc4,
            24 => Self::Breath,
            25 => Self::Expression,
            26 => Self::SustainPedal,
            27 => Self::LowestKey,
            28 => Self::HighestKey,
            29 => Self::LatestKey,
            30 => Self::PolyAftertouch,
            31 => Self::SceneLfo1,
            32 => Self::SceneLfo2,
            _ => Self::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Lfo1 => "LFO 1",
            Self::Lfo2 => "LFO 2",
            Self::Lfo3 => "LFO 3",
            Self::Lfo4 => "LFO 4",
            Self::AmpEnv => "Amp Env",
            Self::FilterEnv => "Filter Env",
            Self::Mseg1 => "MSEG 1",
            Self::Mseg2 => "MSEG 2",
            Self::ModWheel => "Mod Wheel",
            Self::Aftertouch => "Aftertouch",
            Self::Velocity => "Velocity",
            Self::KeyTrack => "Key Track",
            Self::StepSeq => "Step Seq",
            Self::RandomBipolar => "Random ±",
            Self::RandomUnipolar => "Random +",
            Self::AltBipolar => "Alternate ±",
            Self::AltUnipolar => "Alternate +",
            Self::ReleaseVel => "Release Vel",
            Self::PitchBend => "Pitch Bend",
            Self::Cc1 => "CC 1",
            Self::Cc2 => "CC 2",
            Self::Cc3 => "CC 3",
            Self::Cc4 => "CC 4",
            Self::Breath => "Breath",
            Self::Expression => "Expression",
            Self::SustainPedal => "Sustain Pedal",
            Self::LowestKey => "Lowest Key",
            Self::HighestKey => "Highest Key",
            Self::LatestKey => "Latest Key",
            Self::PolyAftertouch => "Poly AT",
            Self::SceneLfo1 => "Scene LFO 1",
            Self::SceneLfo2 => "Scene LFO 2",
        }
    }

    pub const ALL: &[ModSource] = &[
        Self::None, Self::Lfo1, Self::Lfo2, Self::Lfo3, Self::Lfo4,
        Self::SceneLfo1, Self::SceneLfo2,
        Self::AmpEnv, Self::FilterEnv, Self::Mseg1, Self::Mseg2,
        Self::ModWheel, Self::Aftertouch, Self::Velocity, Self::KeyTrack,
        Self::StepSeq,
        Self::RandomBipolar, Self::RandomUnipolar,
        Self::AltBipolar, Self::AltUnipolar,
        Self::ReleaseVel, Self::PitchBend,
        Self::Cc1, Self::Cc2, Self::Cc3, Self::Cc4,
        Self::Breath, Self::Expression, Self::SustainPedal,
        Self::LowestKey, Self::HighestKey, Self::LatestKey,
        Self::PolyAftertouch,
    ];
}

#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum ModDest {
    None = 0,
    Pitch = 1,
    FilterCutoff = 2,
    FilterResonance = 3,
    Amplitude = 4,
    Osc1Level = 5,
    Osc2Level = 6,
    Osc3Level = 7,
    FmCrossDepth = 8,
    PulseWidth = 9,
    NoiseLevel = 10,
    Pan = 11,
    Lfo1Rate = 12,
    Lfo2Rate = 13,
    ChorusMix = 14,
    DelayMix = 15,
    ReverbMix = 16,
    TapeDrive = 17,
    NeuronDrive = 18,
    RingModFreq = 19,
    FreqShiftHz = 20,
    PhaserRate = 21,
    FlangerRate = 22,
    Lfo3Rate = 23,
    Lfo4Rate = 24,
    WaveShaperDrive = 25,
    WaveShaperMix = 26,
    RotaryMix = 27,
    EnsembleMix = 28,
    ResonatorMix = 29,
    BonsaiDrive = 30,
}

impl ModDest {
    pub fn from_param(v: f32) -> Self {
        match v as u8 {
            1 => Self::Pitch,
            2 => Self::FilterCutoff,
            3 => Self::FilterResonance,
            4 => Self::Amplitude,
            5 => Self::Osc1Level,
            6 => Self::Osc2Level,
            7 => Self::Osc3Level,
            8 => Self::FmCrossDepth,
            9 => Self::PulseWidth,
            10 => Self::NoiseLevel,
            11 => Self::Pan,
            12 => Self::Lfo1Rate,
            13 => Self::Lfo2Rate,
            14 => Self::ChorusMix,
            15 => Self::DelayMix,
            16 => Self::ReverbMix,
            17 => Self::TapeDrive,
            18 => Self::NeuronDrive,
            19 => Self::RingModFreq,
            20 => Self::FreqShiftHz,
            21 => Self::PhaserRate,
            22 => Self::FlangerRate,
            23 => Self::Lfo3Rate,
            24 => Self::Lfo4Rate,
            25 => Self::WaveShaperDrive,
            26 => Self::WaveShaperMix,
            27 => Self::RotaryMix,
            28 => Self::EnsembleMix,
            29 => Self::ResonatorMix,
            30 => Self::BonsaiDrive,
            _ => Self::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Pitch => "Pitch",
            Self::FilterCutoff => "Filter Cutoff",
            Self::FilterResonance => "Filter Reso",
            Self::Amplitude => "Amplitude",
            Self::Osc1Level => "Osc 1 Level",
            Self::Osc2Level => "Osc 2 Level",
            Self::Osc3Level => "Osc 3 Level",
            Self::FmCrossDepth => "FM Cross",
            Self::PulseWidth => "Pulse Width",
            Self::NoiseLevel => "Noise Level",
            Self::Pan => "Pan",
            Self::Lfo1Rate => "LFO 1 Rate",
            Self::Lfo2Rate => "LFO 2 Rate",
            Self::ChorusMix => "Chorus Mix",
            Self::DelayMix => "Delay Mix",
            Self::ReverbMix => "Reverb Mix",
            Self::TapeDrive => "Tape Drive",
            Self::NeuronDrive => "Neuron Drive",
            Self::RingModFreq => "Ring Mod Freq",
            Self::FreqShiftHz => "Freq Shift Hz",
            Self::PhaserRate => "Phaser Rate",
            Self::FlangerRate => "Flanger Rate",
            Self::Lfo3Rate => "LFO 3 Rate",
            Self::Lfo4Rate => "LFO 4 Rate",
            Self::WaveShaperDrive => "WaveShaper Drive",
            Self::WaveShaperMix => "WaveShaper Mix",
            Self::RotaryMix => "Rotary Mix",
            Self::EnsembleMix => "Ensemble Mix",
            Self::ResonatorMix => "Resonator Mix",
            Self::BonsaiDrive => "Bonsai Drive",
        }
    }

    /// Bipolar range for this destination (source * depth * range = offset).
    pub fn range(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Pitch => 24.0,
            Self::FilterCutoff => 130.0,
            Self::FilterResonance => 1.0,
            Self::Amplitude => 1.0,
            Self::Osc1Level | Self::Osc2Level | Self::Osc3Level => 1.0,
            Self::FmCrossDepth => 4.0,
            Self::PulseWidth => 0.5,
            Self::NoiseLevel => 1.0,
            Self::Pan => 1.0,
            Self::Lfo1Rate | Self::Lfo2Rate | Self::Lfo3Rate | Self::Lfo4Rate => 10.0,
            Self::ChorusMix | Self::DelayMix | Self::ReverbMix => 1.0,
            Self::TapeDrive | Self::NeuronDrive => 1.0,
            Self::RingModFreq => 1000.0,
            Self::FreqShiftHz => 500.0,
            Self::PhaserRate | Self::FlangerRate => 5.0,
            Self::WaveShaperDrive | Self::WaveShaperMix => 1.0,
            Self::RotaryMix | Self::EnsembleMix | Self::ResonatorMix | Self::BonsaiDrive => 1.0,
        }
    }

    pub const ALL: &[ModDest] = &[
        Self::None, Self::Pitch, Self::FilterCutoff, Self::FilterResonance,
        Self::Amplitude, Self::Osc1Level, Self::Osc2Level, Self::Osc3Level,
        Self::FmCrossDepth, Self::PulseWidth, Self::NoiseLevel, Self::Pan,
        Self::Lfo1Rate, Self::Lfo2Rate, Self::Lfo3Rate, Self::Lfo4Rate,
        Self::ChorusMix, Self::DelayMix, Self::ReverbMix,
        Self::TapeDrive, Self::NeuronDrive, Self::RingModFreq, Self::FreqShiftHz,
        Self::PhaserRate, Self::FlangerRate,
        Self::WaveShaperDrive, Self::WaveShaperMix,
        Self::RotaryMix, Self::EnsembleMix, Self::ResonatorMix, Self::BonsaiDrive,
    ];
}

#[derive(Clone, Copy)]
pub struct ModSlot {
    pub source: ModSource,
    pub dest: ModDest,
    pub depth: f32,  // -1.0..+1.0 bipolar
}

impl Default for ModSlot {
    fn default() -> Self {
        Self { source: ModSource::None, dest: ModDest::None, depth: 0.0 }
    }
}

/// Accumulated modulation offsets — applied to params before voice processing.
#[derive(Clone, Copy, Default)]
pub struct ModOffsets {
    pub pitch: f32,           // semitones offset
    pub filter_cutoff: f32,   // semitones offset (Surge-style, applied as 2^(s/12) multiplier)
    pub filter_resonance: f32,
    pub amplitude: f32,
    pub osc1_level: f32,
    pub osc2_level: f32,
    pub osc3_level: f32,
    pub fm_cross_depth: f32,
    pub pulse_width: f32,
    pub noise_level: f32,
    pub pan: f32,
    pub lfo1_rate: f32,
    pub lfo2_rate: f32,
    pub lfo3_rate: f32,
    pub lfo4_rate: f32,
    pub chorus_mix: f32,
    pub delay_mix: f32,
    pub reverb_mix: f32,
    pub tape_drive: f32,
    pub neuron_drive: f32,
    pub ring_mod_freq: f32,
    pub freq_shift_hz: f32,
    pub phaser_rate: f32,
    pub flanger_rate: f32,
    pub wave_shaper_drive: f32,
    pub wave_shaper_mix: f32,
    pub rotary_mix: f32,
    pub ensemble_mix: f32,
    pub resonator_mix: f32,
    pub bonsai_drive: f32,
}

/// Sources snapshot — gathered once per tick, consumed by matrix evaluation.
#[derive(Clone, Copy)]
pub struct ModSources {
    pub lfo_outputs: [f32; 4],
    pub amp_env: f32,
    pub filter_env: f32,
    pub mseg_outputs: [f32; 2],
    pub mod_wheel: f32,
    pub aftertouch: f32,
    pub velocity: f32,
    pub key_track: f32,       // -1..+1 centered on C4
    pub step_seq: f32,        // -1..+1 normalized step value
    pub random_bipolar: f32,  // -1..+1 randomized per note-on
    pub random_unipolar: f32, // 0..1 randomized per note-on
    pub alt_bipolar: f32,     // alternates +1/-1 per note-on
    pub alt_unipolar: f32,    // alternates 0/1 per note-on
    pub release_vel: f32,     // last note-off velocity 0..1
    pub pitch_bend: f32,      // pitch bend -1..+1
    pub cc: [f32; 4],         // MIDI CC assignable 1..4
    pub breath: f32,          // CC2, 0..1
    pub expression: f32,      // CC11, 0..1
    pub sustain_pedal: f32,   // CC64: 0.0 or 1.0
    pub lowest_key: f32,       // lowest held note -1..+1 centered on C4
    pub highest_key: f32,      // highest held note -1..+1 centered on C4
    pub latest_key: f32,       // most recent note -1..+1 centered on C4
    pub poly_aftertouch: f32,      // current voice's poly AT value, 0..1
    pub scene_lfo_outputs: [f32; 2], // free-running scene LFOs (never retrigger)
}

const MOD_SOURCE_KEYS: [&str; MOD_SLOTS] = [
    "mod_0_source","mod_1_source","mod_2_source","mod_3_source",
    "mod_4_source","mod_5_source","mod_6_source","mod_7_source",
    "mod_8_source","mod_9_source","mod_10_source","mod_11_source",
    "mod_12_source","mod_13_source","mod_14_source","mod_15_source",
];
const MOD_DEST_KEYS: [&str; MOD_SLOTS] = [
    "mod_0_dest","mod_1_dest","mod_2_dest","mod_3_dest",
    "mod_4_dest","mod_5_dest","mod_6_dest","mod_7_dest",
    "mod_8_dest","mod_9_dest","mod_10_dest","mod_11_dest",
    "mod_12_dest","mod_13_dest","mod_14_dest","mod_15_dest",
];
const MOD_DEPTH_KEYS: [&str; MOD_SLOTS] = [
    "mod_0_depth","mod_1_depth","mod_2_depth","mod_3_depth",
    "mod_4_depth","mod_5_depth","mod_6_depth","mod_7_depth",
    "mod_8_depth","mod_9_depth","mod_10_depth","mod_11_depth",
    "mod_12_depth","mod_13_depth","mod_14_depth","mod_15_depth",
];

#[derive(Clone)]
pub struct ModMatrix {
    pub slots: [ModSlot; MOD_SLOTS],
}

impl Default for ModMatrix {
    fn default() -> Self {
        Self { slots: [ModSlot::default(); MOD_SLOTS] }
    }
}

impl ModMatrix {
    /// Evaluate all active slots. Returns accumulated offsets.
    pub fn evaluate(&self, sources: &ModSources) -> ModOffsets {
        // Precompute all source values indexed by ModSource discriminant (0..32)
        let src_vals: [f32; 33] = [
            0.0,                             // None = 0
            sources.lfo_outputs[0],          // Lfo1 = 1
            sources.lfo_outputs[1],          // Lfo2 = 2
            sources.lfo_outputs[2],          // Lfo3 = 3
            sources.lfo_outputs[3],          // Lfo4 = 4
            sources.amp_env,                 // AmpEnv = 5
            sources.filter_env,              // FilterEnv = 6
            sources.mseg_outputs[0],         // Mseg1 = 7
            sources.mseg_outputs[1],         // Mseg2 = 8
            sources.mod_wheel,               // ModWheel = 9
            sources.aftertouch,              // Aftertouch = 10
            sources.velocity,                // Velocity = 11
            sources.key_track,               // KeyTrack = 12
            sources.step_seq,                // StepSeq = 13
            sources.random_bipolar,          // RandomBipolar = 14
            sources.random_unipolar,         // RandomUnipolar = 15
            sources.alt_bipolar,             // AltBipolar = 16
            sources.alt_unipolar,            // AltUnipolar = 17
            sources.release_vel,             // ReleaseVel = 18
            sources.pitch_bend,              // PitchBend = 19
            sources.cc[0],                   // Cc1 = 20
            sources.cc[1],                   // Cc2 = 21
            sources.cc[2],                   // Cc3 = 22
            sources.cc[3],                   // Cc4 = 23
            sources.breath,                  // Breath = 24
            sources.expression,              // Expression = 25
            sources.sustain_pedal,           // SustainPedal = 26
            sources.lowest_key,              // LowestKey = 27
            sources.highest_key,             // HighestKey = 28
            sources.latest_key,              // LatestKey = 29
            sources.poly_aftertouch,         // PolyAftertouch = 30
            sources.scene_lfo_outputs[0],    // SceneLfo1 = 31
            sources.scene_lfo_outputs[1],    // SceneLfo2 = 32
        ];

        let mut offsets = ModOffsets::default();

        for slot in &self.slots {
            if slot.source == ModSource::None || slot.dest == ModDest::None || slot.depth.abs() < 0.001 {
                continue;
            }

            let source_val = src_vals[slot.source as usize];

            let offset = source_val * slot.depth * slot.dest.range();

            match slot.dest {
                ModDest::None => {},
                ModDest::Pitch => offsets.pitch += offset,
                ModDest::FilterCutoff => offsets.filter_cutoff += offset,
                ModDest::FilterResonance => offsets.filter_resonance += offset,
                ModDest::Amplitude => offsets.amplitude += offset,
                ModDest::Osc1Level => offsets.osc1_level += offset,
                ModDest::Osc2Level => offsets.osc2_level += offset,
                ModDest::Osc3Level => offsets.osc3_level += offset,
                ModDest::FmCrossDepth => offsets.fm_cross_depth += offset,
                ModDest::PulseWidth => offsets.pulse_width += offset,
                ModDest::NoiseLevel => offsets.noise_level += offset,
                ModDest::Pan => offsets.pan += offset,
                ModDest::Lfo1Rate => offsets.lfo1_rate += offset,
                ModDest::Lfo2Rate => offsets.lfo2_rate += offset,
                ModDest::ChorusMix => offsets.chorus_mix += offset,
                ModDest::DelayMix => offsets.delay_mix += offset,
                ModDest::ReverbMix => offsets.reverb_mix += offset,
                ModDest::TapeDrive => offsets.tape_drive += offset,
                ModDest::NeuronDrive => offsets.neuron_drive += offset,
                ModDest::RingModFreq => offsets.ring_mod_freq += offset,
                ModDest::FreqShiftHz => offsets.freq_shift_hz += offset,
                ModDest::PhaserRate => offsets.phaser_rate += offset,
                ModDest::FlangerRate => offsets.flanger_rate += offset,
                ModDest::Lfo3Rate => offsets.lfo3_rate += offset,
                ModDest::Lfo4Rate => offsets.lfo4_rate += offset,
                ModDest::WaveShaperDrive => offsets.wave_shaper_drive += offset,
                ModDest::WaveShaperMix => offsets.wave_shaper_mix += offset,
                ModDest::RotaryMix => offsets.rotary_mix += offset,
                ModDest::EnsembleMix => offsets.ensemble_mix += offset,
                ModDest::ResonatorMix => offsets.resonator_mix += offset,
                ModDest::BonsaiDrive => offsets.bonsai_drive += offset,
            }
        }

        offsets
    }

    /// Compute only the PolyAftertouch contribution to ModOffsets,
    /// scaled by the given aftertouch value. Much cheaper than full evaluate()
    /// since it only iterates slots that use PolyAftertouch as source.
    pub fn evaluate_poly_at_delta(&self, poly_at: f32) -> ModOffsets {
        let mut offsets = ModOffsets::default();
        for slot in &self.slots {
            if slot.source != ModSource::PolyAftertouch || slot.depth.abs() < 0.001 {
                continue;
            }
            let offset = poly_at * slot.depth * slot.dest.range();
            match slot.dest {
                ModDest::None => {},
                ModDest::Pitch => offsets.pitch += offset,
                ModDest::FilterCutoff => offsets.filter_cutoff += offset,
                ModDest::FilterResonance => offsets.filter_resonance += offset,
                ModDest::Amplitude => offsets.amplitude += offset,
                ModDest::Osc1Level => offsets.osc1_level += offset,
                ModDest::Osc2Level => offsets.osc2_level += offset,
                ModDest::Osc3Level => offsets.osc3_level += offset,
                ModDest::FmCrossDepth => offsets.fm_cross_depth += offset,
                ModDest::PulseWidth => offsets.pulse_width += offset,
                ModDest::NoiseLevel => offsets.noise_level += offset,
                ModDest::Pan => offsets.pan += offset,
                ModDest::Lfo1Rate => offsets.lfo1_rate += offset,
                ModDest::Lfo2Rate => offsets.lfo2_rate += offset,
                ModDest::ChorusMix => offsets.chorus_mix += offset,
                ModDest::DelayMix => offsets.delay_mix += offset,
                ModDest::ReverbMix => offsets.reverb_mix += offset,
                ModDest::TapeDrive => offsets.tape_drive += offset,
                ModDest::NeuronDrive => offsets.neuron_drive += offset,
                ModDest::RingModFreq => offsets.ring_mod_freq += offset,
                ModDest::FreqShiftHz => offsets.freq_shift_hz += offset,
                ModDest::PhaserRate => offsets.phaser_rate += offset,
                ModDest::FlangerRate => offsets.flanger_rate += offset,
                ModDest::Lfo3Rate => offsets.lfo3_rate += offset,
                ModDest::Lfo4Rate => offsets.lfo4_rate += offset,
                ModDest::WaveShaperDrive => offsets.wave_shaper_drive += offset,
                ModDest::WaveShaperMix => offsets.wave_shaper_mix += offset,
                ModDest::RotaryMix => offsets.rotary_mix += offset,
                ModDest::EnsembleMix => offsets.ensemble_mix += offset,
                ModDest::ResonatorMix => offsets.resonator_mix += offset,
                ModDest::BonsaiDrive => offsets.bonsai_drive += offset,
            }
        }
        offsets
    }

    /// Load from flat param map (preset serialization).
    pub fn load_from_params(&mut self, params: &std::collections::BTreeMap<String, f32>) {
        for i in 0..MOD_SLOTS {
            let src = params.get(MOD_SOURCE_KEYS[i]).copied().unwrap_or(0.0);
            let dst = params.get(MOD_DEST_KEYS[i]).copied().unwrap_or(0.0);
            let depth = params.get(MOD_DEPTH_KEYS[i]).copied().unwrap_or(0.0);
            self.slots[i] = ModSlot {
                source: ModSource::from_param(src),
                dest: ModDest::from_param(dst),
                depth,
            };
        }
    }

    /// Save to flat param map.
    #[allow(dead_code)]
    pub fn save_to_params(&self, params: &mut std::collections::BTreeMap<String, f32>) {
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.source != ModSource::None && slot.dest != ModDest::None {
                params.insert(MOD_SOURCE_KEYS[i].to_string(), slot.source as u8 as f32);
                params.insert(MOD_DEST_KEYS[i].to_string(), slot.dest as u8 as f32);
                params.insert(MOD_DEPTH_KEYS[i].to_string(), slot.depth);
            }
        }
    }
}
