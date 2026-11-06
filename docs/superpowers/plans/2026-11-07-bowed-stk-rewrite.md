# Bowed String STK Rewrite (TECHDEBT #56 roadmap 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-07 (база = master 99df914; факты вычитаны из кода сегодня; спецификация — аудит #56а: однoпетлевой STK-порт сим-верифицирован — лок 0.961–0.994×f0 на всех нотах; research tavily_research 2026-11-06: STK Bowed.cpp/BowTabl.cpp, Maestre 2011 body-банк, JOS PASP)

**Goal:** Смычковая волна-гайд переписана на однoпетлевую STK-топологию (обе
инверсии отражений, дробное чтение на обеих линиях, bow-table по STK, 6-биквад
body-каскад Maestre ВНЕ петли, вибрато): парта играет СВОЮ ноту (сейчас —
чужие высоты до 9.5×f0) и звучит полноценной смычковой струной.

**Architecture:** Полная замена `OscState::BowedString` — старые 15 полей
(две целочисленные линии + один shared allpass + Chamberlin body внутри петли)
заменяются на: две DelayL-линии (линейно-интерполированное дробное чтение),
one-pole loop-loss (pole 0.95, gain 0.95 — STK stringFilter), bow-table
power-law `(0.75+s|Δv|)^−4` (STK BowTabl), 6-биквад body-каскад Maestre ВНЕ
петли, вибрато модуляцией длины nut-линии. Bow velocity под амп-огибающей
(env-проброс `Oscillator::tick(freq, env)` уже существует с #53 Brass).
Питч: `N = sr/f − 2` (компенсация linear-interp + one-pole ≈ 2 сэмпла).

**Tech Stack:** Rust; TDD — частотные пробники через autocorrelation
(паттерн проверен в scratch_mono probe_sf2_poly_control), синтез чисто DSP.

**Проверенные факты (вычитано/сим-верифицировано; не пересматривать):**
- ТЕКУЩЕЕ состояние: note 48 → 1240.7 Гц (9.48×f0), note 60 → 652 Гц
  (2.49×f0) — однoинверсионная петля даёт нечётный субгармонический ряд
  fs/(2(A+B)); режимы плывут с bow_position.
- STK 5.0.1 Bowed.cpp: `bridgeReflection = -stringFilter_.tick(
  bridgeDelay_.lastOut())`; `nutReflection = -neckDelay_.lastOut()` — ОБЕ
  инверсии (round-trip +1 → моды на сетке fs/N); stringFilter OnePole
  pole 0.95, gain 0.95; bowTable: slope = 5 − 4·pressure,
  `out = sample/(slope·|sample| + 0.75)^4`; maxVelocity 0.25; vibrato 6.13 Гц
  модуляцией ДЛИНЫ neckDelay; body = 6-биквад каскад Maestre ВНЕ петли,
  out gain 0.1248.
- Коэффициенты корпуса (violin, Maestre 2011; строки [b0,b1,b2,a1,a2]):
  [1.0, 1.5667, 0.3133, −0.5509, −0.3925], [1.0, −1.9537, 0.9542, −1.6357,
  0.8697], [1.0, −1.6683, 0.8852, −1.7674, 0.8735], [1.0, −1.8585, 0.9653,
  −1.8498, 0.9516], [1.0, −1.9299, 0.9621, −1.9354, 0.9590], [1.0, −1.9800,
  0.9888, −1.9867, 0.9923].
- STK baseDelay: `sr/f − 4` (компенсация string filter + DelayA оверхеда).
  Наша пара: linear-interp + one-pole ≈ 2 сэмпла → `sr/f − 2`.
- `Oscillator::tick(freq, env)` — env-проброс УЖЕ существует (#53 Brass:
  диспетчер oscillator.rs:1042; плечо `OscType::BowedString =>
  self.tick_bowed_string()` :1056 получит второй параметр — компилятор
  перечислит).
- Состояние `OscState::BowedString` (oscillator.rs:264-280): 15 старых полей
  заменяются. Сайты: определение :264, default init :597, match :505,
  init_bowed_string :1527, tick match :1599. Компилятор перечислит все.
- presets/violin.json: bow_position 0.12, bow_pressure 0.5, body_type 0
  (β = FROM BRIDGE после рерайта: 0.12 — Helmholtz-стабильная зона,
  значения НЕ трогаем; трогаем только amp_attack).
- Тест-файл tests/physical_models.rs: helpers `engine_with(name)` (DSP-путь,
  БЕЗ SF2 — bowed чисто движковый), `render(e, note) -> (early, late,
  finite)`.

---

## Test Cases (from the requirement)

Требование (#56а roadmap 1): смычковая струна играет СВОЮ ноту на всём
диапазоне, держится 5 c без NaN, звучит со спектром корпуса.

- TC1 (pitch C4): Violin, note 60, автокорр. доминанта 261.63 ± 13 Гц.
  ДО: 652 Гц (субгармоника).
- TC2 (pitch C3): Violin, note 48, доминанта 130.81 ± 7 Гц. ДО: 1240 Гц.
- TC3 (finite 5s): note 72 хелд 7500 блоков — все сэмплы is_finite.
  (клампы #53 защищали; переписывание не должно сломать).
- TC4 (sustain C3): существующий `tc4_bowed_sustains_at_c3` остаётся
  зелёным (не-регрессия громкости после rewrite).

## Negative Matrix

- velocity = 0 → env 0 → bowVel 0 → force ≈ 0 → затухает (velocity 0 =
  тишина by design).
- delay < 4 сэмплов (ноты > 12 кГц) — clamp `.max(4)` сохраняется.
- body_type 1/2/3 — violin-банк для всех (Out of scope: масштабирование).
- NaN/inf — force: знаменатель ≥ 0.75⁴; клампы и flush канон #53 остаются.
- дубликат NoteOn — voice-level re-key, вне осциллятора.
- RT: арифметика над существующими буферами; rt_safety в гейте.
- вибрато амплитуда 2% длины (≈ 40 центов) —InRange канона 10–40 центов.

---

### Task 1: Частотные regression-тесты (красные)

**Files:**
- Modify: `tests/physical_models.rs`

- [ ] **Step 1: добавить хелпер + 3 теста** (в конец файла):

```rust
/// #56 roadmap 1: доминантная частота позднего окна (блоки 50..=550)
/// автокорреляцией. Lag 140..=500 → 96..343 Гц (C3/C4 + старые жучки).
fn render_freq(e: &mut SynthEngine, note: u8) -> (f32, bool) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut sig: Vec<f32> = Vec::with_capacity(4096);
    let mut finite = true;
    for blk in 0..550 {
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            finite &= l[i].is_finite() && r[i].is_finite();
            if blk >= 50 { sig.push(l[i]); }
        }
    }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note });
    let n = sig.len();
    let mut best = (0.0f32, 0usize);
    for lag in (140..=500).rev() {
        let mut c = 0.0f32;
        for i in lag..n { c += sig[i] * sig[i - lag]; }
        if c > best.0 { best = (c, lag); }
    }
    (48000.0 / best.1 as f32, finite)
}

#[test]
fn tc_b1_pitch_c4_is_nominal() {
    let (f, _) = render_freq(&mut engine_with("Violin"), 60);
    assert!((f - 261.63).abs() <= 13.0, "C4 dominant {f:.1} Hz (expected ~261.6)");
}

#[test]
fn tc_b2_pitch_c3_is_nominal() {
    let (f, _) = render_freq(&mut engine_with("Violin"), 48);
    assert!((f - 130.81).abs() <= 7.0, "C3 dominant {f:.1} Hz (expected ~130.8)");
}

#[test]
fn tc_b3_5s_hold_stays_finite() {
    let mut e = engine_with("Violin");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut finite = true;
    for _ in 0..7500 {
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE { finite &= l[i].is_finite() && r[i].is_finite(); }
    }
    assert!(finite, "5s hold went non-finite");
}
```

- [ ] **Step 2: прогнать — зафиксировать красные**

Run: `cargo test --test physical_models tc_b 2>&1 | tail -10`
Expected: tc_b1/tc_b2 FAIL — доминанты-субгармоники (числа в assert —
диагностика); tc_b3 зелёный (клампы #53). Компиляция чистая.

- [ ] **Step 3: коммит красных**

```bash
git add tests/physical_models.rs && git commit -m "test: bowed pitch regression — subharmonic whistling at C3/C4 (red)"
```

### Task 2: Переписывание BowedString — state + init + tick

**Files:**
- Modify: `src/synth/oscillator.rs` — OscState::BowedString variant (:264-280),
  default init (:597), match (:505), init_bowed_string (:1527),
  tick_bowed_string (:1599), диспетчер env (:1056), tests.rs osc-сигнатуры
  (компилятор перечислит)

- [ ] **Step 1: state variant** — заменить `BowedString { ... }` (:264-280):

```rust
    BowedString {
        bridge_delay: Vec<f32>, // DelayL bridge: запись wpos, чтение
                                // линейной интерполяцией позади (дробная
                                // задержка base_delay·β)
        bridge_wpos: usize,
        bridge_frac: f32,       // дробная часть задержки (0..1)
        nut_delay: Vec<f32>,
        nut_wpos: usize,
        nut_frac: f32,
        nut_vib: f32,           // вибрато: доп. длина nut-линии в сэмплах
        string_filter_state: f32, // one-pole loop loss (pole 0.95, gain 0.95)
        body_bq: [[f32; 5]; 6], // Maestre violin body: [b0,b1,b2,a1,a2] × 6
        body_x1: [f32; 6],
        body_x2: [f32; 6],
        body_y1: [f32; 6],
        body_y2: [f32; 6],
        bow_slope: f32,         // 5 − 4·bow_pressure (STK BowTabl)
        bow_beta: f32,          // bow_position: fraction FROM BRIDGE
        base_delay: f32,        // sr/f − 2 (компенсация фильтра+интерпа)
    },
```

- [ ] **Step 2: все места конструкции** (компилятор перечислит: default init
  :597, match :505, init_bowed_string). Default init — нули и дефолты:

```rust
            BowedString {
                bridge_delay: Vec::new(), bridge_wpos: 0, bridge_frac: 0.0,
                nut_delay: Vec::new(), nut_wpos: 0, nut_frac: 0.0,
                nut_vib: 0.0, string_filter_state: 0.0,
                body_bq: [[0.0; 5]; 6], body_x1: [0.0; 6], body_x2: [0.0; 6],
                body_y1: [0.0; 6], body_y2: [0.0; 6],
                bow_slope: 3.0, bow_beta: 0.127, base_delay: 0.0,
            },
```

Match :505 (reclaim) — имена полей обновить по новой структуре
(компилятор покажет; старые поля nut_pos/bridge_pos/allpass_* удаляются).

- [ ] **Step 3: env-проброс.** Диспетчер :1056:
  `OscType::BowedString => self.tick_bowed_string(env),` — сигнатуру
  `fn tick_bowed_string(&mut self)` → `fn tick_bowed_string(&mut self,
  env: f32)`; env — аргумент диспетчера из #53 (уже существует). Компилятор
  перечислит затронутые osc-тесты (tests.rs osc.tick вызовы — добавить
  `, 1.0`).

- [ ] **Step 4: init_bowed_string** — полная замена тела (чистая версия;
  pool/take_buf/шумовой сид сохраняются — канон #53):

```rust
    pub fn init_bowed_string(
        &mut self,
        freq: f32,
        velocity: f32,
        bow_pressure: f32,
        bow_position: f32,
        body_type: f32,
    ) {
        let _ = body_type; // body bank — violin для всех типов (см. план)
        self.reclaim_buffers();
        let sample_rate = self.sample_rate;
        let base_delay = (sample_rate / freq - 2.0).max(4.0);
        let beta = bow_position.clamp(0.02, 0.98);
        let n_bridge = ((base_delay * beta) as usize).max(2);
        let n_nut = ((base_delay * (1.0 - beta)) as usize).max(2);
        let bridge_frac = (base_delay * beta - n_bridge as f32).max(0.0).min(0.99);
        let nut_frac = (base_delay * (1.0 - beta) - n_nut as f32).max(0.0).min(0.99);
        const BODY_BQ: [[f32; 5]; 6] = [
            [1.0, 1.5667, 0.3133, -0.5509, -0.3925],
            [1.0, -1.9537, 0.9542, -1.6357, 0.8697],
            [1.0, -1.6683, 0.8852, -1.7674, 0.8735],
            [1.0, -1.8585, 0.9653, -1.8498, 0.9516],
            [1.0, -1.9299, 0.9621, -1.9354, 0.9590],
            [1.0, -1.9800, 0.9888, -1.9867, 0.9923],
        ];
        let mut nut_delay = Self::take_buf(&mut self.pool_a, n_nut + 1);
        let mut bridge_delay = Self::take_buf(&mut self.pool_b, n_bridge + 1);
        let seed = 0.005 * velocity;
        let mut rng = 0xDEADBEEFu32;
        for s in nut_delay.iter_mut() {
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            *s = seed * (rng as f32 / u32::MAX as f32 - 0.5) * 2.0;
        }
        for s in bridge_delay.iter_mut() {
            rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5;
            *s = seed * (rng as f32 / u32::MAX as f32 - 0.5) * 2.0;
        }
        self.state = OscState::BowedString {
            bridge_delay,
            bridge_wpos: 0,
            bridge_frac,
            nut_delay,
            nut_wpos: 0,
            nut_frac,
            nut_vib: 0.0,
            string_filter_state: 0.0,
            body_bq: BODY_BQ,
            body_x1: [0.0; 6],
            body_x2: [0.0; 6],
            body_y1: [0.0; 6],
            body_y2: [0.0; 6],
            bow_slope: 5.0 - 4.0 * bow_pressure.clamp(0.0, 1.0),
            bow_beta: beta,
            base_delay,
        };
    }
```

(`bridge_frac`/`nut_frac` — дробные части для linear-interp чтения в tick.)

- [ ] **Step 5: tick_bowed_string** — полная замена тела (сигнатура с env):

```rust
    fn tick_bowed_string(&mut self, env: f32) -> f32 {
        // Дробное чтение позади wpos (linear interp)
        fn delay_read(buf: &[f32], wpos: usize, delay: f32) -> f32 {
            let n = buf.len();
            let r = wpos as f32 - delay;
            let i0f = r.floor();
            let frac = r - i0f;
            let i0 = (i0f as isize).rem_euclid(n as isize) as usize;
            let i1 = (i0 + 1) % n;
            buf[i0] + frac * (buf[i1] - buf[i0])
        }
        fn write(buf: &mut [f32], wpos: &mut usize, v: f32) {
            buf[*wpos] = v;
            *wpos = (*wpos + 1) % buf.len();
        }

        if let OscState::BowedString {
            bridge_delay, bridge_wpos, bridge_frac,
            nut_delay, nut_wpos, nut_frac, nut_vib,
            string_filter_state,
            body_bq, body_x1, body_x2, body_y1, body_y2,
            bow_slope, ..
        } = &mut self.state
        {
            const FLUSH: f32 = 1e-20;
            let flush = |x: f32| if x.abs() < FLUSH { 0.0 } else { x };

            // Вибрато: модуляция длины nut-линии (STK: neckDelay.setDelay)
            *nut_vib = 0.02 * (*vib_phase_v() * std::f32::consts::TAU).sin();
            *vib_phase_v() += 6.13 / self.sample_rate;
            if *vib_phase_v() >= 1.0 { *vib_phase_v() -= 1.0; }

            // 1) Отражения: ОБЕ инверсии (STK Bowed.cpp) — иначе
            // субгармонический ряд (чужие высоты, #56а)
            let nut_d = *base_delay * (1.0 - *bow_beta) * (1.0 + *nut_vib);
            let bridge_d = *base_delay * *bow_beta;
            let nut_refl = -flush(delay_read(nut_delay, *nut_wpos, nut_d));
            let br_raw = flush(delay_read(bridge_delay, *bridge_wpos, bridge_d));

            // 2) Loop loss: one-pole pole 0.95, gain 0.95 (STK stringFilter)
            *string_filter_state =
                flush(0.05 * br_raw + 0.95 * *string_filter_state);
            let br_refl = -0.95 * *string_filter_state;

            // 3) Bow table (STK BowTabl): slope = 5 − 4·pressure;
            //    bow velocity под огибающей (STK: maxVel·adsr)
            let bow_vel = 0.3 * env;
            let v_delta = bow_vel - (br_refl + nut_refl);
            let force = v_delta / ((*bow_slope * v_delta.abs() + 0.75).powi(4));

            // 4) Инжекция в обе линии (STK-канон: force в ОБЕ)
            let nut_in = flush(br_refl + force);
            let br_in = flush(nut_refl + force);
            write(nut_delay, nut_wpos, nut_in);
            write(bridge_delay, bridge_wpos, br_in);

            // 5) Body cascade Maestre — ВНЕ петли, на сыром bridge-выходе
            let mut b_in = br_raw;
            for (i, c) in body_bq.iter().enumerate() {
                let y = c[0] * b_in + c[1] * body_x1[i] + c[2] * body_x2[i]
                    - c[3] * body_y1[i] - c[4] * body_y2[i];
                body_x2[i] = body_x1[i]; body_x1[i] = b_in;
                body_y2[i] = body_y1[i]; body_y1[i] = y;
                b_in = y;
            }

            // Выход: сырой bridge-сигнал + корпус (вне петли, канон STK)
            (br_raw + b_in * 0.1248 * 5.0).clamp(-1.0, 1.0)
        } else {
            0.0
        }
    }
```

Выходной масштаб `0.1248 * 5.0` = 0.624 — нормировка Maestre out gain
компенсируется усилением сырой линии (подобрать по tc4-громкости; если
tc4/sustain провалится по громкости — множитель 5.0 → 8.0, пороги тестов
НЕ ослаблять).

- [ ] **Step 6: прогнать — все зелёные**

Run: `cargo test --test physical_models 2>&1 | tail -4`
Expected: весь файл ok (новые tc_b1-tc_b3 + существующие tc_bowed_sustains
и физ-модели других семейств).

- [ ] **Step 7: коммит**

```bash
git add src/synth/oscillator.rs src/synth/tests.rs tests/physical_models.rs && git commit -m "fix: bowed string — STK single-loop rewrite: both reflections inverted (correct pitch), fractional delays, STK bow table, Maestre body bank outside loop, vibrato (#56 roadmap 1)"
```

### Task 3: Пресеты — bow attack (живой смысл смычка)

**Files:**
- Modify: bowed-пресеты (`presets/violin.json`, `violin_v2.json`,
  `viola.json`, `double_bass.json` + _v2-варианты) — `amp_attack` 0.08 → 0.2
  (STK: bow onset ~200 мс; мгновенная атака = synth tell)

- [ ] **Step 1: правка** — только пресеты с `"osc_type": 14`:

Run: `grep -l '"osc_type": *14' presets/*.json`
Ожидание: violin/violin_v2/viola/double_bass (+_v2). В каждом —
`"amp_attack"` → 0.2.

- [ ] **Step 2: быстрый гейт**

Run: `cargo test --test perf_load_seq 2>&1 | tail -3 && cargo test --test physical_models 2>&1 | tail -3`
Expected: зелёные (пресеты парсятся).

- [ ] **Step 3: коммит**

```bash
git add presets/ && git commit -m "fix: bowed presets — bow onset attack 0.2 s (STK startBowing canon; instant attack = synth tell)"
```

### Task 4: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): Violin/Cello на C3/C4 — СВОИ ноты, полноценный
  смычковый тембр; вибрато MW-колесом работает на bowed-партах
- [ ] TECHDEBT #56а: отметить roadmap 1 ГОТОВО

## Out of scope

- Brass lip drive (роадмап 2), Brass pitch-lock (роадмап 4), Slap (роадмап 3)
  — отдельные планы (env-проброс уже общий — план 2 переиспользует).
- body_type-специфичные банки (cello/bass масштабирование Maestre-коэфф.)
  — после listening-пасса юзера.
- Preset retune bow_pressure — по фидбеку после прослушки.
- PT- вибрато depth в параметры парта — пока константа 2% длины.
