/// Rotary Speaker / Leslie cabinet simulator.
///
/// Models the horn (high frequencies, ~150–400 RPM) and rotor (low frequencies,
/// ~40–350 RPM) with modulated delay lines for Doppler shift and AM for tremolo.
/// A one-pole crossover splits the signal between horn and rotor paths.

use super::dsp_utils::buf_read_linear;

const BUF_SIZE: usize = 2048; // covers up to ~46ms delay at 44100 Hz

pub struct RotarySpeaker {
    sample_rate: f32,
    // Horn (treble) LFO
    horn_phase: f32,
    horn_rate: f32, // Hz, current (interpolating between slow/fast)
    // Rotor (bass) LFO
    rotor_phase: f32,
    rotor_rate: f32,
    // Delay line buffers for Doppler
    horn_buf: Box<[f32; BUF_SIZE]>,
    rotor_buf: Box<[f32; BUF_SIZE]>,
    horn_write: usize,
    rotor_write: usize,
    // 1-pole crossover (LP for rotor, HP = input - LP for horn)
    xover_state: f32,
}

impl RotarySpeaker {
    pub fn new(sr: f32) -> Self {
        Self {
            sample_rate: sr,
            horn_phase: 0.0,
            horn_rate: 0.7,
            rotor_phase: 0.25, // offset so horn and rotor are not perfectly in sync
            rotor_rate: 0.7,
            horn_buf: Box::new([0.0; BUF_SIZE]),
            rotor_buf: Box::new([0.0; BUF_SIZE]),
            horn_write: 0,
            rotor_write: 0,
            xover_state: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.horn_buf.fill(0.0);
        self.rotor_buf.fill(0.0);
        self.horn_write = 0;
        self.rotor_write = 0;
        self.xover_state = 0.0;
    }

    /// Process stereo input.
    /// `speed`: 0.0 = slow (chorale), 1.0 = fast (tremolo).
    /// `mix`: wet/dry mix (0.0 = dry, 1.0 = fully wet).
    pub fn tick(&mut self, in_l: f32, in_r: f32, speed: f32, mix: f32) -> (f32, f32) {
        if mix < 0.001 {
            return (in_l, in_r);
        }

        let mono = (in_l + in_r) * 0.5;

        // Target rates: slow = ~0.7 Hz, fast = ~6 Hz for horn; rotor is slower
        let horn_target = if speed > 0.5 { 6.0_f32 } else { 0.7_f32 };
        let rotor_target = if speed > 0.5 { 3.0_f32 } else { 0.7_f32 };
        // Smooth rate changes (Leslie ramp-up)
        let rate_alpha = 0.0002;
        self.horn_rate += rate_alpha * (horn_target - self.horn_rate);
        self.rotor_rate += rate_alpha * (rotor_target - self.rotor_rate);

        // Advance LFO phases
        let sr = self.sample_rate;
        self.horn_phase += self.horn_rate / sr;
        if self.horn_phase >= 1.0 { self.horn_phase -= 1.0; }
        self.rotor_phase += self.rotor_rate / sr;
        if self.rotor_phase >= 1.0 { self.rotor_phase -= 1.0; }

        let horn_lfo = (self.horn_phase * std::f32::consts::TAU).sin();
        let rotor_lfo = (self.rotor_phase * std::f32::consts::TAU).sin();

        // Crossover at ~800 Hz (one-pole LP)
        let xover_coef = 1.0 - (-std::f32::consts::TAU * 800.0 / sr).exp();
        self.xover_state += xover_coef * (mono - self.xover_state);
        let bass = self.xover_state;
        let treble = mono - bass;

        // --- Horn path (treble) ---
        // Modulated delay: center ~2ms, depth ±1ms
        let horn_center = 0.002 * sr;
        let horn_depth = 0.001 * sr;
        let horn_delay = horn_center + horn_depth * horn_lfo;

        self.horn_buf[self.horn_write] = treble;
        self.horn_write = (self.horn_write + 1) % BUF_SIZE;

        let horn_wet = buf_read_linear(&*self.horn_buf, self.horn_write, horn_delay);
        // AM tremolo (Doppler causes both AM and FM; we add a small AM component)
        let horn_am = 1.0 + 0.15 * horn_lfo;
        let horn_out = horn_wet * horn_am;
        // Stereo: left and right get opposite LFO phase
        let horn_l = horn_out * (1.0 + 0.4 * horn_lfo);
        let horn_r = horn_out * (1.0 - 0.4 * horn_lfo);

        // --- Rotor path (bass) ---
        let rotor_center = 0.004 * sr;
        let rotor_depth = 0.002 * sr;
        let rotor_delay = rotor_center + rotor_depth * rotor_lfo;

        self.rotor_buf[self.rotor_write] = bass;
        self.rotor_write = (self.rotor_write + 1) % BUF_SIZE;

        let rotor_wet = buf_read_linear(&*self.rotor_buf, self.rotor_write, rotor_delay);
        let rotor_am = 1.0 + 0.10 * rotor_lfo;
        let rotor_out = rotor_wet * rotor_am;
        let rotor_l = rotor_out * (1.0 + 0.3 * rotor_lfo);
        let rotor_r = rotor_out * (1.0 - 0.3 * rotor_lfo);

        let wet_l = horn_l + rotor_l;
        let wet_r = horn_r + rotor_r;

        let dry = 1.0 - mix;
        (in_l * dry + wet_l * mix, in_r * dry + wet_r * mix)
    }
}

