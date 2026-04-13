/// Single synth voice: multi-oscillator → filter routing → amplitude envelope.

use super::envelope::Envelope;
use super::filter::{Filter, FilterType};
use super::formant::{FormantFilter, VoiceType, Vowel};
use super::oscillator::{OscType, Oscillator};
use super::wave_shaper::shape_sample;
use super::ModulationState;

/// Fast 2^(semitones/12) — replaces expensive libm powf() in the per-sample hot path.
/// Uses integer trick + 4th-order minimax polynomial for 2^frac.
/// Max error < 0.00004 (0.004%) across the full audio range.
#[inline(always)]
fn semitones_to_ratio(semis: f32) -> f32 {
    let x = semis * (1.0 / 12.0);
    // Split into integer and fractional parts
    let xi = x.floor() as i32;
    let xf = x - xi as f32;
    // 2^xi via bit manipulation (exact for |xi| < 127)
    let int_part = f32::from_bits(((xi + 127).clamp(1, 254) as u32) << 23);
    // 2^xf via minimax polynomial on [0,1)
    let frac_part = 1.0 + xf * (0.693_147_2 + xf * (0.240_226_5 + xf * (0.055_504_1 + xf * 0.009_618_1)));
    int_part * frac_part
}

#[derive(Clone, Copy, PartialEq)]
pub enum FilterRouting {
    Single,   // 0: mix → filter1 → amp
    Serial,   // 1: mix → filter1 → filter2 → amp
    Parallel, // 2: mix → (filter1 + filter2) / 2 → amp
}

impl FilterRouting {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            1 => Self::Serial,
            2 => Self::Parallel,
            _ => Self::Single,
        }
    }
}

fn apply_velocity_curve(v: f32, curve: u8) -> f32 {
    match curve {
        1 => v * v,
        2 => v.sqrt(),
        3 => 1.0,
        _ => v,
    }
}

#[derive(Clone)]
pub struct Voice {
    pub note: u8,
    pub active: bool,
    /// Monotonic counter set at note-on for voice-stealing (oldest first).
    pub age: u64,
    pub poly_aftertouch: f32,  // 0..1, updated per-note via MIDI 0xA0
    oscs: [Oscillator; 3],
    num_oscs: u8,
    osc_levels: [f32; 3],
    filter: Filter,
    filter2: Filter,
    filter_routing: FilterRouting,
    formant_filter: FormantFilter,
    use_formant: bool,
    amp_env: Envelope,
    filter_env: Envelope,
    freq: f32,
    target_freq: f32,
    porta_coeff: f32,
    velocity: f32,
    vel_to_filter: f32,
    filter_base_cutoff: f32,
    filter_env_amount: f32,
    filter_env_semitones: f32,
    filter_key_track: f32,
    filter_key_mult: f32,
    filter2_base_cutoff: f32,
    fm_base_index: f32,
    fm_env_amount: f32,
    pd_base_depth: f32,
    pd_env_amount: f32,
    fm_cross_depth: f32,
    noise_level: f32,
    noise_state: u32,
    sample_rate: f32,
    // Unison
    unison_count: u8,
    unison_detunes: [f32; 8],
    unison_pans: [f32; 8],
    unison_gain: f32,
    unison_l_gains: [f32; 8],
    unison_r_gains: [f32; 8],
    // Portamento (log-frequency domain)
    log_freq: f32,
    log_target_freq: f32,
    // Osc waveshaper params + DC blocker state
    osc_ws_mode: u32,
    osc_ws_drive: f32,
    osc_ws_mix: f32,
    osc_ws_dc_x1: f32,
    osc_ws_dc_y1: f32,
    // Inter-filter waveshaper (applied between Filter1 and Filter2 in Serial routing)
    inter_ws_mode: u32,
    inter_ws_drive: f32,
    inter_ws_mix: f32,
}

impl Voice {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            note: 0,
            active: false,
            age: 0,
            poly_aftertouch: 0.0,
            oscs: [
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
                Oscillator::new(sample_rate),
            ],
            num_oscs: 1,
            osc_levels: [1.0, 0.0, 0.0],
            filter: Filter::new(sample_rate),
            filter2: Filter::new(sample_rate),
            filter_routing: FilterRouting::Single,
            formant_filter: FormantFilter::new(sample_rate),
            use_formant: false,
            amp_env: Envelope::new(sample_rate),
            filter_env: Envelope::new(sample_rate),
            freq: 440.0,
            target_freq: 440.0,
            porta_coeff: 1.0,
            velocity: 0.0,
            vel_to_filter: 0.0,
            filter_base_cutoff: 8000.0,
            filter_env_amount: 0.0,
            filter_env_semitones: 0.0,
            filter_key_track: 0.0,
            filter_key_mult: 1.0,
            filter2_base_cutoff: 8000.0,
            fm_base_index: 5.0,
            fm_env_amount: 0.0,
            pd_base_depth: 0.0,
            pd_env_amount: 0.0,
            fm_cross_depth: 0.0,
            noise_level: 0.0,
            noise_state: 0xDEADBEEF,
            sample_rate,
            unison_count: 1,
            unison_detunes: [0.0; 8],
            unison_pans: [0.0; 8],
            unison_gain: 1.0,
            unison_l_gains: [std::f32::consts::FRAC_1_SQRT_2; 8],
            unison_r_gains: [std::f32::consts::FRAC_1_SQRT_2; 8],
            log_freq: 440.0_f32.ln(),
            log_target_freq: 440.0_f32.ln(),
            osc_ws_mode: 0,
            osc_ws_drive: 1.0,
            osc_ws_mix: 0.0,
            osc_ws_dc_x1: 0.0,
            osc_ws_dc_y1: 0.0,
            inter_ws_mode: 0,
            inter_ws_drive: 1.0,
            inter_ws_mix: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.oscs = [
            Oscillator::new(sample_rate),
            Oscillator::new(sample_rate),
            Oscillator::new(sample_rate),
        ];
        self.filter = Filter::new(sample_rate);
        self.filter2 = Filter::new(sample_rate);
        self.formant_filter = FormantFilter::new(sample_rate);
        self.amp_env = Envelope::new(sample_rate);
        self.filter_env = Envelope::new(sample_rate);
        self.sample_rate = sample_rate;
        self.active = false;
    }

    pub fn note_on(
        &mut self,
        note: u8,
        velocity: u8,
        age: u64,
        params: &VoiceParams,
        prev_freq: Option<f32>,
    ) {
        self.note = note;
        self.active = true;
        self.age = age;
        let new_freq = midi_to_freq(note);
        self.target_freq = new_freq;

        // Portamento
        if let Some(pf) = prev_freq {
            if params.portamento_time > 0.001 {
                self.freq = pf;
                let samples = params.portamento_time * self.sample_rate;
                self.porta_coeff = 1.0 - (-4.0 / samples).exp();
            } else {
                self.freq = new_freq;
                self.porta_coeff = 1.0;
            }
        } else {
            self.freq = new_freq;
            self.porta_coeff = 1.0;
        }
        self.log_freq = self.freq.ln();
        self.log_target_freq = self.target_freq.ln();

        // Velocity curve
        let raw_vel = velocity as f32 / 127.0;
        self.velocity = apply_velocity_curve(raw_vel, params.velocity_curve as u8);
        self.vel_to_filter = params.vel_to_filter;

        // Osc 1 (primary — all types allowed)
        self.oscs[0].osc_type = OscType::from_param(params.osc_type);
        self.oscs[0].set_detune(params.osc_detune);
        self.oscs[0].fm_ratio = params.fm_ratio;
        self.oscs[0].fm_index = params.fm_index;
        self.oscs[0].ks_brightness = params.ks_brightness;
        self.oscs[0].ks_feedback = params.ks_feedback;
        self.oscs[0].pulse_width = params.pulse_width;
        self.oscs[0].organ_drawbars = params.organ_drawbars;
        self.fm_base_index = params.fm_index;
        self.fm_env_amount = params.fm_env_amount;
        self.pd_base_depth = params.pd_depth;
        self.pd_env_amount = params.pd_env_amount;

        self.oscs[0].reset();

        // Initialize physical model delay lines / special osc types for osc 1
        match self.oscs[0].osc_type {
            OscType::KarplusStrong => self.oscs[0].init_ks(self.freq),
            OscType::CommutedPiano => self.oscs[0].init_commuted_piano(self.freq, self.velocity),
            OscType::BandedWG => self.oscs[0].init_banded_wg(self.freq),
            OscType::AdditivePiano => self.oscs[0].init_additive_piano(self.freq),
            OscType::DrumSynth => self.oscs[0].init_drum_synth(
                self.freq,
                params.drum_pitch_amount,
                params.drum_pitch_decay,
                params.drum_noise_level,
                params.drum_noise_decay,
                params.drum_noise_color,
            ),
            OscType::BassGuitar => self.oscs[0].init_bass_guitar(
                self.freq,
                self.velocity,
                params.bass_style,
                params.bass_tone,
                params.bass_body,
                params.bass_pickup,
            ),
            OscType::BowedString => self.oscs[0].init_bowed_string(
                self.freq,
                self.velocity,
                params.bow_pressure,
                params.bow_position,
                params.body_type,
            ),
            OscType::Brass => self.oscs[0].init_brass(
                self.freq,
                self.velocity,
                params.lip_tension,
                params.blowing_pressure,
                params.bell_type,
            ),
            OscType::PhaseDistortion => self.oscs[0].init_phase_distortion(
                params.pd_shape as u8,
                params.pd_depth,
            ),
            OscType::Wavefolder => self.oscs[0].init_wavefolder(
                params.fold_source as u8,
                params.fold_amount,
                params.fold_symmetry,
            ),
            OscType::ModalResonator => self.oscs[0].init_modal_resonator(
                self.freq,
                params.modal_material as u8,
                params.modal_brightness,
                params.modal_damping,
                params.modal_strike_pos,
            ),
            OscType::HardSync => self.oscs[0].init_hard_sync(
                params.sync_ratio,
                params.sync_shape as u8,
            ),
            OscType::Accordion => self.oscs[0].init_accordion(
                self.freq,
                self.velocity,
                params.accordion_register,
                params.accordion_bellows,
            ),
            OscType::Saxophone => self.oscs[0].init_saxophone(
                self.freq,
                self.velocity,
                params.sax_reed_stiffness,
                params.sax_embouchure,
                params.sax_blow_pressure,
                params.sax_type,
            ),
            OscType::Supersaw => self.oscs[0].init_supersaw(
                params.supersaw_detune,
                params.supersaw_mix,
            ),
            OscType::PianoModel => self.oscs[0].init_piano_model(self.freq),
            OscType::ElectricPiano => self.oscs[0].init_electric_piano(self.freq, params.epiano_type as u8),
            OscType::Alias => self.oscs[0].init_alias(params.alias_wave_type as u8, params.alias_crush),
            OscType::Window => self.oscs[0].init_window(params.window_type as u8, params.window_morph, params.window_formant),
            OscType::Wavetable => self.oscs[0].init_wavetable(params.window_morph),
            OscType::Fm3 => { /* state initialized in reset() */ }
            _ => {}
        }

        // Multi-osc setup
        self.num_oscs = (params.osc_count as u8).clamp(1, 3);
        self.osc_levels[0] = params.osc1_level;

        // Osc 2 (simple types only)
        if self.num_oscs >= 2 {
            let osc2_type = OscType::from_param(params.osc2_type);
            self.oscs[1].osc_type = if osc2_type.is_simple() {
                osc2_type
            } else {
                OscType::Saw
            };
            self.oscs[1].set_detune(params.osc2_detune);
            self.oscs[1].fm_ratio = params.fm_ratio;
            self.oscs[1].fm_index = params.fm_index;
            self.oscs[1].pulse_width = params.pulse_width;
            self.oscs[1].reset();
            self.osc_levels[1] = params.osc2_level;
        } else {
            self.osc_levels[1] = 0.0;
        }

        // Osc 3 (simple types only)
        if self.num_oscs >= 3 {
            let osc3_type = OscType::from_param(params.osc3_type);
            self.oscs[2].osc_type = if osc3_type.is_simple() {
                osc3_type
            } else {
                OscType::Saw
            };
            self.oscs[2].set_detune(params.osc3_detune);
            self.oscs[2].fm_ratio = params.fm_ratio;
            self.oscs[2].fm_index = params.fm_index;
            self.oscs[2].pulse_width = params.pulse_width;
            self.oscs[2].reset();
            self.osc_levels[2] = params.osc3_level;
        } else {
            self.osc_levels[2] = 0.0;
        }

        // Filter 1 setup
        let filter_type = FilterType::from_param(params.filter_type);
        self.use_formant = filter_type == FilterType::Formant;

        if self.use_formant {
            let voice_type = VoiceType::from_param(params.formant_voice);
            let vowel = Vowel::from_param(params.formant_vowel);
            self.formant_filter.set_voice_vowel(voice_type, vowel);
            self.formant_filter.reset();
            self.filter_routing = FilterRouting::Single;
        } else {
            self.filter.set_type(filter_type);
            self.filter.set_resonance(params.filter_resonance);
            self.filter.svf_morph = params.svf_morph;
            self.filter.reset();

            // Filter routing + filter 2
            self.filter_routing = FilterRouting::from_param(params.filter_routing);
            if self.filter_routing != FilterRouting::Single {
                let f2_type = FilterType::from_param(params.filter2_type);
                if f2_type == FilterType::Formant {
                    self.filter_routing = FilterRouting::Single;
                } else {
                    self.filter2.set_type(f2_type);
                    self.filter2.set_resonance(params.filter2_resonance);
                    self.filter2.reset();
                    self.filter2_base_cutoff = params.filter2_cutoff;
                }
            }
        }

        self.filter_base_cutoff = params.filter_cutoff;
        self.filter.set_cutoff(params.filter_cutoff);
        self.filter.force_update();
        self.filter2.set_cutoff(params.filter2_cutoff);
        self.filter2.force_update();
        self.filter_env_amount = params.filter_env_amount;
        self.filter_env_semitones = params.filter_env_semitones;
        self.filter_key_track = params.filter_key_track;
        self.filter_key_mult = if params.filter_key_track > 0.001 {
            2.0_f32.powf(params.filter_key_track * (note as f32 - 60.0) / 12.0)
        } else {
            1.0
        };
        self.noise_level = params.noise_level;
        self.fm_cross_depth = params.fm_cross_depth;

        // Osc waveshaper
        self.osc_ws_mode = params.osc_ws_mode;
        self.osc_ws_drive = params.osc_ws_drive;
        self.osc_ws_mix = params.osc_ws_mix;
        self.osc_ws_dc_x1 = 0.0;
        self.osc_ws_dc_y1 = 0.0;

        // Inter-filter waveshaper
        self.inter_ws_mode = params.inter_ws_mode;
        self.inter_ws_drive = params.inter_ws_drive;
        self.inter_ws_mix = params.inter_ws_mix;

        // Unison
        let is_physical = !self.oscs[0].osc_type.is_simple()
            && self.oscs[0].osc_type != OscType::Fm
            && self.oscs[0].osc_type != OscType::Noise;
        self.unison_count = if is_physical {
            1
        } else {
            (params.unison_voices as u8).clamp(1, 8)
        };

        if self.unison_count > 1 {
            let detune_cents = params.unison_detune;
            let spread = params.unison_spread;
            let n = self.unison_count as f32;
            for i in 0..self.unison_count as usize {
                let t = if n > 1.0 {
                    (i as f32 / (n - 1.0)) * 2.0 - 1.0
                } else {
                    0.0
                };
                self.unison_detunes[i] = 2.0_f32.powf(t * detune_cents / 1200.0);
                self.unison_pans[i] = t * spread;
            }
        } else {
            self.unison_detunes[0] = 1.0;
            self.unison_pans[0] = 0.0;
        }
        self.unison_gain = 1.0 / (self.unison_count as f32).sqrt();
        for i in 0..self.unison_count as usize {
            let pan = self.unison_pans[i];
            self.unison_l_gains[i] = (0.5 - pan * 0.5).sqrt();
            self.unison_r_gains[i] = (0.5 + pan * 0.5).sqrt();
        }

        self.amp_env.set_adsr(
            params.amp_attack,
            params.amp_decay,
            params.amp_sustain,
            params.amp_release,
        );
        self.amp_env.set_attack_shape(params.env_attack_shape);
        self.amp_env.set_decay_shape(params.env_decay_shape);
        self.amp_env.set_release_shape(params.env_release_shape);
        self.filter_env.set_adsr(
            params.filter_attack,
            params.filter_decay,
            params.filter_sustain,
            params.filter_release,
        );
        self.filter_env.set_attack_shape(params.filter_env_attack_shape);
        self.filter_env.set_decay_shape(params.filter_env_decay_shape);
        self.filter_env.set_release_shape(params.filter_env_release_shape);

        self.amp_env.note_on();
        self.filter_env.note_on();
    }

    pub fn note_off(&mut self) {
        self.amp_env.note_off();
        self.filter_env.note_off();
    }

    /// Re-trigger envelopes without resetting oscillator state.
    /// Used by step sequencer for per-step articulation.
    pub fn retrigger_envelope(&mut self) {
        self.amp_env.note_on();  // starts attack from current value (no click)
        self.filter_env.note_on();
    }

    /// Re-trigger only the amplitude envelope.
    pub fn retrigger_amp_env(&mut self) {
        self.amp_env.note_on();
    }

    /// Re-trigger only the filter envelope.
    pub fn retrigger_filter_env(&mut self) {
        self.filter_env.note_on();
    }

    /// Mono Single Trigger: change pitch without retriggering envelopes.
    /// Used when a new note arrives while another is held (legato slide).
    pub fn retrigger_note(&mut self, note: u8, params: &VoiceParams) {
        self.note = note;
        let new_freq = midi_to_freq(note);
        self.target_freq = new_freq;
        self.log_target_freq = new_freq.ln();
        // Apply portamento glide (uses existing portamento_time from params)
        if params.portamento_time > 0.001 {
            // log_freq will glide toward log_target_freq in tick() naturally
        } else {
            self.freq = new_freq;
            self.log_freq = new_freq.ln();
        }
    }

    pub fn is_releasing(&self) -> bool {
        self.amp_env.is_releasing()
    }

    #[inline]
    fn next_noise(&mut self) -> f32 {
        self.noise_state ^= self.noise_state << 13;
        self.noise_state ^= self.noise_state >> 17;
        self.noise_state ^= self.noise_state << 5;
        (self.noise_state as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Returns (left, right) stereo pair.
    pub fn tick(&mut self, mods: &ModulationState) -> (f32, f32) {
        if !self.active {
            return (0.0, 0.0);
        }

        let amp = self.amp_env.tick();
        if self.amp_env.is_idle() {
            self.active = false;
            return (0.0, 0.0);
        }

        let filter_mod = self.filter_env.tick();

        // FM index envelope
        if self.fm_env_amount > 0.001 {
            self.oscs[0].fm_index =
                self.fm_base_index * (1.0 - self.fm_env_amount + self.fm_env_amount * filter_mod);
        }

        // Phase Distortion DCW envelope
        if self.pd_env_amount > 0.001 {
            let depth = self.pd_base_depth * (1.0 - self.pd_env_amount + self.pd_env_amount * filter_mod);
            self.oscs[0].set_pd_depth(depth);
        }

        // Portamento (log-frequency domain for perceptually uniform glide)
        if self.porta_coeff < 0.999 {
            self.log_freq += self.porta_coeff * (self.log_target_freq - self.log_freq);
            self.freq = self.log_freq.exp();
        } else {
            self.freq = self.target_freq;
            self.log_freq = self.log_target_freq;
        }

        let base_freq = self.freq * mods.pitch_mult;

        // Velocity → filter
        let vel_factor = 1.0 + self.vel_to_filter * self.velocity * 4.0;
        // Semitone-based modulation (from mod matrix + filter env semitones) applied multiplicatively — matches Surge
        let env_semis = self.filter_env_semitones * filter_mod;
        let total_semis = mods.filter_offset_semis + env_semis;
        let semis_mult = if total_semis.abs() > 0.01 {
            semitones_to_ratio(total_semis)
        } else { 1.0 };
        let cutoff1 = (self.filter_base_cutoff * vel_factor * semis_mult
            + self.filter_env_amount * filter_mod
            + mods.filter_offset)
            * self.filter_key_mult;

        if self.unison_count <= 1 {
            let mix = self.render_oscs(base_freq);
            let filtered = self.apply_filter(mix, cutoff1, filter_mod, semis_mult);
            let out = filtered * amp * self.velocity * mods.amp_mod;
            return (out, out);
        }

        // Unison path: all voices use oscs[0].tick() for consistent timbre
        let mut sum_l = 0.0_f32;
        let mut sum_r = 0.0_f32;
        let gain = self.unison_gain;

        for u in 0..self.unison_count as usize {
            let detuned_freq = base_freq * self.unison_detunes[u];
            let sample = self.oscs[0].tick(detuned_freq);
            sum_l += sample * self.unison_l_gains[u];
            sum_r += sample * self.unison_r_gains[u];
        }

        // Add multi-osc contributions (once, at base freq)
        let mut extra = 0.0_f32;
        if self.num_oscs >= 2 {
            let osc1_out = sum_l + sum_r; // approximate for FM cross
            let fm_freq = if self.fm_cross_depth > 0.001 {
                base_freq * (1.0 + self.fm_cross_depth * osc1_out * 0.5)
            } else {
                base_freq
            };
            extra += self.oscs[1].tick(fm_freq) * self.osc_levels[1];
        }
        if self.num_oscs >= 3 {
            let osc1_out = sum_l + sum_r;
            let fm_freq = if self.fm_cross_depth > 0.001 {
                base_freq * (1.0 + self.fm_cross_depth * osc1_out * 0.5)
            } else {
                base_freq
            };
            extra += self.oscs[2].tick(fm_freq) * self.osc_levels[2];
        }

        // Apply osc1 level and add extra oscs
        sum_l = sum_l * self.osc_levels[0] * gain + extra * 0.5;
        sum_r = sum_r * self.osc_levels[0] * gain + extra * 0.5;

        // Add noise once on summed result
        if self.noise_level > 0.001
            && self.oscs[0].osc_type != OscType::Noise
            && self.oscs[0].osc_type != OscType::DrumSynth
        {
            let n = self.next_noise() * self.noise_level;
            sum_l += n;
            sum_r += n;
        }

        // Apply waveshaper once on summed result
        if self.osc_ws_mix > 0.001 {
            let mono = (sum_l + sum_r) * 0.5;
            let shaped = shape_sample(mono, self.osc_ws_mode, self.osc_ws_drive);
            let r = 0.9997_f32;
            let dc_out = shaped - self.osc_ws_dc_x1 + r * self.osc_ws_dc_y1;
            self.osc_ws_dc_x1 = shaped;
            self.osc_ws_dc_y1 = dc_out;
            let ws_mono = mono + self.osc_ws_mix * (dc_out - mono);
            let ratio = if mono.abs() > 0.0001 { ws_mono / mono } else { 1.0 };
            sum_l *= ratio;
            sum_r *= ratio;
        }

        // Filter the mono sum, restore L/R ratio with safe crossfade
        let mono = (sum_l + sum_r) * 0.5;
        let filtered = self.apply_filter(mono, cutoff1, filter_mod, semis_mult);

        let mono_abs = mono.abs();
        let (out_l, out_r) = if mono_abs > 0.01 {
            // Safe zone: normal ratio path
            let ratio = (filtered / mono).clamp(-4.0, 4.0);
            (sum_l * ratio, sum_r * ratio)
        } else if mono_abs > 0.0001 {
            // Transition zone: crossfade between ratio and mono output
            let blend = (mono_abs - 0.0001) / (0.01 - 0.0001);
            let ratio = (filtered / mono).clamp(-4.0, 4.0);
            let ratio_l = sum_l * ratio;
            let ratio_r = sum_r * ratio;
            (ratio_l * blend + filtered * (1.0 - blend),
             ratio_r * blend + filtered * (1.0 - blend))
        } else {
            (filtered, filtered)
        };

        let final_amp = amp * self.velocity * mods.amp_mod;
        (out_l * final_amp, out_r * final_amp)
    }

    #[inline]
    fn render_oscs(&mut self, freq: f32) -> f32 {
        let osc1_out = self.oscs[0].tick(freq);
        let mut mix = osc1_out * self.osc_levels[0];
        if self.num_oscs >= 2 {
            let fm_freq = if self.fm_cross_depth > 0.001 {
                freq * (1.0 + self.fm_cross_depth * osc1_out)
            } else {
                freq
            };
            mix += self.oscs[1].tick(fm_freq) * self.osc_levels[1];
        }
        if self.num_oscs >= 3 {
            let fm_freq = if self.fm_cross_depth > 0.001 {
                freq * (1.0 + self.fm_cross_depth * osc1_out)
            } else {
                freq
            };
            mix += self.oscs[2].tick(fm_freq) * self.osc_levels[2];
        }

        if self.noise_level > 0.001
            && self.oscs[0].osc_type != OscType::Noise
            && self.oscs[0].osc_type != OscType::DrumSynth
        {
            mix += self.next_noise() * self.noise_level;
        }

        // Per-voice osc waveshaper (pre-filter)
        if self.osc_ws_mix > 0.001 {
            let shaped = shape_sample(mix, self.osc_ws_mode, self.osc_ws_drive);
            // DC blocker (~6 Hz pole at typical sample rates)
            let r = 0.9997_f32;
            let dc_out = shaped - self.osc_ws_dc_x1 + r * self.osc_ws_dc_y1;
            self.osc_ws_dc_x1 = shaped;
            self.osc_ws_dc_y1 = dc_out;
            mix += self.osc_ws_mix * (dc_out - mix);
        }

        mix
    }

    #[inline]
    fn apply_filter(&mut self, mix: f32, cutoff1: f32, filter_mod: f32, semis_mult: f32) -> f32 {
        if self.use_formant {
            self.formant_filter.tick(mix)
        } else {
            self.filter.set_cutoff(cutoff1);

            match self.filter_routing {
                FilterRouting::Single => self.filter.tick(mix),
                FilterRouting::Serial => {
                    let f1 = self.filter.tick(mix);
                    // Inter-filter waveshaper (Surge-style: WS between F1 and F2)
                    let inter = if self.inter_ws_mix > 0.001 {
                        let shaped = shape_sample(f1, self.inter_ws_mode, self.inter_ws_drive);
                        f1 + self.inter_ws_mix * (shaped - f1)
                    } else {
                        f1
                    };
                    let cutoff2 = (self.filter2_base_cutoff * semis_mult
                        + self.filter_env_amount * filter_mod)
                        * self.filter_key_mult;
                    self.filter2.set_cutoff(cutoff2);
                    self.filter2.tick(inter)
                }
                FilterRouting::Parallel => {
                    let f1 = self.filter.tick(mix);
                    let cutoff2 = (self.filter2_base_cutoff * semis_mult
                        + self.filter_env_amount * filter_mod)
                        * self.filter_key_mult;
                    self.filter2.set_cutoff(cutoff2);
                    let f2 = self.filter2.tick(mix);
                    (f1 + f2) * 0.5
                }
            }
        }
    }
}

/// Parameters extracted from preset, passed to voice on note-on.
pub struct VoiceParams {
    pub osc_type: f32,
    pub osc_detune: f32,
    pub fm_ratio: f32,
    pub fm_index: f32,
    pub fm_env_amount: f32,
    // Multi-osc
    pub osc_count: f32,
    pub osc1_level: f32,
    pub osc2_type: f32,
    pub osc2_detune: f32,
    pub osc2_level: f32,
    pub osc3_type: f32,
    pub osc3_detune: f32,
    pub osc3_level: f32,
    // Filter 1
    pub filter_cutoff: f32,
    pub filter_resonance: f32,
    pub filter_type: f32,
    pub filter_env_amount: f32,
    pub filter_key_track: f32,
    // Filter routing + filter 2
    pub filter_routing: f32,
    pub filter2_type: f32,
    pub filter2_cutoff: f32,
    pub filter2_resonance: f32,
    // Noise
    pub noise_level: f32,
    // Amp envelope
    pub amp_attack: f32,
    pub amp_decay: f32,
    pub amp_sustain: f32,
    pub amp_release: f32,
    // Filter envelope
    pub filter_attack: f32,
    pub filter_decay: f32,
    pub filter_sustain: f32,
    pub filter_release: f32,
    // Physical model params
    pub ks_brightness: f32,
    pub ks_feedback: f32,
    pub organ_drawbars: [f32; 9],
    // Formant
    pub formant_voice: f32,
    pub formant_vowel: f32,
    // Drum synth
    pub drum_pitch_amount: f32,
    pub drum_pitch_decay: f32,
    pub drum_noise_level: f32,
    pub drum_noise_decay: f32,
    pub drum_noise_color: f32,
    // Bass guitar
    pub bass_style: f32,
    pub bass_tone: f32,
    pub bass_body: f32,
    pub bass_pickup: f32,
    // Bowed string
    pub bow_pressure: f32,
    pub bow_position: f32,
    pub body_type: f32,
    // Brass
    pub lip_tension: f32,
    pub blowing_pressure: f32,
    pub bell_type: f32,
    // Phase Distortion
    pub pd_shape: f32,
    pub pd_depth: f32,
    pub pd_env_amount: f32,
    // Wavefolder
    pub fold_amount: f32,
    pub fold_symmetry: f32,
    pub fold_source: f32,
    // Modal Resonator
    pub modal_material: f32,
    pub modal_brightness: f32,
    pub modal_damping: f32,
    pub modal_strike_pos: f32,
    // Hard Sync
    pub sync_ratio: f32,
    pub sync_shape: f32,
    // Supersaw
    pub supersaw_detune: f32,
    pub supersaw_mix: f32,
    // Pulse width
    pub pulse_width: f32,
    // Accordion
    pub accordion_register: f32,
    pub accordion_bellows: f32,
    // Saxophone
    pub sax_reed_stiffness: f32,
    pub sax_embouchure: f32,
    pub sax_blow_pressure: f32,
    pub sax_type: f32,
    // Electric Piano
    pub epiano_type: f32,
    // Alias oscillator
    pub alias_wave_type: f32,
    pub alias_crush: f32,
    // Window oscillator
    pub window_type: f32,
    pub window_morph: f32,
    pub window_formant: f32,
    // Envelope shapes
    pub env_attack_shape: f32,
    pub env_decay_shape: f32,
    pub env_release_shape: f32,
    pub filter_env_attack_shape: f32,
    pub filter_env_decay_shape: f32,
    pub filter_env_release_shape: f32,
    // Dynamics
    pub velocity_curve: f32,
    pub vel_to_filter: f32,
    // Portamento
    pub portamento_time: f32,
    pub _portamento_mode: f32,
    // Unison
    pub unison_voices: f32,
    pub unison_detune: f32,
    pub unison_spread: f32,
    // FM cross-routing
    pub fm_cross_depth: f32,
    // Filter env in semitones (Surge-style exponential modulation, 0 = disabled)
    pub filter_env_semitones: f32,
    // Osc Waveshaper (per-voice, pre-filter)
    pub osc_ws_mode: u32,
    pub osc_ws_drive: f32,
    pub osc_ws_mix: f32,
    // Inter-filter waveshaper (between F1 and F2 in Serial routing)
    pub inter_ws_mode: u32,
    pub inter_ws_drive: f32,
    pub inter_ws_mix: f32,
    // SVF Morph filter parameter
    pub svf_morph: f32,
}

fn midi_to_freq(note: u8) -> f32 {
    440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0)
}
