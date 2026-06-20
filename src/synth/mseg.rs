//! Multi-Segment Envelope Generator (MSEG).
//! Algorithm inspired by Surge XT MSEGModulationHelper.

/// Curve type for each segment.
#[derive(Clone, Copy, PartialEq)]
pub enum CurveType {
    Linear,    // 0
    SCurve,    // 1
    Stairs,    // 2
    Hold,      // 3
    Sine,      // 4
    Triangle,  // 5
    Sawtooth,  // 6
    Brownian,  // 7
}

impl CurveType {
    pub fn from_index(i: u8) -> Self {
        match i {
            1 => Self::SCurve,
            2 => Self::Stairs,
            3 => Self::Hold,
            4 => Self::Sine,
            5 => Self::Triangle,
            6 => Self::Sawtooth,
            7 => Self::Brownian,
            _ => Self::Linear,
        }
    }
}

/// Loop mode for the MSEG.
#[derive(Clone, Copy, PartialEq)]
pub enum LoopMode {
    Oneshot,
    Loop,
    GatedLoop,
}

impl LoopMode {
    pub fn from_index(i: u8) -> Self {
        match i {
            1 => Self::Loop,
            2 => Self::GatedLoop,
            _ => Self::Oneshot,
        }
    }
}

/// One segment of the MSEG.
#[derive(Clone, Copy)]
pub struct MsegSegment {
    pub duration: f32,    // seconds
    pub v0: f32,          // start value (-1..1)
    pub v1: f32,          // end value (-1..1)
    pub curve_type: CurveType,
    pub cpv: f32,         // control point / deform (-1..1)
}

impl Default for MsegSegment {
    fn default() -> Self {
        Self { duration: 0.25, v0: 0.0, v1: 1.0, curve_type: CurveType::Linear, cpv: 0.0 }
    }
}

/// MSEG definition — the data describing the shape.
#[derive(Clone)]
pub struct Mseg {
    pub segments: Vec<MsegSegment>,
    pub loop_mode: LoopMode,
    pub loop_start: usize,
    pub loop_end: usize,
}

impl Default for Mseg {
    fn default() -> Self {
        Self {
            segments: vec![
                MsegSegment { duration: 0.1, v0: 0.0, v1: 1.0, curve_type: CurveType::Linear, cpv: 0.0 },
                MsegSegment { duration: 0.3, v0: 1.0, v1: 0.5, curve_type: CurveType::Linear, cpv: 0.0 },
                MsegSegment { duration: 1.0, v0: 0.5, v1: 0.5, curve_type: CurveType::Hold, cpv: 0.0 },
                MsegSegment { duration: 0.5, v0: 0.5, v1: 0.0, curve_type: CurveType::Linear, cpv: 0.0 },
            ],
            loop_mode: LoopMode::Oneshot,
            loop_start: 0,
            loop_end: 3,
        }
    }
}

/// Runtime state for the MSEG.
#[derive(Clone)]
pub struct MsegState {
    phase: f64,          // time within current segment (0..segment_duration)
    current_seg: usize,
    pub output: f32,
    pub playing: bool,
    gated: bool,
    noise_state: u32,    // for Brownian mode
    sample_rate: f32,
    cached_exp_denom: f32, // (a.exp() - 1.0) for current segment's cpv
    cached_cpv: f32,       // cpv at time of last cache update
}

impl MsegState {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            phase: 0.0, current_seg: 0, output: 0.0,
            playing: false, gated: false,
            noise_state: 0xDEADBEEF, sample_rate,
            cached_exp_denom: 1.0, cached_cpv: f32::NAN,
        }
    }

    fn update_curve_cache(&mut self, seg: &MsegSegment) {
        if seg.cpv != self.cached_cpv || self.cached_cpv.is_nan() {
            self.cached_cpv = seg.cpv;
            let a = seg.cpv * 4.0;
            self.cached_exp_denom = if a.abs() > 0.04 { a.exp() - 1.0 } else { 1.0 };
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
    }

    pub fn trigger(&mut self) {
        self.phase = 0.0;
        self.current_seg = 0;
        self.playing = true;
        self.gated = true;
    }

    pub fn release(&mut self) {
        self.gated = false;
    }

    /// Evaluate the curve at a fractional position within a segment.
    fn eval_curve(frac: f32, seg: &MsegSegment, noise: &mut u32, exp_denom: f32) -> f32 {
        let v0 = seg.v0;
        let v1 = seg.v1;
        let cpv = seg.cpv;

        let t = frac.clamp(0.0, 1.0);

        match seg.curve_type {
            CurveType::Linear => {
                // Linear with deform: exponential warp
                let t = if cpv.abs() > 0.01 {
                    let a = cpv * 4.0;
                    ((a * t).exp() - 1.0) / exp_denom
                } else {
                    t
                };
                v0 + (v1 - v0) * t
            }
            CurveType::SCurve => {
                // Smoothstep with adjustable steepness
                let t = t * t * (3.0 - 2.0 * t);
                let t = if cpv.abs() > 0.01 { t.powf(1.0 + cpv) } else { t };
                v0 + (v1 - v0) * t
            }
            CurveType::Stairs => {
                let steps = (4.0 + cpv.abs() * 12.0) as u32;
                let stepped = (t * steps as f32).floor() / (steps as f32 - 1.0).max(1.0);
                v0 + (v1 - v0) * stepped.min(1.0)
            }
            CurveType::Hold => v0,
            CurveType::Sine => {
                let phase = t * std::f32::consts::PI;
                let s = phase.sin();
                v0 + (v1 - v0) * s
            }
            CurveType::Triangle => {
                let tri = if t < 0.5 { t * 2.0 } else { 2.0 - t * 2.0 };
                v0 + (v1 - v0) * tri
            }
            CurveType::Sawtooth => {
                v0 + (v1 - v0) * t
            }
            CurveType::Brownian => {
                // Random walk with bias toward v1
                *noise ^= *noise << 13;
                *noise ^= *noise >> 17;
                *noise ^= *noise << 5;
                let r = (*noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
                let target = v0 + (v1 - v0) * t;
                target + r * (1.0 - t) * 0.3
            }
        }
    }

    /// Tick the MSEG, advancing by one sample. Returns output (-1..1).
    pub fn tick(&mut self, mseg: &Mseg) -> f32 {
        if !self.playing || mseg.segments.is_empty() {
            return self.output;
        }

        let seg = &mseg.segments[self.current_seg];
        let dur = seg.duration.max(0.001) as f64;
        let frac = (self.phase / dur) as f32;

        self.update_curve_cache(seg);
        let exp_denom = self.cached_exp_denom;
        self.output = Self::eval_curve(frac.min(1.0), seg, &mut self.noise_state, exp_denom).clamp(-1.0, 1.0);

        // Advance time
        self.phase += 1.0 / self.sample_rate as f64;

        // Check segment boundary
        if self.phase >= dur {
            self.phase -= dur;
            self.current_seg += 1;

            // Handle looping
            let loop_end = mseg.loop_end.min(mseg.segments.len().saturating_sub(1));
            let loop_start = mseg.loop_start.min(loop_end);

            match mseg.loop_mode {
                LoopMode::Oneshot => {
                    if self.current_seg >= mseg.segments.len() {
                        self.playing = false;
                        self.output = mseg.segments.last()
                            .map(|s| s.v1).unwrap_or(0.0);
                    }
                }
                LoopMode::Loop => {
                    if self.current_seg > loop_end {
                        self.current_seg = loop_start;
                    }
                }
                LoopMode::GatedLoop => {
                    if self.gated {
                        if self.current_seg > loop_end {
                            self.current_seg = loop_start;
                        }
                    } else {
                        // Released — play remaining segments to end
                        if self.current_seg >= mseg.segments.len() {
                            self.playing = false;
                            self.output = mseg.segments.last()
                                .map(|s| s.v1).unwrap_or(0.0);
                        }
                    }
                }
            }
        }

        self.output
    }
}

// ---------------------------------------------------------------------------
// Patch serialization helpers
// ---------------------------------------------------------------------------

impl Mseg {
    /// Save MSEG data to a flat param map (for patch storage).
    #[allow(dead_code)]
    pub fn save_to_params(&self, params: &mut std::collections::BTreeMap<String, f32>) {
        params.insert("mseg_num_segments".into(), self.segments.len() as f32);
        params.insert("mseg_loop_mode".into(), self.loop_mode as u8 as f32);
        params.insert("mseg_loop_start".into(), self.loop_start as f32);
        params.insert("mseg_loop_end".into(), self.loop_end as f32);
        for (i, seg) in self.segments.iter().enumerate() {
            let prefix = format!("mseg_seg{i}_");
            params.insert(format!("{prefix}dur"), seg.duration);
            params.insert(format!("{prefix}v0"), seg.v0);
            params.insert(format!("{prefix}v1"), seg.v1);
            params.insert(format!("{prefix}curve"), seg.curve_type as u8 as f32);
            params.insert(format!("{prefix}cpv"), seg.cpv);
        }
    }

    /// Load MSEG data from a flat param map.
    pub fn load_from_params(params: &std::collections::BTreeMap<String, f32>) -> Self {
        let n = params.get("mseg_num_segments").copied().unwrap_or(0.0) as usize;
        if n == 0 { return Self::default(); }

        let loop_mode = LoopMode::from_index(
            params.get("mseg_loop_mode").copied().unwrap_or(0.0) as u8
        );
        let loop_start = params.get("mseg_loop_start").copied().unwrap_or(0.0) as usize;
        let loop_end = params.get("mseg_loop_end").copied().unwrap_or(0.0) as usize;

        let mut segments = Vec::with_capacity(n);
        for i in 0..n {
            let prefix = format!("mseg_seg{i}_");
            let p = |key: &str, default: f32| -> f32 {
                params.get(&format!("{prefix}{key}")).copied().unwrap_or(default)
            };
            segments.push(MsegSegment {
                duration: p("dur", 0.25),
                v0: p("v0", 0.0),
                v1: p("v1", 1.0),
                curve_type: CurveType::from_index(p("curve", 0.0) as u8),
                cpv: p("cpv", 0.0),
            });
        }

        Self { segments, loop_mode, loop_start, loop_end }
    }
}
