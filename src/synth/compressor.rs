//! Feed-forward compressor with peak detection, attack/release, knee.

pub struct Compressor {
    envelope: f32,
    sample_rate: f32,
    pub threshold: f32,  // dB (-60..0)
    pub ratio: f32,      // 1..20 (20 = limiter)
    attack: f32,         // seconds (0.001..0.1)
    release: f32,        // seconds (0.01..1.0)
    attack_coeff: f32,
    release_coeff: f32,
    pub makeup: f32,     // dB (0..24)
    pub mix: f32,
    /// Control-rate gain computer (Ramos DAFx 2011): dB-математика раз в
    /// 64 сэмпла, между обновлениями — линейный ramp гейна.
    gain: f32,
    gain_step: f32,
    blocks_left: u8,
}

const GAIN_UPDATE_INTERVAL: u8 = 64;

impl Compressor {
    pub fn new(sample_rate: f32) -> Self {
        let attack = 0.001_f32;
        let release = 0.08_f32;
        let mut s = Self {
            envelope: 0.0,
            sample_rate,
            threshold: -6.0,
            ratio: 20.0,
            attack,
            release,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            makeup: 0.0,
            mix: 1.0,
            gain: 1.0,
            gain_step: 0.0,
            blocks_left: 0,
        };
        s.update_coeffs();
        s
    }

    fn update_coeffs(&mut self) {
        self.attack_coeff  = 1.0 - (-1.0 / (self.attack.max(0.0001)  * self.sample_rate)).exp();
        self.release_coeff = 1.0 - (-1.0 / (self.release.max(0.0001) * self.sample_rate)).exp();
    }

    #[allow(dead_code)]
    pub fn set_attack(&mut self, v: f32) { self.attack = v; self.update_coeffs(); }
    #[allow(dead_code)]
    pub fn set_release(&mut self, v: f32) { self.release = v; self.update_coeffs(); }
    #[allow(dead_code)]
    pub fn attack(&self) -> f32 { self.attack }
    #[allow(dead_code)]
    pub fn release(&self) -> f32 { self.release }

    /// Поведенческая дельта (ревью): при makeup>0 первый gain-глид 1.0→makeup
    /// занимает 64 сэмпла (~1.3 мс @48k) после создания или смены SR — это
    /// не мгновенный скачок, как раньше.
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.envelope = 0.0;
        self.gain = 1.0;
        self.gain_step = 0.0;
        self.blocks_left = 0;
        self.update_coeffs();
    }

    /// Пересчёт целевого гейна (control-rate): dB-математика здесь и только здесь.
    fn update_gain_target(&mut self) {
        let env_db = if self.envelope > 0.00001 {
            20.0 * self.envelope.log10()
        } else {
            -100.0
        };
        let knee_width: f32 = 6.0;
        let knee_half = knee_width * 0.5;
        let over = env_db - self.threshold;
        let gain_reduction_db = if over < -knee_half {
            0.0
        } else if over < knee_half {
            let t = over + knee_half;
            (1.0 / self.ratio - 1.0) * t * t / (2.0 * knee_width)
        } else {
            (1.0 / self.ratio - 1.0) * over
        };
        let target = 10.0_f32.powf((gain_reduction_db + self.makeup) / 20.0);
        // Линейный ramp от текущего гейна к целевому за интервал.
        self.gain_step = (target - self.gain) / GAIN_UPDATE_INTERVAL as f32;
        self.blocks_left = GAIN_UPDATE_INTERVAL;
    }

    #[inline]
    pub fn tick(&mut self, in_l: f32, in_r: f32) -> (f32, f32) {
        if self.mix < 0.001 { return (in_l, in_r); }

        // Peak detection + one-pole envelope — per-sample (дёшево: abs + fma)
        let input_level = in_l.abs().max(in_r.abs());
        let coeff = if input_level > self.envelope { self.attack_coeff } else { self.release_coeff };
        self.envelope += coeff * (input_level - self.envelope);

        // Control-rate gain computer
        if self.blocks_left == 0 {
            self.update_gain_target();
        }
        self.blocks_left -= 1;
        self.gain += self.gain_step;

        let wl = in_l * self.gain;
        let wr = in_r * self.gain;
        let m = self.mix;
        (in_l * (1.0 - m) + wl * m, in_r * (1.0 - m) + wr * m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// TC1: новая реализация держит тот же steady-state гейн, что старая
    /// формула (±0.35 dB), ниже порога / в колене / выше порога.
    #[test]
    fn steady_state_gain_matches_reference() {
        let cases: [(f32, f32); 4] = [
            (0.0,  0.0),          // тишина: envelope→0, target = makeup dB
            (0.05, 0.0),          // ниже порога: gain = makeup dB
            (0.5,  0.0),          // в колене (env_db ≈ −6.02, over ≈ −0.02)
            (0.90, 0.0),          // выше порога
        ];
        for (level, _label) in cases {
            let mut c = Compressor::new(48000.0);
            // Прогрев: держим уровень, ждём установления ramp'ов
            for _ in 0..48000 { c.tick(level, level); }
            // Отклонение от кода теста в плане: при level==0 выход всегда 0
            // (0 * gain), измерить гейн по отношению l/level нельзя (даёт -inf
            // при любом гейне). Вместо этого — одиночный пробный сэмпл 1.0:
            // выход равен текущему applied gain.
            let last;
            if level > 0.0 {
                let mut acc = level;
                for _ in 0..48000 {
                    let (l, _) = c.tick(level, level);
                    acc = l / level;
                }
                last = acc;
            } else {
                for _ in 0..48000 { c.tick(level, level); }
                // выход = gain * 1.0
                let (l, _) = c.tick(1.0, 1.0);
                last = l;
            }
            let got_db = 20.0 * last.log10();
            // Референс: старая формула на установившемся envelope≈level
            let env_db = if level > 0.0 { 20.0 * level.log10() } else { -100.0 };
            let over = env_db - c.threshold;
            let kw = 6.0;
            let red = if over < -kw / 2.0 { 0.0 }
                else if over < kw / 2.0 { (1.0 / c.ratio - 1.0) * (over + kw / 2.0).powi(2) / (2.0 * kw) }
                else { (1.0 / c.ratio - 1.0) * over };
            let want_db = red + c.makeup;
            assert!((got_db - want_db).abs() < 0.35, "level {level}: got {got_db:.2} dB, want {want_db:.2} dB");
        }
    }
}
