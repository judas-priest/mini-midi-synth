//! Per-block snapshot of FX-relevant params (S4-synth 3c). Move-only out
//! of `mod.rs` — bodies verbatim; visibility widened to `pub(super)` for
//! the parent (`mod.rs`) caller.

use super::part::GlobalParams;
use super::PatchParams;

/// Cached effect parameters — extracted once per block from PatchParams.
#[derive(Clone, Copy)]
pub(super) struct CachedFxParams {
    // Tape
    pub(super) tape_drive: f32, pub(super) tape_saturation: f32, pub(super) tape_bias: f32,
    pub(super) tape_tone: f32, pub(super) tape_speed: f32, pub(super) tape_mix: f32,
    // Neuron
    pub(super) neuron_drive: f32, pub(super) neuron_squash: f32, pub(super) neuron_stab: f32,
    pub(super) neuron_asym: f32, pub(super) neuron_bias: f32, pub(super) neuron_comb_freq: f32,
    pub(super) neuron_comb_sep: f32, pub(super) neuron_mix: f32,
    // Ring mod
    pub(super) ring_mod_freq: f32, pub(super) ring_mod_shape: f32, pub(super) ring_mod_bias: f32,
    pub(super) ring_mod_linear: f32, pub(super) ring_mod_mix: f32,
    // Freq shift
    pub(super) freq_shift_hz: f32, pub(super) freq_shift_feedback: f32,
    pub(super) freq_shift_delay: f32, pub(super) freq_shift_mix: f32,
    // Chorus
    pub(super) chorus_mix: f32,
    // Delay
    pub(super) delay_time_l: f32, pub(super) delay_time_r: f32, pub(super) delay_feedback: f32,
    pub(super) delay_filter: f32, pub(super) delay_ping_pong: f32, pub(super) delay_mix: f32,
    // Reverb
    pub(super) reverb_type: f32, pub(super) reverb_mix: f32,
    pub(super) reverb_room_size: f32, pub(super) reverb_damping: f32,
    pub(super) reverb_width: f32, pub(super) reverb_pre_delay: f32,
    // Spring reverb
    pub(super) spring_size: f32, pub(super) spring_decay: f32, pub(super) spring_reflections: f32,
    pub(super) spring_damping: f32, pub(super) spring_spin: f32, pub(super) spring_chaos: f32,
    pub(super) spring_mix: f32,
    // Rotary Speaker
    pub(super) rotary_speed: f32, pub(super) rotary_mix: f32,
    // BBD Ensemble
    pub(super) ensemble_depth: f32, pub(super) ensemble_rate: f32, pub(super) ensemble_mix: f32,
    // Resonator
    pub(super) resonator_freq: f32, pub(super) resonator_decay: f32, pub(super) resonator_mix: f32,
    // Bonsai
    pub(super) bonsai_drive: f32, pub(super) bonsai_tone: f32, pub(super) bonsai_asym: f32,
    pub(super) bonsai_mode: f32, pub(super) bonsai_mix: f32,
    // WaveShaper
    pub(super) wave_shaper_drive: f32, pub(super) wave_shaper_mode: f32, pub(super) wave_shaper_bias: f32, pub(super) wave_shaper_mix: f32,
    // MS Tool
    pub(super) ms_mid_gain: f32, pub(super) ms_side_gain: f32, pub(super) ms_rotation: f32, pub(super) ms_mix: f32,
    // Graphic EQ
    pub(super) graphic_eq_gains: [f32; 11], pub(super) graphic_eq_output: f32,
    // Conditioner
    pub(super) conditioner_bass_cut: f32, pub(super) conditioner_width: f32, pub(super) conditioner_threshold: f32, pub(super) conditioner_mix: f32,
    // Exciter
    pub(super) exciter_drive: f32, pub(super) exciter_freq: f32, pub(super) exciter_presence: f32, pub(super) exciter_mix: f32,
    // Floaty Delay
    pub(super) floaty_time: f32, pub(super) floaty_feedback: f32, pub(super) floaty_wobble: f32, pub(super) floaty_rate: f32, pub(super) floaty_damp: f32, pub(super) floaty_mix: f32,
    // Reverb2
    pub(super) reverb2_decay: f32, pub(super) reverb2_damping: f32, pub(super) reverb2_size: f32, pub(super) reverb2_mix: f32,
    // Combulator
    pub(super) combulator_freq: f32, pub(super) combulator_offset2: f32, pub(super) combulator_offset3: f32,
    pub(super) combulator_feedback: f32, pub(super) combulator_tone: f32, pub(super) combulator_mix: f32,
    // Treemonster
    pub(super) treemonster_threshold: f32, pub(super) treemonster_shift: f32, pub(super) treemonster_ring_mix: f32, pub(super) treemonster_mix: f32,
    // Nimbus
    pub(super) nimbus_position: f32, pub(super) nimbus_size: f32, pub(super) nimbus_pitch: f32, pub(super) nimbus_density: f32,
    pub(super) nimbus_spread: f32, pub(super) nimbus_texture: f32, pub(super) nimbus_mix: f32,
    // Vocoder
    pub(super) vocoder_env_follow: f32, pub(super) vocoder_gate: f32, pub(super) vocoder_mix: f32,
    // Convolution Reverb
    pub(super) conv_reverb_room: f32, pub(super) conv_reverb_damping: f32, pub(super) conv_reverb_predelay: f32, pub(super) conv_reverb_mix: f32,
    // Airwindows
    pub(super) airwindows_mode: u32, pub(super) airwindows_drive: f32, pub(super) airwindows_mix: f32,
}

impl CachedFxParams {
    pub(super) fn from_preset(p: Option<&PatchParams>, global: &GlobalParams) -> Self {
        // Нет включённой части → PatchParams::default() (один источник
        // дефолтов; прежние 111 unwrap_or-литералов дублировали его).
        let default_params = PatchParams::default();
        let p = p.unwrap_or(&default_params);
        Self {
            tape_drive: p.tape_drive,
            tape_saturation: p.tape_saturation,
            tape_bias: p.tape_bias,
            tape_tone: p.tape_tone,
            tape_speed: p.tape_speed,
            tape_mix: p.tape_mix,
            neuron_drive: p.neuron_drive,
            neuron_squash: p.neuron_squash,
            neuron_stab: p.neuron_stab,
            neuron_asym: p.neuron_asym,
            neuron_bias: p.neuron_bias,
            neuron_comb_freq: p.neuron_comb_freq,
            neuron_comb_sep: p.neuron_comb_sep,
            neuron_mix: p.neuron_mix,
            ring_mod_freq: p.ring_mod_freq,
            ring_mod_shape: p.ring_mod_shape,
            ring_mod_bias: p.ring_mod_bias,
            ring_mod_linear: p.ring_mod_linear,
            ring_mod_mix: p.ring_mod_mix,
            freq_shift_hz: p.freq_shift_hz,
            freq_shift_feedback: p.freq_shift_feedback,
            freq_shift_delay: p.freq_shift_delay,
            freq_shift_mix: p.freq_shift_mix,
            chorus_mix: p.chorus_mix,
            delay_time_l: p.delay_time_l,
            delay_time_r: p.delay_time_r,
            delay_feedback: p.delay_feedback,
            delay_filter: p.delay_filter,
            delay_ping_pong: p.delay_ping_pong,
            delay_mix: global.delay_mix
                .unwrap_or(p.delay_mix),
            reverb_type: p.reverb_type,
            reverb_mix: global.reverb_mix
                .unwrap_or(p.reverb_mix),
            reverb_room_size: p.reverb_room_size,
            reverb_damping: p.reverb_damping,
            reverb_width: p.reverb_width,
            reverb_pre_delay: p.reverb_pre_delay,
            spring_size: p.spring_size,
            spring_decay: p.spring_decay,
            spring_reflections: p.spring_reflections,
            spring_damping: p.spring_damping,
            spring_spin: p.spring_spin,
            spring_chaos: p.spring_chaos,
            spring_mix: p.spring_mix,
            rotary_speed: p.rotary_speed,
            rotary_mix: p.rotary_mix,
            ensemble_depth: p.ensemble_depth,
            ensemble_rate: p.ensemble_rate,
            ensemble_mix: p.ensemble_mix,
            resonator_freq: p.resonator_freq,
            resonator_decay: p.resonator_decay,
            resonator_mix: p.resonator_mix,
            bonsai_drive: p.bonsai_drive,
            bonsai_tone: p.bonsai_tone,
            bonsai_asym: p.bonsai_asym,
            bonsai_mode: p.bonsai_mode,
            bonsai_mix: p.bonsai_mix,
            wave_shaper_drive: p.wave_shaper_drive,
            wave_shaper_mode: p.wave_shaper_mode,
            wave_shaper_bias: p.wave_shaper_bias,
            wave_shaper_mix: p.wave_shaper_mix,
            ms_mid_gain: p.ms_mid_gain,
            ms_side_gain: p.ms_side_gain,
            ms_rotation: p.ms_rotation,
            ms_mix: p.ms_mix,
            graphic_eq_gains: p.graphic_eq_gains,
            graphic_eq_output: p.graphic_eq_output,
            conditioner_bass_cut: p.conditioner_bass_cut,
            conditioner_width: p.conditioner_width,
            conditioner_threshold: p.conditioner_threshold,
            conditioner_mix: p.conditioner_mix,
            exciter_drive: p.exciter_drive,
            exciter_freq: p.exciter_freq,
            exciter_presence: p.exciter_presence,
            exciter_mix: p.exciter_mix,
            floaty_time: p.floaty_time,
            floaty_feedback: p.floaty_feedback,
            floaty_wobble: p.floaty_wobble,
            floaty_rate: p.floaty_rate,
            floaty_damp: p.floaty_damp,
            floaty_mix: p.floaty_mix,
            reverb2_decay: p.reverb2_decay,
            reverb2_damping: p.reverb2_damping,
            reverb2_size: p.reverb2_size,
            reverb2_mix: p.reverb2_mix,
            combulator_freq: p.combulator_freq,
            combulator_offset2: p.combulator_offset2,
            combulator_offset3: p.combulator_offset3,
            combulator_feedback: p.combulator_feedback,
            combulator_tone: p.combulator_tone,
            combulator_mix: p.combulator_mix,
            treemonster_threshold: p.treemonster_threshold,
            treemonster_shift: p.treemonster_shift,
            treemonster_ring_mix: p.treemonster_ring_mix,
            treemonster_mix: p.treemonster_mix,
            nimbus_position: p.nimbus_position,
            nimbus_size: p.nimbus_size,
            nimbus_pitch: p.nimbus_pitch,
            nimbus_density: p.nimbus_density,
            nimbus_spread: p.nimbus_spread,
            nimbus_texture: p.nimbus_texture,
            nimbus_mix: p.nimbus_mix,
            vocoder_env_follow: p.vocoder_env_follow,
            vocoder_gate: p.vocoder_gate,
            vocoder_mix: p.vocoder_mix,
            conv_reverb_room: p.conv_reverb_room,
            conv_reverb_damping: p.conv_reverb_damping,
            conv_reverb_predelay: p.conv_reverb_predelay,
            conv_reverb_mix: p.conv_reverb_mix,
            airwindows_mode: p.airwindows_mode as u32,
            airwindows_drive: p.airwindows_drive,
            airwindows_mix: p.airwindows_mix,
        }
    }
}
