/// Nimbus — Granular synthesis cloud.
/// Records incoming audio into a buffer and replays it as overlapping grains.
/// Each grain has: start position, size, pitch shift, envelope (Hann window), pan.
///
/// Inspired by Mutable Instruments Clouds / Surge XT Nimbus.
///
/// Parameters:
/// - `position` (0..1): playback position in the recorded buffer
/// - `size` (0..1): grain size (0.01s..0.5s)
/// - `pitch` (-1..1): pitch shift (-1 octave to +1 octave, 0=unity)
/// - `density` (0..1): grain spawn rate (1..50 grains/sec)
/// - `spread` (0..1): random spread of position/pitch per grain
/// - `texture` (0..1): grain window shape (0=Hann, 1=square with short crossfade)
/// - `mix` (0..1): wet/dry

use std::f32::consts::PI;

const BUF_LEN: usize = 88200; // 2 seconds at 44100 Hz
const MAX_GRAINS: usize = 16;

#[derive(Clone, Copy)]
struct Grain {
    active: bool,
    read_pos: f32,    // current read position in buffer (float for interpolation)
    read_speed: f32,  // pitch-shifted read speed (1.0 = normal, 2.0 = octave up)
    age: usize,       // samples elapsed
    lifetime: usize,  // total grain duration in samples
    pan: f32,         // -1..+1 panning
    level: f32,       // initial level (randomized slightly)
}

impl Default for Grain {
    fn default() -> Self {
        Grain {
            active: false,
            read_pos: 0.0,
            read_speed: 1.0,
            age: 0,
            lifetime: 4410,
            pan: 0.0,
            level: 1.0,
        }
    }
}

pub struct Nimbus {
    sample_rate: f32,
    /// Left channel ring buffer (2 seconds at 44.1kHz)
    buf_l: Box<[f32; BUF_LEN]>,
    /// Right channel ring buffer
    buf_r: Box<[f32; BUF_LEN]>,
    /// Current write position in the ring buffer
    write_pos: usize,
    grains: [Grain; MAX_GRAINS],
    /// Accumulator for grain spawning — crosses 1.0 to trigger a new grain
    spawn_accumulator: f32,
    rng_state: u64,
}

impl Nimbus {
    pub fn new(sr: f32) -> Self {
        Nimbus {
            sample_rate: sr,
            buf_l: Box::new([0.0f32; BUF_LEN]),
            buf_r: Box::new([0.0f32; BUF_LEN]),
            write_pos: 0,
            grains: [Grain::default(); MAX_GRAINS],
            spawn_accumulator: 0.0,
            rng_state: 0x123456789ABCDEF0u64,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    fn next_rand(&mut self) -> f32 {
        self.rng_state = self.rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.rng_state >> 33) as f32 / u32::MAX as f32
    }

    /// Read left buffer with linear interpolation.
    #[inline(always)]
    fn read_interp_l(&self, pos: f32) -> f32 {
        let len = BUF_LEN as f32;
        let pos = ((pos % len) + len) % len;
        let i0 = pos as usize % BUF_LEN;
        let i1 = (i0 + 1) % BUF_LEN;
        let frac = pos - pos.floor();
        self.buf_l[i0] + frac * (self.buf_l[i1] - self.buf_l[i0])
    }

    /// Read right buffer with linear interpolation.
    #[inline(always)]
    fn read_interp_r(&self, pos: f32) -> f32 {
        let len = BUF_LEN as f32;
        let pos = ((pos % len) + len) % len;
        let i0 = pos as usize % BUF_LEN;
        let i1 = (i0 + 1) % BUF_LEN;
        let frac = pos - pos.floor();
        self.buf_r[i0] + frac * (self.buf_r[i1] - self.buf_r[i0])
    }

    /// Compute grain envelope at normalized phase t (0..1).
    /// `texture` 0 = Hann window, 1 = square with short crossfade.
    #[inline(always)]
    fn grain_envelope(t: f32, texture: f32) -> f32 {
        let hann = 0.5 * (1.0 - (2.0 * PI * t).cos());
        // Square with 5% linear fade-in / fade-out
        let fade_len = 0.05_f32;
        let square = if t < fade_len {
            t / fade_len
        } else if t > 1.0 - fade_len {
            (1.0 - t) / fade_len
        } else {
            1.0
        };
        // Crossfade between Hann and square
        hann + texture * (square - hann)
    }

    /// Spawn a new grain using the current playback parameters.
    fn spawn_grain(&mut self, position: f32, size: f32, pitch: f32, spread: f32) {
        // Find an inactive grain slot
        let slot = match self.grains.iter().position(|g| !g.active) {
            Some(s) => s,
            None => return, // all 16 slots busy
        };

        let sr = self.sample_rate;
        // Grain size: map 0..1 to 0.01s..0.5s
        let grain_dur_s = 0.01 + size * 0.49;
        let lifetime = (grain_dur_s * sr) as usize;
        if lifetime == 0 {
            return;
        }

        // Randomize position within spread range
        let pos_rand = self.next_rand() * 2.0 - 1.0; // -1..1
        let raw_pos = (position + pos_rand * spread * 0.2).clamp(0.0, 1.0);
        // Start the grain raw_pos * BUF_LEN samples behind the current write head
        let offset = (raw_pos * (BUF_LEN as f32 - 1.0)) as usize;
        let start = (self.write_pos + BUF_LEN - offset) % BUF_LEN;

        // Pitch: -1..+1 maps to -1..+1 octaves => speed = 2^pitch
        let pitch_rand = self.next_rand() * 2.0 - 1.0;
        let pitched = pitch + pitch_rand * spread * 0.25;
        let read_speed = 2.0_f32.powf(pitched);

        // Pan — randomized according to spread
        let pan_rand = self.next_rand() * 2.0 - 1.0;
        let pan = (pan_rand * spread).clamp(-1.0, 1.0);

        // Level — slight randomization for natural variation
        let level = 0.85 + 0.15 * self.next_rand();

        self.grains[slot] = Grain {
            active: true,
            read_pos: start as f32,
            read_speed,
            age: 0,
            lifetime,
            pan,
            level,
        };
    }

    /// Process one stereo sample through the granular cloud.
    ///
    /// - `position` (0..1): playback position in recorded buffer
    /// - `size` (0..1): grain size (0.01s..0.5s)
    /// - `pitch` (-1..1): pitch shift in octaves
    /// - `density` (0..1): grain spawn rate (1..50 grains/sec)
    /// - `spread` (0..1): random spread of position/pitch/pan per grain
    /// - `texture` (0..1): window shape (0=Hann, 1=square-fade)
    /// - `mix` (0..1): wet/dry ratio
    pub fn tick(
        &mut self,
        in_l: f32,
        in_r: f32,
        position: f32,
        size: f32,
        pitch: f32,
        density: f32,
        spread: f32,
        texture: f32,
        mix: f32,
    ) -> (f32, f32) {
        // --- Record input into ring buffer ---
        self.buf_l[self.write_pos] = in_l;
        self.buf_r[self.write_pos] = in_r;
        self.write_pos = (self.write_pos + 1) % BUF_LEN;

        // --- Grain spawning ---
        // density 0..1 → 1..50 grains/sec
        let grains_per_sec = 1.0 + density * 49.0;
        let spawn_inc = grains_per_sec / self.sample_rate;
        self.spawn_accumulator += spawn_inc;
        while self.spawn_accumulator >= 1.0 {
            self.spawn_accumulator -= 1.0;
            self.spawn_grain(position, size, pitch, spread);
        }

        // --- Render grains ---
        let mut wet_l = 0.0_f32;
        let mut wet_r = 0.0_f32;

        let active_count = self.grains.iter().filter(|g| g.active).count().max(1) as f32;
        // Gain normalisation: scale down with sqrt of active grain count
        let grain_gain = 1.0 / active_count.sqrt();

        // Collect (new_read_pos, done) for each grain without aliasing buf_l/buf_r
        let mut grain_updates: [(f32, bool); MAX_GRAINS] = [(0.0, false); MAX_GRAINS];

        for (i, grain) in self.grains.iter().enumerate() {
            if !grain.active {
                continue;
            }
            let t = grain.age as f32 / grain.lifetime as f32;
            let env = Self::grain_envelope(t, texture);
            let amp = env * grain.level * grain_gain;

            let s_l = self.read_interp_l(grain.read_pos);
            let s_r = self.read_interp_r(grain.read_pos);

            // Constant-power panning: pan -1..+1 → angle 0..PI/2
            let pan_angle = (grain.pan + 1.0) * 0.25 * PI;
            let pan_l = pan_angle.cos();
            let pan_r = pan_angle.sin();

            wet_l += s_l * amp * pan_l;
            wet_r += s_r * amp * pan_r;

            let done = grain.age + 1 >= grain.lifetime;
            grain_updates[i] = (grain.read_pos + grain.read_speed, done);
        }

        // Apply grain state updates
        for (i, grain) in self.grains.iter_mut().enumerate() {
            if !grain.active {
                continue;
            }
            let (new_pos, done) = grain_updates[i];
            grain.read_pos = new_pos;
            grain.age += 1;
            if done {
                grain.active = false;
            }
        }

        let dry_l = in_l * (1.0 - mix);
        let dry_r = in_r * (1.0 - mix);
        (dry_l + wet_l * mix, dry_r + wet_r * mix)
    }
}
