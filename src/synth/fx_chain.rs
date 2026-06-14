/// 16-slot configurable FX chain (Surge XT style).
/// Each slot selects an effect type, has a mix and up to 4 parameters.
/// Serialized as `fx{i}_type`, `fx{i}_en`, `fx{i}_mix`, `fx{i}_p0..p3` in patch BTreeMap.

pub const FX_SLOTS: usize = 16;

// ── Slot type ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
#[repr(u8)]
pub enum FxSlotType {
    None         = 0,
    // Saturation / Drive
    Overdrive    = 1,
    Tape         = 2,
    Neuron       = 3,
    Bonsai       = 4,
    WaveShaper   = 5,
    Airwindows   = 6,
    // Modulation
    Chorus       = 7,
    BbdEnsemble  = 8,
    Flanger      = 9,
    Phaser       = 10,
    Tremolo      = 11,
    Rotary       = 12,
    // Time-based
    Delay        = 13,
    FloatyDelay  = 14,
    // Reverb
    Reverb       = 15,
    Reverb2      = 16,
    SpringReverb = 17,
    ConvReverb   = 18,
    Nimbus       = 19,
    // Spectral / Special
    RingMod      = 20,
    FreqShift    = 21,
    Bitcrusher   = 22,
    Resonator    = 23,
    Combulator   = 24,
    Treemonster  = 25,
    Vocoder      = 26,
    // Utility
    GraphicEq    = 27,
    Compressor   = 28,
    MsTool       = 29,
    Conditioner  = 30,
    Exciter      = 31,
    // Filters (inter-filter WS already per-voice)
}

impl FxSlotType {
    pub fn name(self) -> &'static str {
        match self {
            Self::None         => "---",
            Self::Overdrive    => "Overdrive",
            Self::Tape         => "Tape",
            Self::Neuron       => "Neuron Dist",
            Self::Bonsai       => "Bonsai Sat",
            Self::WaveShaper   => "WaveShaper",
            Self::Airwindows   => "Airwindows",
            Self::Chorus       => "Chorus",
            Self::BbdEnsemble  => "BBD Ensemble",
            Self::Flanger      => "Flanger",
            Self::Phaser       => "Phaser",
            Self::Tremolo      => "Tremolo",
            Self::Rotary       => "Rotary",
            Self::Delay        => "Delay",
            Self::FloatyDelay  => "Floaty Delay",
            Self::Reverb       => "Reverb (Plate)",
            Self::Reverb2      => "Reverb2 (FDN)",
            Self::SpringReverb => "Spring Reverb",
            Self::ConvReverb   => "Conv Reverb",
            Self::Nimbus       => "Nimbus Granular",
            Self::RingMod      => "Ring Mod",
            Self::FreqShift    => "Freq Shift",
            Self::Bitcrusher   => "Bitcrusher",
            Self::Resonator    => "Resonator",
            Self::Combulator   => "Combulator",
            Self::Treemonster  => "Treemonster",
            Self::Vocoder      => "Vocoder",
            Self::GraphicEq    => "Graphic EQ",
            Self::Compressor   => "Compressor",
            Self::MsTool       => "MS Tool",
            Self::Conditioner  => "Conditioner",
            Self::Exciter      => "Exciter",
        }
    }

    pub fn from_u8(v: u8) -> Self {
        match v {
            1  => Self::Overdrive,
            2  => Self::Tape,
            3  => Self::Neuron,
            4  => Self::Bonsai,
            5  => Self::WaveShaper,
            6  => Self::Airwindows,
            7  => Self::Chorus,
            8  => Self::BbdEnsemble,
            9  => Self::Flanger,
            10 => Self::Phaser,
            11 => Self::Tremolo,
            12 => Self::Rotary,
            13 => Self::Delay,
            14 => Self::FloatyDelay,
            15 => Self::Reverb,
            16 => Self::Reverb2,
            17 => Self::SpringReverb,
            18 => Self::ConvReverb,
            19 => Self::Nimbus,
            20 => Self::RingMod,
            21 => Self::FreqShift,
            22 => Self::Bitcrusher,
            23 => Self::Resonator,
            24 => Self::Combulator,
            25 => Self::Treemonster,
            26 => Self::Vocoder,
            27 => Self::GraphicEq,
            28 => Self::Compressor,
            29 => Self::MsTool,
            30 => Self::Conditioner,
            31 => Self::Exciter,
            _  => Self::None,
        }
    }

    /// Parameter labels for GUI. `None` = hide that param slot.
    pub fn param_labels(self) -> [Option<&'static str>; 4] {
        match self {
            Self::None         => [None; 4],
            Self::Overdrive    => [Some("Drive"), Some("Tone"), Some("Type 0-3"), None],
            Self::Tape         => [Some("Drive"), Some("Saturation"), Some("Bias"), Some("Tone")],
            Self::Neuron       => [Some("Drive"), Some("Squash"), Some("Stab"), Some("Comb Hz")],
            Self::Bonsai       => [Some("Drive"), Some("Tone"), Some("Asymm"), Some("Mode 0-4")],
            Self::WaveShaper   => [Some("Drive"), Some("Mode 0-24"), Some("Bias"), None],
            Self::Airwindows   => [Some("Mode 0-32"), Some("Drive"), None, None],
            Self::Chorus       => [None, None, None, None],
            Self::BbdEnsemble  => [Some("Depth"), Some("Rate"), None, None],
            Self::Flanger      => [None, None, None, None],
            Self::Phaser       => [None, None, None, None],
            Self::Tremolo      => [None, None, None, None],
            Self::Rotary       => [Some("Speed 0/1"), None, None, None],
            Self::Delay        => [Some("Time L"), Some("Time R"), Some("Feedback"), Some("Filter")],
            Self::FloatyDelay  => [Some("Time"), Some("Feedback"), Some("Wobble"), Some("Rate")],
            Self::Reverb       => [Some("Room"), Some("Damp"), Some("Width"), Some("Pre-Delay")],
            Self::Reverb2      => [Some("Decay"), Some("Damp"), Some("Size"), None],
            Self::SpringReverb => [Some("Size"), Some("Decay"), Some("Reflections"), Some("Damp")],
            Self::ConvReverb   => [Some("Room"), Some("Damp"), Some("Pre-Delay"), None],
            Self::Nimbus       => [Some("Position"), Some("Size"), Some("Pitch"), Some("Density")],
            Self::RingMod      => [Some("Freq Hz"), Some("Shape 0-2"), Some("Bias"), None],
            Self::FreqShift    => [Some("Hz -1..+1"), Some("Feedback"), Some("Delay"), None],
            Self::Bitcrusher   => [None, None, None, None],
            Self::Resonator    => [Some("Freq Hz"), Some("Decay"), None, None],
            Self::Combulator   => [Some("Freq Hz"), Some("Offset2"), Some("Offset3"), Some("Fbk")],
            Self::Treemonster  => [Some("Threshold"), Some("Pitch -1..+1"), Some("Ring"), None],
            Self::Vocoder      => [Some("Env Follow"), Some("Gate"), None, None],
            Self::GraphicEq    => [Some("Low"), Some("Mid"), Some("High"), Some("Output")],
            Self::Compressor   => [None, None, None, None],
            Self::MsTool       => [Some("Mid Gain"), Some("Side Gain"), Some("Rotation"), None],
            Self::Conditioner  => [Some("Bass Cut"), Some("Width"), Some("Threshold"), None],
            Self::Exciter      => [Some("Drive"), Some("Freq"), Some("Presence"), None],
        }
    }

    /// Parameter value ranges (min, max).
    pub fn param_range(self, idx: usize) -> (f32, f32) {
        match (self, idx) {
            (Self::Overdrive,  0) => (0.0, 1.0),
            (Self::Overdrive,  1) => (0.0, 1.0),
            (Self::Overdrive,  2) => (0.0, 3.9),
            (Self::Delay,      0) => (0.01, 2.0),
            (Self::Delay,      1) => (0.01, 2.0),
            (Self::Delay,      2) => (0.0, 0.98),
            (Self::Delay,      3) => (0.0, 1.0),
            (Self::FloatyDelay,0) => (0.01, 1.0),
            (Self::FloatyDelay,1) => (0.0, 0.98),
            (Self::FloatyDelay,2) => (0.0, 1.0),
            (Self::FloatyDelay,3) => (0.0, 1.0),
            (Self::RingMod,    0) => (20.0, 8000.0),
            (Self::RingMod,    1) => (0.0, 2.9),
            (Self::FreqShift,  0) => (-1.0, 1.0),
            (Self::Resonator,  0) => (50.0, 4000.0),
            (Self::Combulator, 0) => (50.0, 4000.0),
            (Self::Combulator, 1) => (0.0, 1.0),
            (Self::Combulator, 2) => (0.0, 1.0),
            (Self::Combulator, 3) => (0.0, 0.95),
            (Self::Nimbus,     2) => (-1.0, 1.0),
            (Self::Airwindows, 0) => (0.0, 32.0),
            (Self::WaveShaper, 1) => (0.0, 24.0),
            (Self::Bonsai,     3) => (0.0, 4.9),
            (Self::MsTool,     0) => (-1.0, 1.0),
            (Self::MsTool,     1) => (-1.0, 1.0),
            (Self::MsTool,     2) => (-90.0, 90.0),
            (Self::Treemonster,1) => (-1.0, 1.0),
            (Self::GraphicEq,  3) => (0.0, 2.0),
            _ => (0.0, 1.0),
        }
    }

    /// Default parameter values.
    pub fn default_params(self) -> [f32; 4] {
        match self {
            Self::Delay        => [0.3, 0.4, 0.4, 0.3],
            Self::FloatyDelay  => [0.25, 0.4, 0.3, 0.5],
            Self::Reverb       => [0.5, 0.5, 1.0, 0.02],
            Self::Reverb2      => [0.5, 0.5, 0.5, 0.5],
            Self::SpringReverb => [0.5, 0.5, 0.5, 0.5],
            Self::ConvReverb   => [0.5, 0.5, 0.02, 0.5],
            Self::Nimbus       => [0.5, 0.5, 0.0, 0.5],
            Self::RingMod      => [440.0, 0.0, 0.5, 0.5],
            Self::FreqShift    => [0.0, 0.0, 0.0, 0.5],
            Self::Resonator    => [440.0, 0.5, 0.5, 0.5],
            Self::Combulator   => [440.0, 0.5, 0.5, 0.5],
            Self::Treemonster  => [0.5, 0.0, 0.5, 0.5],
            Self::Airwindows   => [0.0, 0.5, 0.5, 0.5],
            Self::WaveShaper   => [0.5, 0.0, 0.5, 0.5],
            Self::GraphicEq    => [0.0, 0.0, 0.0, 1.0],
            _ => [0.5, 0.5, 0.5, 0.5],
        }
    }

    /// All variants in display order.
    pub const ALL: &'static [Self] = &[
        Self::None,
        Self::Overdrive, Self::Tape, Self::Neuron, Self::Bonsai,
        Self::WaveShaper, Self::Airwindows,
        Self::Chorus, Self::BbdEnsemble, Self::Flanger, Self::Phaser,
        Self::Tremolo, Self::Rotary,
        Self::Delay, Self::FloatyDelay,
        Self::Reverb, Self::Reverb2, Self::SpringReverb, Self::ConvReverb, Self::Nimbus,
        Self::RingMod, Self::FreqShift, Self::Bitcrusher,
        Self::Resonator, Self::Combulator, Self::Treemonster, Self::Vocoder,
        Self::GraphicEq, Self::Compressor, Self::MsTool, Self::Conditioner, Self::Exciter,
    ];
}

// ── Slot ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct FxSlot {
    pub slot_type: FxSlotType,
    pub enabled:   bool,
    pub mix:       f32,
    pub params:    [f32; 4],
}

impl Default for FxSlot {
    fn default() -> Self {
        Self {
            slot_type: FxSlotType::None,
            enabled:   true,
            mix:       0.0,
            params:    [0.5, 0.5, 0.5, 0.5],
        }
    }
}

// ── Chain ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct FxChain {
    pub slots:  [FxSlot; FX_SLOTS],
    /// True when this chain was loaded from explicit fx* keys (not legacy fallback).
    pub active: bool,
}

impl Default for FxChain {
    fn default() -> Self {
        Self { slots: [FxSlot::default(); FX_SLOTS], active: false }
    }
}

impl FxChain {
    /// Serialize into a preset BTreeMap.
    pub fn to_map(&self, out: &mut std::collections::BTreeMap<String, f32>) {
        for (i, s) in self.slots.iter().enumerate() {
            out.insert(format!("fx{i}_type"), s.slot_type as u8 as f32);
            out.insert(format!("fx{i}_en"),   if s.enabled { 1.0 } else { 0.0 });
            out.insert(format!("fx{i}_mix"),  s.mix);
            for (j, &p) in s.params.iter().enumerate() {
                out.insert(format!("fx{i}_p{j}"), p);
            }
        }
    }

    /// Deserialize from a preset BTreeMap that contains `fx0_type` etc.
    pub fn from_map(map: &std::collections::BTreeMap<String, f32>) -> Self {
        let p = |key: &str, def: f32| map.get(key).copied().unwrap_or(def);
        let mut chain = Self::default();
        chain.active = true;
        for i in 0..FX_SLOTS {
            let ty = FxSlotType::from_u8(p(&format!("fx{i}_type"), 0.0) as u8);
            let defaults = ty.default_params();
            chain.slots[i] = FxSlot {
                slot_type: ty,
                enabled:   p(&format!("fx{i}_en"), 1.0) > 0.5,
                mix:       p(&format!("fx{i}_mix"), 0.0),
                params: [
                    p(&format!("fx{i}_p0"), defaults[0]),
                    p(&format!("fx{i}_p1"), defaults[1]),
                    p(&format!("fx{i}_p2"), defaults[2]),
                    p(&format!("fx{i}_p3"), defaults[3]),
                ],
            };
        }
        chain
    }

    /// Create a new chain pre-populated to replicate the current hardcoded order.
    /// Call this when converting a legacy patch to the new format.
    pub fn from_legacy_preset_active(map: &std::collections::BTreeMap<String, f32>) -> Self {
        let get = |k: &str, d: f32| map.get(k).copied().unwrap_or(d);
        let mut chain = Self::default();
        chain.active = true;

        let mut idx = 0usize;
        let _ = idx; // incremented by macro
        #[allow(unused_assignments)]
        macro_rules! slot {
            ($ty:expr, $mix:expr, $p:expr) => {
                if idx < FX_SLOTS {
                    let mix_val = $mix;
                    if mix_val > 0.0001 {
                        let p: [f32; 4] = $p;
                        chain.slots[idx] = FxSlot { slot_type: $ty, enabled: true, mix: mix_val, params: p };
                        idx += 1;
                    }
                }
            };
        }

        slot!(FxSlotType::Tape,         get("tape_mix",0.0),
              [get("tape_drive",0.0), get("tape_saturation",0.5), get("tape_bias",0.5), get("tape_tone",0.5)]);
        slot!(FxSlotType::Neuron,        get("neuron_mix",0.0),
              [get("neuron_drive",0.0), get("neuron_squash",0.5), get("neuron_stab",0.5), get("neuron_comb_freq",200.0)]);
        slot!(FxSlotType::RingMod,       get("ring_mod_mix",0.0),
              [get("ring_mod_freq",440.0), get("ring_mod_shape",0.0), get("ring_mod_bias",0.5), 0.5]);
        slot!(FxSlotType::FreqShift,     get("freq_shift_mix",0.0),
              [(get("freq_shift_hz",0.0)/1000.0).clamp(-1.0,1.0), get("freq_shift_feedback",0.0), get("freq_shift_delay",0.0), 0.5]);
        slot!(FxSlotType::Bonsai,        get("bonsai_mix",0.0),
              [get("bonsai_drive",0.0), get("bonsai_tone",0.5), get("bonsai_asym",0.0), get("bonsai_mode",0.0)]);
        slot!(FxSlotType::Resonator,     get("resonator_mix",0.0),
              [get("resonator_freq",440.0), get("resonator_decay",0.5), 0.5, 0.5]);
        slot!(FxSlotType::Chorus,        get("chorus_mix",0.0),
              [0.5, 0.5, 0.5, 0.5]);
        slot!(FxSlotType::BbdEnsemble,   get("ensemble_mix",0.0),
              [get("ensemble_depth",0.5), get("ensemble_rate",0.5), 0.5, 0.5]);
        slot!(FxSlotType::Delay,         get("delay_mix",0.0),
              [get("delay_time_l",0.3), get("delay_time_r",0.4), get("delay_feedback",0.4), get("delay_filter",0.3)]);
        slot!(FxSlotType::Reverb,        get("reverb_mix",0.0),
              [get("reverb_room_size",0.5), get("reverb_damping",0.5), get("reverb_width",1.0), get("reverb_pre_delay",0.02)]);
        slot!(FxSlotType::SpringReverb,  get("spring_mix",0.0),
              [get("spring_size",0.5), get("spring_decay",0.5), get("spring_reflections",0.5), get("spring_damping",0.5)]);
        slot!(FxSlotType::Rotary,        get("rotary_mix",0.0),
              [get("rotary_speed",0.0), 0.5, 0.5, 0.5]);
        slot!(FxSlotType::WaveShaper,    get("wave_shaper_mix",0.0),
              [get("wave_shaper_drive",0.5), get("wave_shaper_mode",0.0), get("wave_shaper_bias",0.5), 0.5]);
        slot!(FxSlotType::Reverb2,       get("reverb2_mix",0.0),
              [get("reverb2_decay",0.5), get("reverb2_damping",0.5), get("reverb2_size",0.5), 0.5]);
        slot!(FxSlotType::Airwindows,    get("airwindows_mix",0.0),
              [get("airwindows_mode",0.0), get("airwindows_drive",0.5), 0.5, 0.5]);
        let _ = idx; // suppress unused_assignments lint
        chain
    }
}
