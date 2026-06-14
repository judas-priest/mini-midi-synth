/// Patch parameter struct — all per-voice/per-patch synth parameters as f32 fields.

use super::fx_chain::FxChain;

#[derive(Clone, Copy)]
pub struct PatchParams {
    pub(super) osc_type: f32, pub(super) osc_detune: f32, pub(super) fm_ratio: f32, pub(super) fm_index: f32, pub(super) fm_env_amount: f32,
    pub(super) osc_count: f32, pub(super) osc1_level: f32,
    pub(super) osc2_type: f32, pub(super) osc2_detune: f32, pub(super) osc2_level: f32,
    pub(super) osc3_type: f32, pub(super) osc3_detune: f32, pub(super) osc3_level: f32,
    pub(super) filter_cutoff: f32, pub(super) filter_resonance: f32, pub(super) filter_type: f32,
    pub(super) filter_env_amount: f32, pub(super) filter_key_track: f32,
    pub(super) filter_routing: f32, pub(super) filter2_type: f32, pub(super) filter2_cutoff: f32, pub(super) filter2_resonance: f32,
    pub(super) noise_level: f32,
    pub(super) amp_attack: f32, pub(super) amp_decay: f32, pub(super) amp_sustain: f32, pub(super) amp_release: f32,
    pub(super) amp_hold: f32,
    pub(super) filter_attack: f32, pub(super) filter_decay: f32, pub(super) filter_sustain: f32, pub(super) filter_release: f32,
    pub(super) filter_hold: f32,
    pub(super) ks_brightness: f32, pub(super) ks_feedback: f32, pub(super) organ_drawbars: [f32; 9],
    pub(super) formant_voice: f32, pub(super) formant_vowel: f32,
    pub(super) drum_pitch_amount: f32, pub(super) drum_pitch_decay: f32, pub(super) drum_noise_level: f32,
    pub(super) drum_noise_decay: f32, pub(super) drum_noise_color: f32,
    pub(super) bass_style: f32, pub(super) bass_tone: f32, pub(super) bass_body: f32, pub(super) bass_pickup: f32,
    pub(super) bow_pressure: f32, pub(super) bow_position: f32, pub(super) body_type: f32,
    pub(super) lip_tension: f32, pub(super) blowing_pressure: f32, pub(super) bell_type: f32,
    // Phase Distortion
    pub(super) pd_shape: f32, pub(super) pd_depth: f32, pub(super) pd_env_amount: f32,
    // Wavefolder
    pub(super) fold_amount: f32, pub(super) fold_symmetry: f32, pub(super) fold_source: f32,
    // Modal Resonator
    pub(super) modal_material: f32, pub(super) modal_brightness: f32, pub(super) modal_damping: f32, pub(super) modal_strike_pos: f32,
    // Hard Sync
    pub(super) sync_ratio: f32, pub(super) sync_shape: f32,
    // Supersaw
    pub(super) supersaw_detune: f32, pub(super) supersaw_mix: f32,
    pub(super) pulse_width: f32,
    // Accordion
    pub accordion_register: f32, pub accordion_bellows: f32,
    // Saxophone
    pub sax_reed_stiffness: f32, pub sax_embouchure: f32, pub sax_blow_pressure: f32, pub sax_type: f32,
    // Electric Piano
    pub epiano_type: f32,
    pub(super) velocity_curve: f32, pub(super) vel_to_filter: f32,
    pub(super) lfo_waveform: f32, pub(super) lfo_rate: f32, pub(super) lfo_pitch_depth: f32, pub(super) lfo_filter_depth: f32, pub(super) lfo_amp_depth: f32,
    // LFO 2
    pub(super) lfo2_waveform: f32, pub(super) lfo2_rate: f32, pub(super) lfo2_pitch_depth: f32, pub(super) lfo2_filter_depth: f32, pub(super) lfo2_amp_depth: f32, pub(super) lfo2_deform: f32,
    // LFO 3 & 4 (routed via mod matrix only)
    pub(super) lfo3_waveform: f32, pub(super) lfo3_rate: f32, pub(super) lfo3_deform: f32,
    pub(super) lfo4_waveform: f32, pub(super) lfo4_rate: f32, pub(super) lfo4_deform: f32,
    // LFO tempo sync & unipolar (all 4 LFOs)
    pub(super) lfo1_tempo_sync: f32, pub(super) lfo2_tempo_sync: f32, pub(super) lfo3_tempo_sync: f32, pub(super) lfo4_tempo_sync: f32,
    pub(super) lfo1_unipolar: f32, pub(super) lfo2_unipolar: f32, pub(super) lfo3_unipolar: f32, pub(super) lfo4_unipolar: f32,
    pub(super) portamento_time: f32, pub(super) portamento_mode: f32,
    pub(super) pitch_bend_up: f32,   // 0 = use global pitch_bend_range
    pub(super) pitch_bend_down: f32,
    pub play_mode: f32,     // 0=poly, 1=mono, 2=mono-st, 3=latch
    pub sustain_mode: f32,  // 0=hold all notes, 1=release if others held, 4=poly-high, 5=poly-low, 6=piano
    pub(super) unison_voices: f32, pub(super) unison_detune: f32, pub(super) unison_spread: f32,
    // Envelope shapes
    pub(super) env_attack_shape: f32, pub(super) env_decay_shape: f32,
    pub(super) env_release_shape: f32,
    pub(super) filter_env_attack_shape: f32, pub(super) filter_env_decay_shape: f32, pub(super) filter_env_release_shape: f32,
    // LFO retrigger flags
    pub(super) lfo1_retrigger: f32, pub(super) lfo2_retrigger: f32, pub(super) lfo3_retrigger: f32, pub(super) lfo4_retrigger: f32,
    pub(super) lfo1_trigger_mode: f32, pub(super) lfo2_trigger_mode: f32, pub(super) lfo3_trigger_mode: f32, pub(super) lfo4_trigger_mode: f32,
    // LFO deform
    pub(super) lfo_deform: f32,
    // Scene LFOs (free-running — never retrigger on note-on)
    pub(super) slfo1_rate: f32, pub(super) slfo1_waveform: f32, pub(super) slfo1_deform: f32,
    pub(super) slfo1_tempo_sync: f32, pub(super) slfo1_unipolar: f32,
    pub(super) slfo2_rate: f32, pub(super) slfo2_waveform: f32, pub(super) slfo2_deform: f32,
    pub(super) slfo2_tempo_sync: f32, pub(super) slfo2_unipolar: f32,
    // Macro knobs (8 named user-controllable mod sources, 0..1)
    pub(super) macro_vals: [f32; 8],
    // Alias oscillator
    pub(super) alias_wave_type: f32, pub(super) alias_crush: f32,
    // Window oscillator
    pub(super) window_type: f32, pub(super) window_morph: f32, pub(super) window_formant: f32,
    // Twist / Plaits
    pub(super) twist_engine: f32, pub(super) twist_harmonics: f32, pub(super) twist_timbre: f32, pub(super) twist_morph: f32,
    pub(super) twist_lpg_decay: f32, pub(super) twist_lpg_colour: f32, pub(super) twist_aux_mix: f32,
    // Effects (per-patch)
    pub(super) chorus_mix: f32,
    pub(super) delay_mix: f32, pub(super) delay_time_l: f32, pub(super) delay_time_r: f32,
    pub(super) delay_feedback: f32, pub(super) delay_ping_pong: f32, pub(super) delay_filter: f32,
    pub(super) reverb_mix: f32, pub(super) reverb_room_size: f32, pub(super) reverb_damping: f32,
    pub(super) reverb_width: f32, pub(super) reverb_pre_delay: f32,
    // Ring Modulator
    pub(super) ring_mod_freq: f32, pub(super) ring_mod_shape: f32, pub(super) ring_mod_bias: f32,
    pub(super) ring_mod_linear: f32, pub(super) ring_mod_mix: f32,
    // Frequency Shifter
    pub(super) freq_shift_hz: f32, pub(super) freq_shift_feedback: f32, pub(super) freq_shift_delay: f32,
    pub(super) freq_shift_mix: f32,
    // Tape Saturation
    pub(super) tape_drive: f32, pub(super) tape_saturation: f32, pub(super) tape_bias: f32,
    pub(super) tape_tone: f32, pub(super) tape_speed: f32, pub(super) tape_mix: f32,
    // Neuron Distortion
    pub(super) neuron_drive: f32, pub(super) neuron_squash: f32, pub(super) neuron_stab: f32,
    pub(super) neuron_asym: f32, pub(super) neuron_bias: f32,
    pub(super) neuron_comb_freq: f32, pub(super) neuron_comb_sep: f32, pub(super) neuron_mix: f32,
    // Spring Reverb
    pub(super) spring_size: f32, pub(super) spring_decay: f32, pub(super) spring_reflections: f32,
    pub(super) spring_damping: f32, pub(super) spring_spin: f32, pub(super) spring_chaos: f32, pub(super) spring_mix: f32,
    // Reverb type (0=plate, 1=spring)
    pub(super) reverb_type: f32,
    // FM cross-routing (osc1 → osc2/3 frequency modulation)
    pub(super) fm_cross_depth: f32,
    // MSEG
    #[allow(dead_code)]
    pub(super) mseg_enabled: f32,
    // Step sequencer pitch contribution (1.0 = normal, 0.0 = use seq only for mod matrix)
    pub(super) seq_pitch_depth: f32,
    // Note range for split/part mode (0..127)
    pub min_note: f32,
    pub max_note: f32,
    // Filter env amount in semitones (Surge-style, applied exponentially)
    pub filter_env_semitones: f32,
    // Rotary Speaker / Leslie
    pub(super) rotary_speed: f32, pub(super) rotary_mix: f32,
    // BBD Ensemble chorus
    pub(super) ensemble_depth: f32, pub(super) ensemble_rate: f32, pub(super) ensemble_mix: f32,
    // Resonator bank
    pub(super) resonator_freq: f32, pub(super) resonator_decay: f32, pub(super) resonator_mix: f32,
    // Bonsai saturation
    pub(super) bonsai_drive: f32, pub(super) bonsai_tone: f32, pub(super) bonsai_asym: f32,
    pub(super) bonsai_mode: f32, pub(super) bonsai_mix: f32,
    // WaveShaper (FX chain)
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
    // Reverb2 (FDN)
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
    // Osc Waveshaper (per-voice, pre-filter)
    pub(super) osc_ws_mode: f32, pub(super) osc_ws_drive: f32, pub(super) osc_ws_mix: f32,
    // Inter-filter waveshaper (between F1 and F2 in Serial routing)
    pub(super) inter_ws_mode: f32, pub(super) inter_ws_drive: f32, pub(super) inter_ws_mix: f32,
    // SVF Morph filter parameter (0=LP, 0.5=BP, 1=HP)
    pub svf_morph: f32,
    // Polivoks filter parameters
    pub filter_drive: f32,
    pub filter_starve: f32,
    // Airwindows
    pub(super) airwindows_mode: f32,
    pub(super) airwindows_drive: f32,
    pub(super) airwindows_mix: f32,
    // Arpeggiator
    pub(super) arp_enabled: f32,
    pub(super) arp_mode: f32,
    pub(super) arp_rate: f32,
    pub(super) arp_octaves: f32,
    pub(super) arp_gate: f32,
    // 16-slot FX chain (new style — takes precedence when active)
    pub fx_chain: FxChain,
}

impl Default for PatchParams {
    fn default() -> Self {
        Self {
            osc_type: 0.0, osc_detune: 0.0, fm_ratio: 3.5, fm_index: 5.0, fm_env_amount: 0.0,
            osc_count: 1.0, osc1_level: 1.0,
            osc2_type: 1.0, osc2_detune: 0.0, osc2_level: 0.0,
            osc3_type: 1.0, osc3_detune: 0.0, osc3_level: 0.0,
            filter_cutoff: 8000.0, filter_resonance: 0.0, filter_type: 0.0,
            filter_env_amount: 0.0, filter_key_track: 0.0,
            filter_routing: 0.0, filter2_type: 0.0, filter2_cutoff: 8000.0, filter2_resonance: 0.0,
            noise_level: 0.0,
            amp_attack: 0.01, amp_decay: 0.1, amp_sustain: 0.7, amp_release: 0.3, amp_hold: 0.0,
            filter_attack: 0.01, filter_decay: 0.2, filter_sustain: 0.5, filter_release: 0.3, filter_hold: 0.0,
            ks_brightness: 0.5, ks_feedback: 0.996,
            organ_drawbars: [0.0, 0.0, 8.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            formant_voice: 0.0, formant_vowel: 0.0,
            drum_pitch_amount: 24.0, drum_pitch_decay: 40.0, drum_noise_level: 0.5,
            drum_noise_decay: 40.0, drum_noise_color: 0.5,
            bass_style: 0.0, bass_tone: 0.5, bass_body: 0.3, bass_pickup: 0.0,
            bow_pressure: 0.5, bow_position: 0.12, body_type: 0.0,
            lip_tension: 0.5, blowing_pressure: 0.5, bell_type: 0.0,
            pd_shape: 0.0, pd_depth: 0.5, pd_env_amount: 0.0,
            fold_amount: 0.5, fold_symmetry: 0.5, fold_source: 0.0,
            modal_material: 0.0, modal_brightness: 0.5, modal_damping: 0.3, modal_strike_pos: 0.5,
            sync_ratio: 2.0, sync_shape: 0.0,
            supersaw_detune: 0.5, supersaw_mix: 0.5,
            pulse_width: 0.5,
            accordion_register: 0.0, accordion_bellows: 0.7,
            sax_reed_stiffness: 0.5, sax_embouchure: 0.5, sax_blow_pressure: 0.6, sax_type: 1.0,
            epiano_type: 0.0,
            velocity_curve: 0.0, vel_to_filter: 0.0,
            env_attack_shape: 0.0, env_decay_shape: 0.0,
            env_release_shape: 0.0,
            filter_env_attack_shape: 0.0, filter_env_decay_shape: 0.0, filter_env_release_shape: 0.0,
            lfo1_retrigger: 1.0, lfo2_retrigger: 1.0, lfo3_retrigger: 0.0, lfo4_retrigger: 0.0,
            lfo1_trigger_mode: 1.0, lfo2_trigger_mode: 1.0, lfo3_trigger_mode: 0.0, lfo4_trigger_mode: 0.0,
            lfo_waveform: 0.0, lfo_rate: 5.0, lfo_pitch_depth: 0.0, lfo_filter_depth: 0.0, lfo_amp_depth: 0.0,
            lfo_deform: 0.0,
            lfo2_waveform: 0.0, lfo2_rate: 5.0, lfo2_pitch_depth: 0.0, lfo2_filter_depth: 0.0, lfo2_amp_depth: 0.0, lfo2_deform: 0.0,
            lfo3_waveform: 0.0, lfo3_rate: 3.0, lfo3_deform: 0.0,
            lfo4_waveform: 0.0, lfo4_rate: 1.0, lfo4_deform: 0.0,
            lfo1_tempo_sync: 0.0, lfo2_tempo_sync: 0.0, lfo3_tempo_sync: 0.0, lfo4_tempo_sync: 0.0,
            lfo1_unipolar: 0.0, lfo2_unipolar: 0.0, lfo3_unipolar: 0.0, lfo4_unipolar: 0.0,
            slfo1_rate: 0.5, slfo1_waveform: 0.0, slfo1_deform: 0.0, slfo1_tempo_sync: 0.0, slfo1_unipolar: 0.0,
            slfo2_rate: 0.25, slfo2_waveform: 0.0, slfo2_deform: 0.0, slfo2_tempo_sync: 0.0, slfo2_unipolar: 0.0,
            macro_vals: [0.0; 8],
            alias_wave_type: 0.0, alias_crush: 8.0,
            window_type: 0.0, window_morph: 0.0, window_formant: 0.0,
            twist_engine: 0.0, twist_harmonics: 0.5, twist_timbre: 0.5, twist_morph: 0.5,
            twist_lpg_decay: 0.5, twist_lpg_colour: 0.5, twist_aux_mix: 0.0,
            portamento_time: 0.0, portamento_mode: 0.0,
            pitch_bend_up: 0.0, pitch_bend_down: 0.0,
            play_mode: 0.0, sustain_mode: 0.0,
            unison_voices: 1.0, unison_detune: 0.0, unison_spread: 0.0,
            chorus_mix: 0.0,
            delay_mix: 0.0, delay_time_l: 0.3, delay_time_r: 0.4,
            delay_feedback: 0.4, delay_ping_pong: 0.0, delay_filter: 0.3,
            reverb_mix: 0.0, reverb_room_size: 0.5, reverb_damping: 0.5,
            reverb_width: 1.0, reverb_pre_delay: 0.02,
            ring_mod_freq: 440.0, ring_mod_shape: 0.0, ring_mod_bias: 0.5,
            ring_mod_linear: 0.5, ring_mod_mix: 0.0,
            freq_shift_hz: 0.0, freq_shift_feedback: 0.0, freq_shift_delay: 0.0,
            freq_shift_mix: 0.0,
            tape_drive: 0.0, tape_saturation: 0.5, tape_bias: 0.5,
            tape_tone: 0.5, tape_speed: 0.5, tape_mix: 0.0,
            neuron_drive: 0.0, neuron_squash: 0.5, neuron_stab: 0.5,
            neuron_asym: 0.0, neuron_bias: 0.5,
            neuron_comb_freq: 200.0, neuron_comb_sep: 0.5, neuron_mix: 0.0,
            spring_size: 0.5, spring_decay: 0.5, spring_reflections: 0.5,
            spring_damping: 0.5, spring_spin: 0.3, spring_chaos: 0.0, spring_mix: 0.0,
            reverb_type: 0.0,
            fm_cross_depth: 0.0,
            mseg_enabled: 0.0,
            seq_pitch_depth: 1.0,
            min_note: 0.0,
            max_note: 127.0,
            filter_env_semitones: 0.0,
            rotary_speed: 0.0, rotary_mix: 0.0,
            ensemble_depth: 0.5, ensemble_rate: 0.5, ensemble_mix: 0.0,
            resonator_freq: 440.0, resonator_decay: 0.7, resonator_mix: 0.0,
            bonsai_drive: 0.5, bonsai_tone: 0.5, bonsai_asym: 0.0,
            bonsai_mode: 0.0, bonsai_mix: 0.0,
            wave_shaper_drive: 0.5, wave_shaper_mode: 0.0, wave_shaper_bias: 0.0, wave_shaper_mix: 0.0,
            ms_mid_gain: 1.0, ms_side_gain: 1.0, ms_rotation: 0.0, ms_mix: 0.0,
            graphic_eq_gains: [0.0; 11], graphic_eq_output: 1.0,
            conditioner_bass_cut: 20.0, conditioner_width: 1.0, conditioner_threshold: 1.0, conditioner_mix: 0.0,
            exciter_drive: 0.5, exciter_freq: 3000.0, exciter_presence: 0.5, exciter_mix: 0.0,
            floaty_time: 0.3, floaty_feedback: 0.3, floaty_wobble: 0.3, floaty_rate: 0.5, floaty_damp: 0.5, floaty_mix: 0.0,
            reverb2_decay: 0.5, reverb2_damping: 0.5, reverb2_size: 0.5, reverb2_mix: 0.0,
            combulator_freq: 440.0, combulator_offset2: 5.0, combulator_offset3: -7.0,
            combulator_feedback: 0.5, combulator_tone: 0.5, combulator_mix: 0.0,
            treemonster_threshold: 0.5, treemonster_shift: 0.0, treemonster_ring_mix: 0.5, treemonster_mix: 0.0,
            nimbus_position: 0.5, nimbus_size: 0.5, nimbus_pitch: 0.0, nimbus_density: 0.5,
            nimbus_spread: 0.5, nimbus_texture: 0.5, nimbus_mix: 0.0,
            vocoder_env_follow: 0.5, vocoder_gate: 0.0, vocoder_mix: 0.0,
            conv_reverb_room: 0.5, conv_reverb_damping: 0.5, conv_reverb_predelay: 0.02, conv_reverb_mix: 0.0,
            osc_ws_mode: 0.0, osc_ws_drive: 0.5, osc_ws_mix: 0.0,
            inter_ws_mode: 0.0, inter_ws_drive: 0.5, inter_ws_mix: 0.0,
            svf_morph: 0.0,
            filter_drive: 0.0,
            filter_starve: 0.0,
            airwindows_mode: 0.0, airwindows_drive: 0.5, airwindows_mix: 0.0,
            arp_enabled: 0.0, arp_mode: 0.0, arp_rate: 1.0, arp_octaves: 1.0, arp_gate: 0.5,
            fx_chain: FxChain::default(),
        }
    }
}

impl PatchParams {
    pub fn from_map(params: &std::collections::BTreeMap<String, f32>) -> Self {
        let p = |key: &str, default: f32| -> f32 {
            params.get(key).copied().unwrap_or(default)
        };
        Self {
            osc_type: p("osc_type", 0.0), osc_detune: p("osc_detune", 0.0),
            fm_ratio: p("fm_ratio", 3.5), fm_index: p("fm_index", 5.0),
            fm_env_amount: p("fm_env_amount", 0.0),
            osc_count: p("osc_count", 1.0), osc1_level: p("osc1_level", 1.0),
            osc2_type: p("osc2_type", 1.0), osc2_detune: p("osc2_detune", 0.0), osc2_level: p("osc2_level", 0.0),
            osc3_type: p("osc3_type", 1.0), osc3_detune: p("osc3_detune", 0.0), osc3_level: p("osc3_level", 0.0),
            filter_cutoff: p("filter_cutoff", 8000.0), filter_resonance: p("filter_resonance", 0.0),
            filter_type: p("filter_type", 0.0), filter_env_amount: p("filter_env_amount", 0.0),
            filter_key_track: p("filter_key_track", 0.0),
            filter_routing: p("filter_routing", 0.0), filter2_type: p("filter2_type", 0.0),
            filter2_cutoff: p("filter2_cutoff", 8000.0), filter2_resonance: p("filter2_resonance", 0.0),
            noise_level: p("noise_level", 0.0),
            amp_attack: p("amp_attack", 0.01), amp_decay: p("amp_decay", 0.1),
            amp_sustain: p("amp_sustain", 0.7), amp_release: p("amp_release", 0.3),
            amp_hold: p("amp_hold", 0.0),
            filter_attack: p("filter_attack", 0.01), filter_decay: p("filter_decay", 0.2),
            filter_sustain: p("filter_sustain", 0.5), filter_release: p("filter_release", 0.3),
            filter_hold: p("filter_hold", 0.0),
            ks_brightness: p("ks_brightness", 0.5), ks_feedback: p("ks_feedback", 0.996),
            organ_drawbars: [
                p("drawbar_1", 0.0), p("drawbar_2", 0.0), p("drawbar_3", 8.0),
                p("drawbar_4", 0.0), p("drawbar_5", 0.0), p("drawbar_6", 0.0),
                p("drawbar_7", 0.0), p("drawbar_8", 0.0), p("drawbar_9", 0.0),
            ],
            formant_voice: p("formant_voice", 0.0), formant_vowel: p("formant_vowel", 0.0),
            drum_pitch_amount: p("drum_pitch_amount", 24.0), drum_pitch_decay: p("drum_pitch_decay", 40.0),
            drum_noise_level: p("drum_noise_level", 0.5), drum_noise_decay: p("drum_noise_decay", 40.0),
            drum_noise_color: p("drum_noise_color", 0.5),
            bass_style: p("bass_style", 0.0), bass_tone: p("bass_tone", 0.5), bass_body: p("bass_body", 0.3), bass_pickup: p("bass_pickup", 0.0),
            bow_pressure: p("bow_pressure", 0.5), bow_position: p("bow_position", 0.12), body_type: p("body_type", 0.0),
            lip_tension: p("lip_tension", 0.5), blowing_pressure: p("blowing_pressure", 0.5), bell_type: p("bell_type", 0.0),
            pd_shape: p("pd_shape", 0.0), pd_depth: p("pd_depth", 0.5), pd_env_amount: p("pd_env_amount", 0.0),
            fold_amount: p("fold_amount", 0.5), fold_symmetry: p("fold_symmetry", 0.5), fold_source: p("fold_source", 0.0),
            modal_material: p("modal_material", 0.0), modal_brightness: p("modal_brightness", 0.5),
            modal_damping: p("modal_damping", 0.3), modal_strike_pos: p("modal_strike_pos", 0.5),
            sync_ratio: p("sync_ratio", 2.0), sync_shape: p("sync_shape", 0.0),
            supersaw_detune: p("supersaw_detune", 0.5), supersaw_mix: p("supersaw_mix", 0.5),
            pulse_width: p("pulse_width", 0.5),
            accordion_register: p("accordion_register", 0.0), accordion_bellows: p("accordion_bellows", 0.7),
            sax_reed_stiffness: p("sax_reed_stiffness", 0.5), sax_embouchure: p("sax_embouchure", 0.5),
            sax_blow_pressure: p("sax_blow_pressure", 0.6), sax_type: p("sax_type", 1.0),
            epiano_type: p("epiano_type", 0.0),
            velocity_curve: p("velocity_curve", 0.0), vel_to_filter: p("vel_to_filter", 0.0),
            env_attack_shape: p("env_attack_shape", 0.0), env_decay_shape: p("env_decay_shape", 0.0),
            env_release_shape: p("env_release_shape", 0.0),
            filter_env_attack_shape: p("filter_env_attack_shape", 0.0),
            filter_env_decay_shape: p("filter_env_decay_shape", 0.0),
            filter_env_release_shape: p("filter_env_release_shape", 0.0),
            lfo1_retrigger: p("lfo1_retrigger", 1.0), lfo2_retrigger: p("lfo2_retrigger", 1.0),
            lfo3_retrigger: p("lfo3_retrigger", 0.0), lfo4_retrigger: p("lfo4_retrigger", 0.0),
            // Trigger mode: 0=Free Run, 1=Key, 2=Random, 3=RandomUni. Falls back to retrigger boolean.
            lfo1_trigger_mode: p("lfo1_trigger_mode", if p("lfo1_retrigger", 1.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo2_trigger_mode: p("lfo2_trigger_mode", if p("lfo2_retrigger", 1.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo3_trigger_mode: p("lfo3_trigger_mode", if p("lfo3_retrigger", 0.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo4_trigger_mode: p("lfo4_trigger_mode", if p("lfo4_retrigger", 0.0) > 0.5 { 1.0 } else { 0.0 }),
            lfo_waveform: p("lfo_waveform", 0.0), lfo_rate: p("lfo_rate", 5.0),
            lfo_pitch_depth: p("lfo_pitch_depth", 0.0), lfo_filter_depth: p("lfo_filter_depth", 0.0),
            lfo_amp_depth: p("lfo_amp_depth", 0.0),
            lfo_deform: p("lfo_deform", 0.0),
            lfo2_waveform: p("lfo2_waveform", 0.0), lfo2_rate: p("lfo2_rate", 5.0),
            lfo2_pitch_depth: p("lfo2_pitch_depth", 0.0), lfo2_filter_depth: p("lfo2_filter_depth", 0.0),
            lfo2_amp_depth: p("lfo2_amp_depth", 0.0), lfo2_deform: p("lfo2_deform", 0.0),
            lfo3_waveform: p("lfo3_waveform", 0.0), lfo3_rate: p("lfo3_rate", 3.0), lfo3_deform: p("lfo3_deform", 0.0),
            lfo4_waveform: p("lfo4_waveform", 0.0), lfo4_rate: p("lfo4_rate", 1.0), lfo4_deform: p("lfo4_deform", 0.0),
            lfo1_tempo_sync: p("lfo1_tempo_sync", 0.0), lfo2_tempo_sync: p("lfo2_tempo_sync", 0.0),
            lfo3_tempo_sync: p("lfo3_tempo_sync", 0.0), lfo4_tempo_sync: p("lfo4_tempo_sync", 0.0),
            lfo1_unipolar: p("lfo1_unipolar", 0.0), lfo2_unipolar: p("lfo2_unipolar", 0.0),
            lfo3_unipolar: p("lfo3_unipolar", 0.0), lfo4_unipolar: p("lfo4_unipolar", 0.0),
            slfo1_rate: p("slfo1_rate", 0.5), slfo1_waveform: p("slfo1_waveform", 0.0),
            slfo1_deform: p("slfo1_deform", 0.0), slfo1_tempo_sync: p("slfo1_tempo_sync", 0.0),
            slfo1_unipolar: p("slfo1_unipolar", 0.0),
            slfo2_rate: p("slfo2_rate", 0.25), slfo2_waveform: p("slfo2_waveform", 0.0),
            slfo2_deform: p("slfo2_deform", 0.0), slfo2_tempo_sync: p("slfo2_tempo_sync", 0.0),
            slfo2_unipolar: p("slfo2_unipolar", 0.0),
            macro_vals: [
                p("macro_0", 0.0), p("macro_1", 0.0), p("macro_2", 0.0), p("macro_3", 0.0),
                p("macro_4", 0.0), p("macro_5", 0.0), p("macro_6", 0.0), p("macro_7", 0.0),
            ],
            alias_wave_type: p("alias_wave_type", 0.0), alias_crush: p("alias_crush", 8.0),
            window_type: p("window_type", 0.0), window_morph: p("window_morph", 0.0),
            window_formant: p("window_formant", 0.0),
            twist_engine: p("twist_engine", 0.0), twist_harmonics: p("twist_harmonics", 0.5),
            twist_timbre: p("twist_timbre", 0.5), twist_morph: p("twist_morph", 0.5),
            twist_lpg_decay: p("twist_lpg_decay", 0.5), twist_lpg_colour: p("twist_lpg_colour", 0.5),
            twist_aux_mix: p("twist_aux_mix", 0.0),
            portamento_time: p("portamento_time", 0.0), portamento_mode: p("portamento_mode", 0.0),
            pitch_bend_up: p("pitch_bend_up", 0.0), pitch_bend_down: p("pitch_bend_down", 0.0),
            play_mode: p("play_mode", 0.0), sustain_mode: p("sustain_mode", 0.0),
            unison_voices: p("unison_voices", 1.0), unison_detune: p("unison_detune", 0.0),
            unison_spread: p("unison_spread", 0.0),
            chorus_mix: p("chorus_mix", 0.0),
            delay_mix: p("delay_mix", 0.0), delay_time_l: p("delay_time_l", 0.3), delay_time_r: p("delay_time_r", 0.4),
            delay_feedback: p("delay_feedback", 0.4), delay_ping_pong: p("delay_ping_pong", 0.0), delay_filter: p("delay_filter", 0.3),
            reverb_mix: p("reverb_mix", 0.0), reverb_room_size: p("reverb_room_size", 0.5), reverb_damping: p("reverb_damping", 0.5),
            reverb_width: p("reverb_width", 1.0), reverb_pre_delay: p("reverb_pre_delay", 0.02),
            ring_mod_freq: p("ring_mod_freq", 440.0), ring_mod_shape: p("ring_mod_shape", 0.0),
            ring_mod_bias: p("ring_mod_bias", 0.5), ring_mod_linear: p("ring_mod_linear", 0.5),
            ring_mod_mix: p("ring_mod_mix", 0.0),
            freq_shift_hz: p("freq_shift_hz", 0.0), freq_shift_feedback: p("freq_shift_feedback", 0.0),
            freq_shift_delay: p("freq_shift_delay", 0.0), freq_shift_mix: p("freq_shift_mix", 0.0),
            tape_drive: p("tape_drive", 0.0), tape_saturation: p("tape_saturation", 0.5),
            tape_bias: p("tape_bias", 0.5), tape_tone: p("tape_tone", 0.5),
            tape_speed: p("tape_speed", 0.5), tape_mix: p("tape_mix", 0.0),
            neuron_drive: p("neuron_drive", 0.0), neuron_squash: p("neuron_squash", 0.5),
            neuron_stab: p("neuron_stab", 0.5), neuron_asym: p("neuron_asym", 0.0),
            neuron_bias: p("neuron_bias", 0.5), neuron_comb_freq: p("neuron_comb_freq", 200.0),
            neuron_comb_sep: p("neuron_comb_sep", 0.5), neuron_mix: p("neuron_mix", 0.0),
            spring_size: p("spring_size", 0.5), spring_decay: p("spring_decay", 0.5),
            spring_reflections: p("spring_reflections", 0.5), spring_damping: p("spring_damping", 0.5),
            spring_spin: p("spring_spin", 0.3), spring_chaos: p("spring_chaos", 0.0),
            spring_mix: p("spring_mix", 0.0),
            reverb_type: p("reverb_type", 0.0),
            fm_cross_depth: p("fm_cross_depth", 0.0),
            mseg_enabled: p("mseg_enabled", 0.0),
            seq_pitch_depth: p("seq_pitch_depth", 1.0),
            min_note: p("min_note", 0.0),
            max_note: p("max_note", 127.0),
            filter_env_semitones: p("filter_env_semitones", 0.0),
            rotary_speed: p("rotary_speed", 0.0), rotary_mix: p("rotary_mix", 0.0),
            ensemble_depth: p("ensemble_depth", 0.5), ensemble_rate: p("ensemble_rate", 0.5),
            ensemble_mix: p("ensemble_mix", 0.0),
            resonator_freq: p("resonator_freq", 440.0), resonator_decay: p("resonator_decay", 0.7),
            resonator_mix: p("resonator_mix", 0.0),
            bonsai_drive: p("bonsai_drive", 0.5), bonsai_tone: p("bonsai_tone", 0.5),
            bonsai_asym: p("bonsai_asym", 0.0), bonsai_mode: p("bonsai_mode", 0.0),
            bonsai_mix: p("bonsai_mix", 0.0),
            wave_shaper_drive: p("wave_shaper_drive", 0.5), wave_shaper_mode: p("wave_shaper_mode", 0.0),
            wave_shaper_bias: p("wave_shaper_bias", 0.0), wave_shaper_mix: p("wave_shaper_mix", 0.0),
            ms_mid_gain: p("ms_mid_gain", 1.0), ms_side_gain: p("ms_side_gain", 1.0),
            ms_rotation: p("ms_rotation", 0.0), ms_mix: p("ms_mix", 0.0),
            graphic_eq_gains: [
                p("geq_0", 0.0), p("geq_1", 0.0), p("geq_2", 0.0), p("geq_3", 0.0), p("geq_4", 0.0),
                p("geq_5", 0.0), p("geq_6", 0.0), p("geq_7", 0.0), p("geq_8", 0.0), p("geq_9", 0.0), p("geq_10", 0.0),
            ],
            graphic_eq_output: p("graphic_eq_output", 1.0),
            conditioner_bass_cut: p("conditioner_bass_cut", 20.0), conditioner_width: p("conditioner_width", 1.0),
            conditioner_threshold: p("conditioner_threshold", 1.0), conditioner_mix: p("conditioner_mix", 0.0),
            exciter_drive: p("exciter_drive", 0.5), exciter_freq: p("exciter_freq", 3000.0),
            exciter_presence: p("exciter_presence", 0.5), exciter_mix: p("exciter_mix", 0.0),
            floaty_time: p("floaty_time", 0.3), floaty_feedback: p("floaty_feedback", 0.3),
            floaty_wobble: p("floaty_wobble", 0.3), floaty_rate: p("floaty_rate", 0.5),
            floaty_damp: p("floaty_damp", 0.5), floaty_mix: p("floaty_mix", 0.0),
            reverb2_decay: p("reverb2_decay", 0.5), reverb2_damping: p("reverb2_damping", 0.5),
            reverb2_size: p("reverb2_size", 0.5), reverb2_mix: p("reverb2_mix", 0.0),
            combulator_freq: p("combulator_freq", 440.0), combulator_offset2: p("combulator_offset2", 5.0),
            combulator_offset3: p("combulator_offset3", -7.0), combulator_feedback: p("combulator_feedback", 0.5),
            combulator_tone: p("combulator_tone", 0.5), combulator_mix: p("combulator_mix", 0.0),
            treemonster_threshold: p("treemonster_threshold", 0.5), treemonster_shift: p("treemonster_shift", 0.0),
            treemonster_ring_mix: p("treemonster_ring_mix", 0.5), treemonster_mix: p("treemonster_mix", 0.0),
            nimbus_position: p("nimbus_position", 0.5), nimbus_size: p("nimbus_size", 0.5),
            nimbus_pitch: p("nimbus_pitch", 0.0), nimbus_density: p("nimbus_density", 0.5),
            nimbus_spread: p("nimbus_spread", 0.5), nimbus_texture: p("nimbus_texture", 0.5),
            nimbus_mix: p("nimbus_mix", 0.0),
            vocoder_env_follow: p("vocoder_env_follow", 0.5), vocoder_gate: p("vocoder_gate", 0.0),
            vocoder_mix: p("vocoder_mix", 0.0),
            conv_reverb_room: p("conv_reverb_room", 0.5), conv_reverb_damping: p("conv_reverb_damping", 0.5),
            conv_reverb_predelay: p("conv_reverb_predelay", 0.02), conv_reverb_mix: p("conv_reverb_mix", 0.0),
            osc_ws_mode: p("osc_ws_mode", 0.0), osc_ws_drive: p("osc_ws_drive", 0.5),
            osc_ws_mix: p("osc_ws_mix", 0.0),
            inter_ws_mode: p("inter_ws_mode", 0.0), inter_ws_drive: p("inter_ws_drive", 0.5),
            inter_ws_mix: p("inter_ws_mix", 0.0),
            svf_morph: p("svf_morph", 0.0),
            filter_drive: p("filter_drive", 0.0),
            filter_starve: p("filter_starve", 0.0),
            airwindows_mode: p("airwindows_mode", 0.0),
            airwindows_drive: p("airwindows_drive", 0.5),
            airwindows_mix: p("airwindows_mix", 0.0),
            arp_enabled: p("arp_enabled", 0.0),
            arp_mode: p("arp_mode", 0.0),
            arp_rate: p("arp_rate", 1.0),
            arp_octaves: p("arp_octaves", 1.0),
            arp_gate: p("arp_gate", 0.5),
            fx_chain: if params.contains_key("fx0_type") {
                FxChain::from_map(params)
            } else {
                FxChain::default() // active=false → legacy path in tick_block
            },
        }
    }

    /// Serialize all parameters into a BTreeMap (inverse of `from_map`).
    pub fn to_map(&self) -> std::collections::BTreeMap<String, f32> {
        let mut m = std::collections::BTreeMap::new();
        macro_rules! s {
            ($k:expr, $v:expr) => { m.insert($k.to_string(), $v); };
        }
        s!("osc_type", self.osc_type); s!("osc_detune", self.osc_detune);
        s!("fm_ratio", self.fm_ratio); s!("fm_index", self.fm_index); s!("fm_env_amount", self.fm_env_amount);
        s!("osc_count", self.osc_count); s!("osc1_level", self.osc1_level);
        s!("osc2_type", self.osc2_type); s!("osc2_detune", self.osc2_detune); s!("osc2_level", self.osc2_level);
        s!("osc3_type", self.osc3_type); s!("osc3_detune", self.osc3_detune); s!("osc3_level", self.osc3_level);
        s!("filter_cutoff", self.filter_cutoff); s!("filter_resonance", self.filter_resonance);
        s!("filter_type", self.filter_type); s!("filter_env_amount", self.filter_env_amount);
        s!("filter_key_track", self.filter_key_track);
        s!("filter_routing", self.filter_routing); s!("filter2_type", self.filter2_type);
        s!("filter2_cutoff", self.filter2_cutoff); s!("filter2_resonance", self.filter2_resonance);
        s!("noise_level", self.noise_level);
        s!("amp_attack", self.amp_attack); s!("amp_decay", self.amp_decay);
        s!("amp_sustain", self.amp_sustain); s!("amp_release", self.amp_release); s!("amp_hold", self.amp_hold);
        s!("filter_attack", self.filter_attack); s!("filter_decay", self.filter_decay);
        s!("filter_sustain", self.filter_sustain); s!("filter_release", self.filter_release); s!("filter_hold", self.filter_hold);
        s!("ks_brightness", self.ks_brightness); s!("ks_feedback", self.ks_feedback);
        for (i, &v) in self.organ_drawbars.iter().enumerate() { m.insert(format!("drawbar_{}", i + 1), v); }
        s!("formant_voice", self.formant_voice); s!("formant_vowel", self.formant_vowel);
        s!("drum_pitch_amount", self.drum_pitch_amount); s!("drum_pitch_decay", self.drum_pitch_decay);
        s!("drum_noise_level", self.drum_noise_level); s!("drum_noise_decay", self.drum_noise_decay);
        s!("drum_noise_color", self.drum_noise_color);
        s!("bass_style", self.bass_style); s!("bass_tone", self.bass_tone); s!("bass_body", self.bass_body); s!("bass_pickup", self.bass_pickup);
        s!("bow_pressure", self.bow_pressure); s!("bow_position", self.bow_position); s!("body_type", self.body_type);
        s!("lip_tension", self.lip_tension); s!("blowing_pressure", self.blowing_pressure); s!("bell_type", self.bell_type);
        s!("pd_shape", self.pd_shape); s!("pd_depth", self.pd_depth); s!("pd_env_amount", self.pd_env_amount);
        s!("fold_amount", self.fold_amount); s!("fold_symmetry", self.fold_symmetry); s!("fold_source", self.fold_source);
        s!("modal_material", self.modal_material); s!("modal_brightness", self.modal_brightness);
        s!("modal_damping", self.modal_damping); s!("modal_strike_pos", self.modal_strike_pos);
        s!("sync_ratio", self.sync_ratio); s!("sync_shape", self.sync_shape);
        s!("supersaw_detune", self.supersaw_detune); s!("supersaw_mix", self.supersaw_mix);
        s!("pulse_width", self.pulse_width);
        s!("accordion_register", self.accordion_register); s!("accordion_bellows", self.accordion_bellows);
        s!("sax_reed_stiffness", self.sax_reed_stiffness); s!("sax_embouchure", self.sax_embouchure);
        s!("sax_blow_pressure", self.sax_blow_pressure); s!("sax_type", self.sax_type);
        s!("epiano_type", self.epiano_type);
        s!("velocity_curve", self.velocity_curve); s!("vel_to_filter", self.vel_to_filter);
        s!("env_attack_shape", self.env_attack_shape); s!("env_decay_shape", self.env_decay_shape);
        s!("env_release_shape", self.env_release_shape);
        s!("filter_env_attack_shape", self.filter_env_attack_shape);
        s!("filter_env_decay_shape", self.filter_env_decay_shape);
        s!("filter_env_release_shape", self.filter_env_release_shape);
        s!("lfo1_retrigger", self.lfo1_retrigger); s!("lfo2_retrigger", self.lfo2_retrigger);
        s!("lfo3_retrigger", self.lfo3_retrigger); s!("lfo4_retrigger", self.lfo4_retrigger);
        s!("lfo1_trigger_mode", self.lfo1_trigger_mode); s!("lfo2_trigger_mode", self.lfo2_trigger_mode);
        s!("lfo3_trigger_mode", self.lfo3_trigger_mode); s!("lfo4_trigger_mode", self.lfo4_trigger_mode);
        s!("lfo_waveform", self.lfo_waveform); s!("lfo_rate", self.lfo_rate);
        s!("lfo_pitch_depth", self.lfo_pitch_depth); s!("lfo_filter_depth", self.lfo_filter_depth);
        s!("lfo_amp_depth", self.lfo_amp_depth); s!("lfo_deform", self.lfo_deform);
        s!("lfo2_waveform", self.lfo2_waveform); s!("lfo2_rate", self.lfo2_rate);
        s!("lfo2_pitch_depth", self.lfo2_pitch_depth); s!("lfo2_filter_depth", self.lfo2_filter_depth);
        s!("lfo2_amp_depth", self.lfo2_amp_depth); s!("lfo2_deform", self.lfo2_deform);
        s!("lfo3_waveform", self.lfo3_waveform); s!("lfo3_rate", self.lfo3_rate); s!("lfo3_deform", self.lfo3_deform);
        s!("lfo4_waveform", self.lfo4_waveform); s!("lfo4_rate", self.lfo4_rate); s!("lfo4_deform", self.lfo4_deform);
        s!("lfo1_tempo_sync", self.lfo1_tempo_sync); s!("lfo2_tempo_sync", self.lfo2_tempo_sync);
        s!("lfo3_tempo_sync", self.lfo3_tempo_sync); s!("lfo4_tempo_sync", self.lfo4_tempo_sync);
        s!("lfo1_unipolar", self.lfo1_unipolar); s!("lfo2_unipolar", self.lfo2_unipolar);
        s!("lfo3_unipolar", self.lfo3_unipolar); s!("lfo4_unipolar", self.lfo4_unipolar);
        s!("slfo1_rate", self.slfo1_rate); s!("slfo1_waveform", self.slfo1_waveform);
        s!("slfo1_deform", self.slfo1_deform); s!("slfo1_tempo_sync", self.slfo1_tempo_sync); s!("slfo1_unipolar", self.slfo1_unipolar);
        s!("slfo2_rate", self.slfo2_rate); s!("slfo2_waveform", self.slfo2_waveform);
        s!("slfo2_deform", self.slfo2_deform); s!("slfo2_tempo_sync", self.slfo2_tempo_sync); s!("slfo2_unipolar", self.slfo2_unipolar);
        for (i, &v) in self.macro_vals.iter().enumerate() { m.insert(format!("macro_{i}"), v); }
        s!("alias_wave_type", self.alias_wave_type); s!("alias_crush", self.alias_crush);
        s!("window_type", self.window_type); s!("window_morph", self.window_morph); s!("window_formant", self.window_formant);
        s!("twist_engine", self.twist_engine); s!("twist_harmonics", self.twist_harmonics);
        s!("twist_timbre", self.twist_timbre); s!("twist_morph", self.twist_morph);
        s!("twist_lpg_decay", self.twist_lpg_decay); s!("twist_lpg_colour", self.twist_lpg_colour);
        s!("twist_aux_mix", self.twist_aux_mix);
        s!("portamento_time", self.portamento_time); s!("portamento_mode", self.portamento_mode);
        s!("pitch_bend_up", self.pitch_bend_up); s!("pitch_bend_down", self.pitch_bend_down);
        s!("play_mode", self.play_mode); s!("sustain_mode", self.sustain_mode);
        s!("unison_voices", self.unison_voices); s!("unison_detune", self.unison_detune); s!("unison_spread", self.unison_spread);
        s!("chorus_mix", self.chorus_mix);
        s!("delay_mix", self.delay_mix); s!("delay_time_l", self.delay_time_l); s!("delay_time_r", self.delay_time_r);
        s!("delay_feedback", self.delay_feedback); s!("delay_ping_pong", self.delay_ping_pong); s!("delay_filter", self.delay_filter);
        s!("reverb_mix", self.reverb_mix); s!("reverb_room_size", self.reverb_room_size); s!("reverb_damping", self.reverb_damping);
        s!("reverb_width", self.reverb_width); s!("reverb_pre_delay", self.reverb_pre_delay);
        s!("ring_mod_freq", self.ring_mod_freq); s!("ring_mod_shape", self.ring_mod_shape);
        s!("ring_mod_bias", self.ring_mod_bias); s!("ring_mod_linear", self.ring_mod_linear); s!("ring_mod_mix", self.ring_mod_mix);
        s!("freq_shift_hz", self.freq_shift_hz); s!("freq_shift_feedback", self.freq_shift_feedback);
        s!("freq_shift_delay", self.freq_shift_delay); s!("freq_shift_mix", self.freq_shift_mix);
        s!("tape_drive", self.tape_drive); s!("tape_saturation", self.tape_saturation);
        s!("tape_bias", self.tape_bias); s!("tape_tone", self.tape_tone);
        s!("tape_speed", self.tape_speed); s!("tape_mix", self.tape_mix);
        s!("neuron_drive", self.neuron_drive); s!("neuron_squash", self.neuron_squash);
        s!("neuron_stab", self.neuron_stab); s!("neuron_asym", self.neuron_asym);
        s!("neuron_bias", self.neuron_bias); s!("neuron_comb_freq", self.neuron_comb_freq);
        s!("neuron_comb_sep", self.neuron_comb_sep); s!("neuron_mix", self.neuron_mix);
        s!("spring_size", self.spring_size); s!("spring_decay", self.spring_decay);
        s!("spring_reflections", self.spring_reflections); s!("spring_damping", self.spring_damping);
        s!("spring_spin", self.spring_spin); s!("spring_chaos", self.spring_chaos); s!("spring_mix", self.spring_mix);
        s!("reverb_type", self.reverb_type);
        s!("fm_cross_depth", self.fm_cross_depth);
        s!("mseg_enabled", self.mseg_enabled);
        s!("seq_pitch_depth", self.seq_pitch_depth);
        s!("min_note", self.min_note); s!("max_note", self.max_note);
        s!("filter_env_semitones", self.filter_env_semitones);
        s!("rotary_speed", self.rotary_speed); s!("rotary_mix", self.rotary_mix);
        s!("ensemble_depth", self.ensemble_depth); s!("ensemble_rate", self.ensemble_rate); s!("ensemble_mix", self.ensemble_mix);
        s!("resonator_freq", self.resonator_freq); s!("resonator_decay", self.resonator_decay); s!("resonator_mix", self.resonator_mix);
        s!("bonsai_drive", self.bonsai_drive); s!("bonsai_tone", self.bonsai_tone);
        s!("bonsai_asym", self.bonsai_asym); s!("bonsai_mode", self.bonsai_mode); s!("bonsai_mix", self.bonsai_mix);
        s!("wave_shaper_drive", self.wave_shaper_drive); s!("wave_shaper_mode", self.wave_shaper_mode);
        s!("wave_shaper_bias", self.wave_shaper_bias); s!("wave_shaper_mix", self.wave_shaper_mix);
        s!("ms_mid_gain", self.ms_mid_gain); s!("ms_side_gain", self.ms_side_gain);
        s!("ms_rotation", self.ms_rotation); s!("ms_mix", self.ms_mix);
        for (i, &v) in self.graphic_eq_gains.iter().enumerate() { m.insert(format!("geq_{i}"), v); }
        s!("graphic_eq_output", self.graphic_eq_output);
        s!("conditioner_bass_cut", self.conditioner_bass_cut); s!("conditioner_width", self.conditioner_width);
        s!("conditioner_threshold", self.conditioner_threshold); s!("conditioner_mix", self.conditioner_mix);
        s!("exciter_drive", self.exciter_drive); s!("exciter_freq", self.exciter_freq);
        s!("exciter_presence", self.exciter_presence); s!("exciter_mix", self.exciter_mix);
        s!("floaty_time", self.floaty_time); s!("floaty_feedback", self.floaty_feedback);
        s!("floaty_wobble", self.floaty_wobble); s!("floaty_rate", self.floaty_rate);
        s!("floaty_damp", self.floaty_damp); s!("floaty_mix", self.floaty_mix);
        s!("reverb2_decay", self.reverb2_decay); s!("reverb2_damping", self.reverb2_damping);
        s!("reverb2_size", self.reverb2_size); s!("reverb2_mix", self.reverb2_mix);
        s!("combulator_freq", self.combulator_freq); s!("combulator_offset2", self.combulator_offset2);
        s!("combulator_offset3", self.combulator_offset3); s!("combulator_feedback", self.combulator_feedback);
        s!("combulator_tone", self.combulator_tone); s!("combulator_mix", self.combulator_mix);
        s!("treemonster_threshold", self.treemonster_threshold); s!("treemonster_shift", self.treemonster_shift);
        s!("treemonster_ring_mix", self.treemonster_ring_mix); s!("treemonster_mix", self.treemonster_mix);
        s!("nimbus_position", self.nimbus_position); s!("nimbus_size", self.nimbus_size);
        s!("nimbus_pitch", self.nimbus_pitch); s!("nimbus_density", self.nimbus_density);
        s!("nimbus_spread", self.nimbus_spread); s!("nimbus_texture", self.nimbus_texture); s!("nimbus_mix", self.nimbus_mix);
        s!("vocoder_env_follow", self.vocoder_env_follow); s!("vocoder_gate", self.vocoder_gate); s!("vocoder_mix", self.vocoder_mix);
        s!("conv_reverb_room", self.conv_reverb_room); s!("conv_reverb_damping", self.conv_reverb_damping);
        s!("conv_reverb_predelay", self.conv_reverb_predelay); s!("conv_reverb_mix", self.conv_reverb_mix);
        s!("osc_ws_mode", self.osc_ws_mode); s!("osc_ws_drive", self.osc_ws_drive); s!("osc_ws_mix", self.osc_ws_mix);
        s!("inter_ws_mode", self.inter_ws_mode); s!("inter_ws_drive", self.inter_ws_drive); s!("inter_ws_mix", self.inter_ws_mix);
        s!("svf_morph", self.svf_morph);
        s!("filter_drive", self.filter_drive); s!("filter_starve", self.filter_starve);
        s!("airwindows_mode", self.airwindows_mode); s!("airwindows_drive", self.airwindows_drive); s!("airwindows_mix", self.airwindows_mix);
        s!("arp_enabled", self.arp_enabled); s!("arp_mode", self.arp_mode); s!("arp_rate", self.arp_rate);
        s!("arp_octaves", self.arp_octaves); s!("arp_gate", self.arp_gate);
        self.fx_chain.to_map(&mut m);
        m
    }

    /// Generate a BTreeMap with musically useful random parameter values.
    /// Uses a simple xorshift64 RNG seeded from system time (no external deps).
    pub fn random_map() -> std::collections::BTreeMap<String, f32> {
        // Start from defaults
        let mut m = Self::default().to_map();

        // Simple xorshift64 RNG
        let mut state: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEAD_BEEF_CAFE_1234);
        if state == 0 { state = 1; }
        let mut rng = || -> f32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state as f64 / u64::MAX as f64) as f32
        };
        // Helper macro to avoid closure borrow conflicts
        macro_rules! range {
            ($lo:expr, $hi:expr) => {{ let r = rng(); $lo + r * ($hi - $lo) }}
        }

        // -- Oscillator: pick from common types (Sine=0, Saw=1, Square=2, Tri=3, FM=4, Supersaw=20)
        let osc_choices = [0.0_f32, 1.0, 2.0, 3.0, 4.0, 20.0];
        let osc_idx = (rng() * osc_choices.len() as f32) as usize % osc_choices.len();
        let osc = osc_choices[osc_idx];
        m.insert("osc_type".into(), osc);

        // Detune (subtle)
        m.insert("osc_detune".into(), range!(-0.15, 0.15));

        // Filter
        m.insert("filter_cutoff".into(), range!(200.0, 12000.0));
        m.insert("filter_resonance".into(), range!(0.0, 0.6));
        // Filter type: mostly LP (0), sometimes others
        let filt_choices = [0.0_f32, 0.0, 0.0, 4.0, 12.0]; // bias toward LP
        let filt_idx = (rng() * filt_choices.len() as f32) as usize % filt_choices.len();
        m.insert("filter_type".into(), filt_choices[filt_idx]);
        m.insert("filter_env_amount".into(), range!(0.0, 0.5));

        // Amp ADSR
        m.insert("amp_attack".into(), range!(0.001, 0.5));
        m.insert("amp_decay".into(), range!(0.05, 0.8));
        m.insert("amp_sustain".into(), range!(0.3, 1.0));
        m.insert("amp_release".into(), range!(0.05, 1.5));

        // Filter ADSR
        m.insert("filter_attack".into(), range!(0.001, 0.3));
        m.insert("filter_decay".into(), range!(0.05, 0.6));
        m.insert("filter_sustain".into(), range!(0.2, 0.8));
        m.insert("filter_release".into(), range!(0.05, 1.0));

        // Noise (usually none, sometimes a touch)
        let r = rng();
        m.insert("noise_level".into(), if r < 0.3 { range!(0.0, 0.15) } else { 0.0 });

        // LFO 1 (subtle modulation)
        m.insert("lfo_rate".into(), range!(0.5, 8.0));
        let r = rng(); m.insert("lfo_filter_depth".into(), if r < 0.4 { range!(0.0, 0.3) } else { 0.0 });
        let r = rng(); m.insert("lfo_pitch_depth".into(), if r < 0.2 { range!(0.0, 0.1) } else { 0.0 });
        let r = rng(); m.insert("lfo_amp_depth".into(), if r < 0.2 { range!(0.0, 0.2) } else { 0.0 });

        // FX: small random mix values (keep things subtle)
        let r = rng(); m.insert("chorus_mix".into(), if r < 0.3 { range!(0.05, 0.25) } else { 0.0 });
        let r = rng(); m.insert("delay_mix".into(), if r < 0.25 { range!(0.05, 0.2) } else { 0.0 });
        let r = rng(); m.insert("reverb_mix".into(), if r < 0.35 { range!(0.05, 0.3) } else { 0.0 });

        // Supersaw params (only relevant if osc_type == 20)
        if osc == 20.0 {
            m.insert("supersaw_detune".into(), range!(0.2, 0.7));
            m.insert("supersaw_mix".into(), range!(0.3, 0.8));
        }

        // FM params (only relevant if osc_type == 4)
        if osc == 4.0 {
            m.insert("fm_ratio".into(), range!(1.0, 8.0));
            m.insert("fm_index".into(), range!(1.0, 10.0));
        }

        // Velocity: fixed curve (user has no velocity keyboard)
        m.insert("velocity_curve".into(), 3.0); // Fixed

        m
    }
}
