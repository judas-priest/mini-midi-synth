/// Formant filter: 5 parallel resonant bandpass filters for vocal synthesis.
///
/// Source-filter model: glottal source (saw/square) → 5 parallel biquad bandpass → sum.
/// Each formant has: center frequency, bandwidth (Hz), gain (dB).

use std::f32::consts::PI;

const NUM_FORMANTS: usize = 5;

/// Formant parameters for one resonance.
#[derive(Clone, Copy)]
struct Formant {
    freq: f32,
    gain_db: f32,
    bw: f32,
}

/// Voice types for formant table lookup.
#[derive(Clone, Copy)]
pub enum VoiceType {
    Bass = 0,
    Tenor = 1,
    Alto = 2,
    Soprano = 3,
}

impl VoiceType {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::Bass,
            1 => Self::Tenor,
            2 => Self::Alto,
            _ => Self::Soprano,
        }
    }
}

/// Vowel types.
#[derive(Clone, Copy)]
pub enum Vowel {
    A = 0, // "ah"
    E = 1, // "eh"
    I = 2, // "ee"
    O = 3, // "oh"
    U = 4, // "oo"
}

impl Vowel {
    pub fn from_param(v: f32) -> Self {
        match v as u32 {
            0 => Self::A,
            1 => Self::E,
            2 => Self::I,
            3 => Self::O,
            _ => Self::U,
        }
    }
}

/// Canonical formant data: [voice_type][vowel] → [F1..F5 as (freq, gain_db, bandwidth)]
/// Data from Csound/IRCAM measurements.
const FORMANT_TABLE: [[[[f32; 3]; NUM_FORMANTS]; 5]; 4] = [
    // Bass (0)
    [
        // A (ah)
        [[600.0, 0.0, 60.0], [1040.0, -7.0, 70.0], [2250.0, -9.0, 110.0], [2450.0, -9.0, 120.0], [2750.0, -20.0, 130.0]],
        // E (eh)
        [[400.0, 0.0, 40.0], [1620.0, -12.0, 80.0], [2400.0, -9.0, 100.0], [2800.0, -12.0, 120.0], [3100.0, -18.0, 120.0]],
        // I (ee)
        [[250.0, 0.0, 60.0], [1750.0, -30.0, 90.0], [2600.0, -16.0, 100.0], [3050.0, -22.0, 120.0], [3340.0, -28.0, 120.0]],
        // O (oh)
        [[400.0, 0.0, 40.0], [750.0, -11.0, 80.0], [2400.0, -21.0, 100.0], [2600.0, -20.0, 120.0], [2900.0, -40.0, 120.0]],
        // U (oo)
        [[350.0, 0.0, 40.0], [600.0, -20.0, 80.0], [2400.0, -32.0, 100.0], [2675.0, -28.0, 120.0], [2950.0, -36.0, 120.0]],
    ],
    // Tenor (1)
    [
        [[650.0, 0.0, 80.0], [1080.0, -6.0, 90.0], [2650.0, -7.0, 120.0], [2900.0, -8.0, 130.0], [3250.0, -22.0, 140.0]],
        [[400.0, 0.0, 70.0], [1700.0, -14.0, 80.0], [2600.0, -12.0, 100.0], [3200.0, -14.0, 120.0], [3580.0, -20.0, 120.0]],
        [[290.0, 0.0, 40.0], [1870.0, -15.0, 90.0], [2800.0, -18.0, 100.0], [3250.0, -20.0, 120.0], [3540.0, -30.0, 120.0]],
        [[400.0, 0.0, 40.0], [800.0, -10.0, 80.0], [2600.0, -12.0, 100.0], [2800.0, -12.0, 120.0], [3000.0, -26.0, 120.0]],
        [[350.0, 0.0, 40.0], [600.0, -20.0, 60.0], [2700.0, -17.0, 100.0], [2900.0, -14.0, 120.0], [3300.0, -26.0, 120.0]],
    ],
    // Alto (2)
    [
        [[800.0, 0.0, 80.0], [1150.0, -4.0, 90.0], [2800.0, -20.0, 120.0], [3500.0, -36.0, 130.0], [4950.0, -60.0, 140.0]],
        [[400.0, 0.0, 60.0], [1600.0, -24.0, 80.0], [2700.0, -30.0, 120.0], [3300.0, -35.0, 150.0], [4950.0, -60.0, 200.0]],
        [[350.0, 0.0, 50.0], [1700.0, -20.0, 100.0], [2700.0, -30.0, 120.0], [3700.0, -36.0, 150.0], [4950.0, -60.0, 200.0]],
        [[450.0, 0.0, 70.0], [800.0, -9.0, 80.0], [2830.0, -16.0, 100.0], [3500.0, -28.0, 130.0], [4950.0, -55.0, 135.0]],
        [[325.0, 0.0, 50.0], [700.0, -12.0, 60.0], [2530.0, -30.0, 170.0], [3500.0, -40.0, 180.0], [4950.0, -64.0, 200.0]],
    ],
    // Soprano (3)
    [
        [[800.0, 0.0, 80.0], [1150.0, -6.0, 90.0], [2900.0, -32.0, 120.0], [3900.0, -20.0, 130.0], [4950.0, -50.0, 140.0]],
        [[350.0, 0.0, 60.0], [2000.0, -20.0, 100.0], [2800.0, -15.0, 120.0], [3600.0, -40.0, 150.0], [4950.0, -56.0, 200.0]],
        [[270.0, 0.0, 60.0], [2140.0, -12.0, 90.0], [2950.0, -26.0, 100.0], [3900.0, -26.0, 120.0], [4950.0, -44.0, 120.0]],
        [[450.0, 0.0, 70.0], [800.0, -11.0, 80.0], [2830.0, -22.0, 100.0], [3800.0, -22.0, 130.0], [4950.0, -50.0, 135.0]],
        [[325.0, 0.0, 50.0], [700.0, -16.0, 60.0], [2700.0, -35.0, 170.0], [3800.0, -40.0, 180.0], [4950.0, -60.0, 200.0]],
    ],
];

/// Second-order resonant bandpass biquad filter.
#[derive(Clone)]
struct ResonantBP {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl ResonantBP {
    fn new() -> Self {
        Self { b0: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, x1: 0.0, x2: 0.0, y1: 0.0, y2: 0.0 }
    }

    fn set_params(&mut self, freq: f32, bw: f32, gain_db: f32, sample_rate: f32) {
        let gain_linear = 10.0_f32.powf(gain_db / 20.0);
        let omega = 2.0 * PI * freq / sample_rate;
        let r = (-PI * bw / sample_rate).exp();
        let r2 = r * r;

        self.a1 = -2.0 * r * omega.cos();
        self.a2 = r2;

        let peak = (1.0 - r2).max(0.001);
        let scale = gain_linear / peak;
        self.b0 = scale * peak * 0.5;
        self.b2 = -self.b0;
    }

    #[inline]
    fn tick(&mut self, input: f32) -> f32 {
        let out = self.b0 * input + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = out;
        out
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Formant filter bank: 5 parallel resonant bandpass filters.
#[derive(Clone)]
pub struct FormantFilter {
    filters: [ResonantBP; NUM_FORMANTS],
    sample_rate: f32,
}

impl FormantFilter {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            filters: std::array::from_fn(|_| ResonantBP::new()),
            sample_rate,
        }
    }

    /// Configure formants for a given voice type and vowel.
    pub fn set_voice_vowel(&mut self, voice: VoiceType, vowel: Vowel) {
        let data = &FORMANT_TABLE[voice as usize][vowel as usize];
        for i in 0..NUM_FORMANTS {
            let [freq, gain_db, bw] = data[i];
            self.filters[i].set_params(freq, bw, gain_db, self.sample_rate);
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
    }

    /// Process one sample: sum of all 5 parallel bandpass outputs.
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        let mut out = 0.0;
        for f in &mut self.filters {
            out += f.tick(input);
        }
        out
    }
}
