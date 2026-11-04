# Physical Models Fix (TECHDEBT #53): Brass / Bowed / Slap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-03 (база = master 3982fe2; полный аудит #53: Brass-аудит с f32-симуляцией + свип 884 пресетов (/tmp/dsp_sweep.txt) + Bowed/Slap-аудит с посим-верифицированными числами; практики — Tavily 2026-11-03: STK Brass/Bowed (CCRMA, `startBlowing/startBowing` = давление/смычок через ADSR-рампу), dsprelated PASP Bowed Strings (bow-table, noise-сеялка KS-канона), DAFx'20 lip-reed (коллизия с мягкой жёсткостью, без знакового обрушения))

**Goal:** Устранить три ядерных дефекта физ-моделей: Brass молчит+щёлкает ниже
C5 (все 8 патчей), BowedString полностью молчит на C3 (все 7 патчей),
BassGuitar slap затухает за 100 мс с полнодиапазонным щелчком.

**Architecture:** Только `src/synth/oscillator.rs` и `src/synth/bass.rs`.
Brass: мягкое насыщение рассеивающей нелинейности (устойчивая мёртвая точка →
неустойчивая), давление мундштука через амп-огибающую (канон STK), denormal-
гарды. Bowed: шумовой сид延迟-линий (канон KS), питч-компенсированный gain
трения, починка сломанного Chamberlin-уравнения, клампы + denormal-гарды +
non-finite щит (внутренние состояния улетают в inf/NaN за 2-3 c!). Slap:
коллизия с ладом — сжимающая (без знакового обрушения) и амплитудно-
адекватная, щелчок тише. Структурная переделка Bowed (корректная высота тона
мод) — Out of scope (день работы, отдельный план).

**Tech Stack:** Rust; регрессионные тесты — engine-level амплитудные пробы
(паттерн tests/layer_switch_debug.rs, звуковой файл НЕ нужен — все три модели
чисто DSP), позднее окно рендера (блоки 100..=600) для отличия сустейна от
щелчка; finiteness-тест на 5 c хелда.

**Проверенные факты (аудит + симуляции, не пересматривать):**
- **Brass** (oscillator.rs:1644-1788): нелинейность `delta = min(bq_y², 1)`
  (:1767) имеет DC-усиление петли 0 в насыщении → устойчивая мёртвая точка;
  DC-gain лип-биквака ≈ 1/ω₀² (36 на A3) → вылет при любых параметрах ниже
  B4 (`max_pressure ≥ 0.55 > 0.093`); `blowing_pressure` хранится константой
  (:1718) и используется без огибающей (:1757); щелчок = раунд-трип DC-ступеньки
  (пик 0.75, 9 мс); denormals без гардов (2.6 c субнормальной арифметики/нота).
  Эмпирика движка: notes 45..67 peak 0.05-0.12 (щелчок), 68..79 peak 0.43-0.53.
- **Bowed** (oscillator.rs:1521-1642): excitation starvation — bow_velocity
  0.3 в хвостовой зоне friction-кривой (f(0.3)=3e-5 при max 0.0146), delay-линии
  нулевые (take_buf zero-fill :460-465); per-period gain 1.106 не зависит от
  питча → lock-время в периодах (77 периодов = 0.59 c на C3 → «пик 0.004» в
  свипе); Chamberlin-баг :1623-1625 (`bp` перезаписывается через `(1+w)·hp`,
  утечка DC ≈ 1 вместо 0, раздувает петлю); внутренние состояния (nut_filter,
  delay-линии, bridge_bp) НЕ клампятся — inf на 3.09 c (note 60), NaN на 1.94 c
  (note 72) → NaN в весь микс; denormal-гардов нет. Сим-верифицировано:
  noise-сид ±0.005 + friction-gain k=clamp(n_total/183,1,3) → note 48 слышен
  на 0.1 c, ограничен.
- **Slap** (bass.rs): `fret_height = 0.05` (:306) — коллизия :439-443
  (`|x|>0.05 → sign(x)·(0.05−(|x|−0.05)·0.85)`) — давилка с фиксированной
  точкой 0.027 и знаковым флипом выше 0.109; сим: fret=0.05 затухает
  1.0→0.04 за 0.1 c, fret=0 сустейнится как finger (0.37@0.1c);
  `slap_click_amp = velocity·0.5` (:361) — полнодиапазонный щелчок до SVF
  (Q≈2.8, овершут) и атаки 0.001 c.
- Тестовый паттерн: engine + `SetPartEnabled/SetPartRange/LoadPatch
  (LoadPatchEvent { params: PatchParams::from_map(&patch.params), ..None })` +
  NoteOn + tick_block (работает в tests/scratch-пробах этого сеанса).
- Специфические ключи params: brass `lip_tension/blowing_pressure/bell_type`
  (patch_params.rs:25); bowed `bow_pressure/bow_position/body_type`
  (:320); DAFx-канон коллизии: мягкая жёсткость, без знакового флипа.

**Best practices (Tavily, вшиты в план):** STK `Brass.cpp`:
`breathPressure = maxPressure * adsr.tick()` — давление всегда под огибающей;
STK `Bowed.cpp`: `startBowing(amplitude, rate)` — рампа смычка; PASP (Smith):
bow-table с плоской центральной зоной (stick-reflection) и шумовым
возбуждением задержек; KS-канон: начальный шумовой буфер; DAFx'20 lip-reed:
коллизия = мягкая пружина, не знаковый обрыв.

---

## Test Cases (from the requirement)

Требование: физ-модельные патчи звучат во всём рабочем диапазоне нот, без
щелчков на атаке, без NaN в миксе.

- TC1 (Brass sustain): French Horn, note 60, ПОЗДНИЙ пик (блоки 100..=600)
  > 0.05. ДО фикса ≈ 0 (щелчок в раннем окне).
- TC2 (Brass low): French Horn, note 48, поздний пик > 0.03. ДО фикса ≈ 0.
- TC3 (Brass click): French Horn, note 60: ранний пик (блоки 0..30) ≤ 0.9
  и ранний/поздний ≤ 3 (атака без взрыва). ДО фикса ранний ≥ 0.7 при
  позднем ≈ 0.
- TC4 (Bowed low): Violin, note 48, поздний пик > 0.05. ДО фикса ≈ 0.004.
- TC5 (Bowed finite): Violin, note 72 хелд 5 c — каждое выходное значение
  `is_finite()` на протяжении хелда. ДО фикса NaN на ~1.94 c.
- TC6 (Slap sustain): «Bass Guitar (Slap)», note 48, поздний пик ≥ 0.08.
  ДО фикса ≈ 0.04 (давилка).
- TC7 (Slap click): slap note 48 ранний пик ≤ 0.35 (было 0.5+).
- TC8 (не-регрессия): finger-вариант «Bass Guitar», note 48 поздний ≥ 0.1
  (коллизия/клик-правки не трогают finger/pick); Trumpet note 72 поздний
  ≥ 0.2 (рабочая полоса не сломана).

## Negative Matrix

- NaN/inf на входе нелинейности — мягкое насыщение s/(1+s) определено при
  любых конечных s; non-finite щит на выходе диспетчера ловит остаточное.
- denormal-состояния — явные флэши `if x.abs() < 1e-20 { 0.0 }` на
  перечисленных состояниях.
- velocity=0 (клавиша без velocity — наш кейс, всегда 127, но velocity=0
  теоретически) — `velocity.max(0.1)` множители в сиде/давлении: модель
  тихая, не мёртвая (сид ~0 — допустимо: velocity 0 = тишина by design).
- Все параметры из перф-оверрайдов (bass_test-кейс) — фиксы на уровне
  модели, не зависят от значений lip_tension/pressure (проверено аудитом:
  вылет при ЛЮБЫХ p/t).
- 5-сек тест не должен флейкнуть по времени: фиксированное число блоков
  (7500), без Realtime.
- CLICK-ONLY классификация свипа vs позднее окно — тесты используют окно
  ≥ 100 блоков, щелчок (≤ 45 сэмплов + DC-хвост ~1400 сэмплов ≈ 44 блока)
  гарантированно вне окна.
- Не-регрессия рабочей полосы Brass (72+) — TC8.
- Surge-WT и обычные осцилляторы не затронуты — правки только в трёх
  несущих функциях (осциллятор-диспетчер: один non-finite щит).

---

### Task 1: Regression-тесты (TDD — красные до фикса)

**Files:**
- Create: `tests/physical_models.rs`

- [ ] **Step 1: написать тесты** (паттерн — tests/scratch-пробы этого сеанса,
  проверены компиляцией и запуском):

```rust
//! TECHDEBT #53 regression probes: Brass blowout, Bowed low-pitch silence +
//! NaN, BassGuitar slap crusher. Late-window peaks (blocks 100..=600)
//! distinguish sustain from onset clicks. Red before the fix, green after.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn engine_with(patch_name: &str) -> SynthEngine {
    let patches = preset::load_factory_patches(); // hermetic (preset.rs:560)
    let idx = patches.iter().position(|p| p.name == patch_name)
        .unwrap_or_else(|| panic!("patch {patch_name} must exist"));
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches.clone());
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 127 });
    let ev = ControlEvent::LoadPatch(Box::new(mini_midi_synth::synth::LoadPatchEvent {
        part: 0,
        params: mini_midi_synth::synth::PatchParams::from_map(&patches[idx].params),
        mod_matrix: Default::default(),
        mseg1: None, mseg2: None, pitch_seq: None, lfo_step_seq: None, wavetable: None,
    }));
    let _ = e.handle_control(ev);
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    e
}

/// (early [0..30], late [100..=600]) peaks; note held the whole time.
fn render(e: &mut SynthEngine, note: u8) -> (f32, f32, bool) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let (mut early, mut late) = (0.0f32, 0.0f32);
    let mut finite = true;
    for blk in 0..600 {
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            let a = l[i].abs().max(r[i].abs());
            finite &= l[i].is_finite() && r[i].is_finite();
            if blk < 30 { early = early.max(a); }
            if blk >= 100 { late = late.max(a); }
        }
    }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note });
    let mut l2 = [0.0f32; BLOCK_SIZE]; let mut r2 = [0.0f32; BLOCK_SIZE];
    for _ in 0..200 { e.tick_block(&mut l2, &mut r2); }
    (early, late, finite)
}

#[test]
fn tc1_brass_sustains_at_c4() {
    let (early, late, _) = render(&mut engine_with("French Horn"), 60);
    assert!(late > 0.05, "brass silent at C4: early {early:.3} late {late:.3}");
}

#[test]
fn tc2_brass_sustains_at_c3() {
    let (_, late, _) = render(&mut engine_with("French Horn"), 48);
    assert!(late > 0.03, "brass silent at C3: late {late:.3}");
}

#[test]
fn tc3_brass_attack_not_a_blast() {
    let (early, late, _) = render(&mut engine_with("French Horn"), 60);
    assert!(early <= 0.9, "brass onset blast: early {early:.3}");
    assert!(early <= late * 3.0 + 0.1, "onset dominates: early {early:.3} late {late:.3}");
}

#[test]
fn tc4_bowed_sustains_at_c3() {
    let (_, late, _) = render(&mut engine_with("Violin"), 48);
    assert!(late > 0.05, "bowed silent at C3: late {late:.3}");
}

#[test]
fn tc5_bowed_stays_finite_5s() {
    let mut e = engine_with("Violin");
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE];
    let mut finite = true;
    for _ in 0..7500 { // 5 c
        l.fill(0.0); r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE { finite &= l[i].is_finite() && r[i].is_finite(); }
    }
    assert!(finite, "bowed output went non-finite during 5s hold");
}

#[test]
fn tc6_slap_sustains() {
    let (_, late, _) = render(&mut engine_with("Bass Guitar (Slap)"), 48);
    assert!(late >= 0.08, "slap decays instantly: late {late:.3}");
}

#[test]
fn tc7_slap_click_tamed() {
    let (early, _, _) = render(&mut engine_with("Bass Guitar (Slap)"), 48);
    assert!(early <= 0.35, "slap onset blast: early {early:.3}");
}

#[test]
fn tc8_no_regression_finger_and_high_brass() {
    let (_, late_f, _) = render(&mut engine_with("Bass Guitar (Finger)"), 48);
    assert!(late_f >= 0.1, "finger bass regressed: late {late_f:.3}");
    let (_, late_b, _) = render(&mut engine_with("Trumpet"), 72);
    assert!(late_b >= 0.2, "brass working band regressed: late {late_b:.3}");
}
```

- [ ] **Step 2: прогнать — зафиксировать красные**

Run: `cargo test --test physical_models -- --nocapture 2>&1 | tail -25`
Expected: tc1/tc2/tc4/tc5 FAIL (тишина/NaN), tc6 FAIL (давилка), tc3/tc7
FAIL (взрыв атаки), tc8 PASS (бейзлайн не-регрессии). Если расклад иной —
НЕ подкручивать пороги: приложить фактические пики и отчитаться BLOCKED.

- [ ] **Step 3: коммит красных**

```bash
git add tests/physical_models.rs && git commit -m "test: physical-model regression probes — brass/bowed silence, slap crusher (red)"
```

### Task 2: Brass — мягкое насыщение + давление под огибающей + денормал-гарды

**Files:**
- Modify: `src/synth/oscillator.rs` (Brass state init ~:1709, `blowing_pressure`
  хранение :1718, `tick_brass` :1733-1788), `src/synth/voice.rs`
  (render_oscs/dispatch env-проброс), `src/synth/tests.rs` (:184/:204/:222 —
  osc.tick(440.0) → osc.tick(440.0, 1.0); компилятор перечислит)

- [ ] **Step 1: давление через амп-огибающую (STK-канон).** Цепочка env:
  в voice.rs локальная `amp` — УЖЕ ОТТИКАННОЕ значение `self.amp_env.tick()`
  (voice.rs:601, до render_oscs :646/:740). КРИТИЧНО: НЕ тикать amp_env ещё
  раз — ADSR stateful, двойной tick = вдвое быстрее огибающая у ВСЕХ патчей.
  Риппл: `render_oscs(freq, amp)` (+ унисон-ветка :659) →
  `Oscillator::tick(freq, env)` (диспетчер oscillator.rs:1042, ОДИН match) —
  только плечо Brass пересылает env в tick_brass, остальные осцилляторы
  игнорируют параметр (передаётся, не используется). Сигнатурный риппл
  компилятор перечислит. В состоянии Brass добавить поле:

```rust
    pressure_env: f32, // текущее давление мундштука (рампа к цели)
```

в `init_brass` — `pressure_env: 0.0` (поле в деструктуризации
`if let OscState::Brass { .. } = &mut self.state` — мутации через
`*pressure_env`); в tick_brass (функция :1733-1788) заменить прямое
использование `self.blowing_pressure` (:1757) на рампу с коэффициентом,
выведенным из SR (τ ≈ 5 мс; `self.sample_rate` — поле `Oscillator`,
доступно рядом с `&mut self.state` — disjoint borrow):

```rust
        // STK-канон: breathPressure = maxPressure * adsr.tick() — давление
        // мундштука под огибающей, иначе DC-ступенька бьёт в бору щелчком.
        // Однополюсная рампа с τ=5мс: coeff = 1 - exp(-1/(0.005*sr))
        let coeff = 1.0 - (-1.0 / (0.005 * self.sample_rate)).exp();
        *pressure_env += coeff * (self.blowing_pressure * env - *pressure_env);
        let blowing = *pressure_env;
```

(`env` — аргумент tick_brass; `self.sample_rate` — поле OscState, проверить
фактическое имя, если нет — пробросить или использовать конст SR-эквивалент
из файла.) Все места tick_brass, использовавшие `self.blowing_pressure` как
давление, используют локальную `blowing`.

- [ ] **Step 2: мягкое насыщение.** В tick_brass заменить (:1767):

```rust
                    let delta = (bq_y * bq_y).min(1.0);
```

на

```rust
                    // Мягкое насыщение вместо жёсткого клампа: жёсткий кламп
                    // имеет нулевую производную — устойчивая мёртвая точка
                    // (тишина ниже B4, #53). s/(1+s) выводит из любой точки.
                    let s = bq_y * bq_y;
                    let delta = s / (1.0 + s);
```

- [ ] **Step 3: денормал-гарды** — в конце tick_brass, для состояний
  `bell_hp_state`, `bore_lp_state` (точные имена по коду ~:1770-1788) и по
  bore-буферу при записи:

```rust
                    let flush = |x: f32| if x.abs() < 1e-20 { 0.0 } else { x };
```

(применить к каждому записываемому состоянию; for-цикл по bore — флэш
значения перед записью).

- [ ] **Step 4: зелёный Brass-блок + не-регрессия**

Run: `cargo test --test physical_models -- --nocapture 2>&1 | grep -E "tc[1238]|test result" | head -8`
Expected: tc1/tc2/tc3/tc8 зелёные; tc4-tc7 ещё красные (Bowed/Slap — Task 3/4).

- [ ] **Step 5: коммит**

```bash
git add src/synth/oscillator.rs src/synth/voice.rs src/synth/tests.rs && git commit -m "fix: Brass lip model — soft saturation kills the dead point, breath pressure under amp env, denormal guards (#53)"
```

### Task 3: Bowed — сид, питч-компенсация, Chamberlin, клампы

**Files:**
- Modify: `src/synth/oscillator.rs` (init_bowed_string ~:1560-1580,
  tick_bowed_string ~:1595-1642)

- [ ] **Step 1: шумовой сид delay-линий** — в init_bowed_string после
  zero-fill линий (найти take_buf/zero-init):

```rust
        // KS-канон: шумовой сид — без него excitation starvation
        // (bow_velocity 0.3 в хвосте friction-кривой, f≈3e-5)
        let seed = 0.005 * velocity;
        for s in nut_delay.iter_mut() { *s = seed * (rand_term()); }
        for s in bridge_delay.iter_mut() { *s = seed * (rand_term()); }
```

Источник термов — СУЩЕСТВУЮЩИЙ в файле идиом bore-сида (init_brass
:1700-1704) / `Oscillator::next_noise` (oscillator.rs:1035) — тот же паттерн
применить к обеим линиям (LCG-вариант из лоб: `(i * 2654435761) >> 16` в
usize даёт термы до ±600 — НЕ использовать). `velocity` в скоупе init —
проверить сигнатуру; если нет — взять 1.0 с комментарием.

- [ ] **Step 2: питч-компенсированный gain трения** — в tick_bowed_string,
  где friction вычислен (после `f = p·v_rel·exp(...)`):

```rust
        // Питч-компенсация: per-period loop gain не зависит от питча →
        // lock-время в периодах (77 периодов = 0.59 c на C3). k делает
        // его константным в секундах (Brass-аналог, #53).
        let k = (n_total / 183.0).clamp(1.0, 3.0);
        let f = f * k;
```

(`n_total` = `n_nut + n_bridge` — в tick_bowed_string есть только компоненты
:1584-1585; вычислить локально один раз.)

- [ ] **Step 3: починка Chamberlin** (:1623-1625) — заменить:

```rust
            *bridge_bp = *bridge_lp - *bridge_bp * q_inv;
            let body_out = *bridge_bp;
            *bridge_bp += w * body_out;
```

на

```rust
            // Канонический Chamberlin: bp накапливает w*hp ПОВЕРХ bp_old,
            // иначе (1+w)·hp + утечка lp с единичным DC-гейном в петлю
            let hp = *bridge_lp - *bridge_bp * q_inv;
            *bridge_bp += w * hp;
            let body_out = hp;
```

- [ ] **Step 4: клампы + флэши + non-finite щит:**

```rust
            // Анти-blowup: внутренние состояния улетали в inf (3.09 c) / NaN
            const BOWED_CLAMP: f32 = 4.0;
            *nut_filter = nut_filter.max(-BOWED_CLAMP).min(BOWED_CLAMP);
            // ... записи в delay-линии клампятся тем же констом ...
```

плюс флэш `if x.abs() < 1e-20 { 0.0 }` на `nut_filter`, `bridge_lp`,
`bridge_bp` при записи; плюс в диспетчере `Oscillator::tick` (oscillator.rs:1042 — плечи match
возвращают выражение напрямую, привязать `let out = match ...;` перед
возвратом) общий щит:

```rust
    let out = if out.is_finite() { out } else { 0.0 };
    out
```

- [ ] **Step 5: зелёный Bowed + 5s-finite**

Run: `cargo test --test physical_models -- --nocapture 2>&1 | grep -E "tc[45]|test result" | head -5`
Expected: tc4/tc5 зелёные.

- [ ] **Step 6: коммит**

```bash
git add src/synth/oscillator.rs && git commit -m "fix: BowedString — noise seed + pitch-compensated friction (C3 audible), Chamberlin bp fixed, clamp+denormal+non-finite guards (#53)"
```

### Task 4: Slap — сжимающая коллизия + клик потише

**Files:**
- Modify: `src/synth/bass.rs` (:306 fret_height, :361 slap_click_amp, :439-443
  collision)

- [ ] **Step 1: коллизия** — заменить (:439-443):

```rust
                    // DAFx-канон: коллизия с мягкой жёсткостью — сжимает, но
                    // не обрушивает в фиксированную точку и не флипает знак
                    // (старая давилка: |x|>fh → fh−(x−fh)·0.85, FP 0.027,
                    // знак флипался выше 1.33·fh — slap умирал за 100 мс)
                    let fh = self.fret_height;
                    if x.abs() > fh {
                        let d = x.abs() - fh;
                        y = x.signum() * (fh + d * self.fret_damping);
                    } else {
                        y = x;
                    }
```

(`x`/`y` — фактические имена переменных collision-ветки (:439-443 —
`loop_out`/`self.fret_height`); сохранить окружающий код. `fret_damping`
(bass.rs:307, =0.85) в init Slap ставить 0.15 — поле живёт, не dead.)

- [ ] **Step 2: fret_height амплитудно-адекватный** (:306, фактическая строка
  `self.fret_height = if style == BassStyle::Slap { 0.05 } else { 0.0 };` —
  style-guard СОХРАНИТЬ, иначе finger/pick получат коллизию):

```rust
        self.fret_height = if style == BassStyle::Slap {
            0.45 * velocity.max(0.5) // было 0.05 — ниже рабочей амплитуды струны
        } else {
            0.0
        };
```

- [ ] **Step 3: клик** (:361): `velocity * 0.5` → `velocity * 0.15` с
  комментарием (STK-канон: эксайтер маскируется телом ноты).

- [ ] **Step 4: зелёный Slap + не-регрессия + полный тест-файл**

Run: `cargo test --test physical_models 2>&1 | tail -4`
Expected: 8/8 ok. КОНТИНГЕНЦИЯ TC7: убрав давилку, поднимается всё начало
(инжекция velocity·1.1, pickup читается ДО коллизии bass.rs:399-418) —
если ранний пик > 0.35 уже от тела ноты, а не щелчка: снизить эксайт
slap-ветки (инжекция 1.1 → ~0.6) и перезапустить; порог не ослаблять молча —
коммит с комментарием о причине.

- [ ] **Step 5: коммит**

```bash
git add src/synth/bass.rs && git commit -m "fix: BassGuitar slap — compressive fret collision instead of sign-flip crusher, click tamed (#53)"
```

### Task 5: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): perf bass_test A3 French Horn — звучит на любых нотах, без
  щелчков; Violin/Cello на низкой октаве — звучит; Bass Guitar (Slap) — нота,
  не щелчок; остальные физ-патчи не изменились к худшему
- [ ] TECHDEBT #53: пометить ГОТОВО (Brass/Bowed/Slap), кроме Out of scope

## Out of scope

- Структурная переделка Bowed (одна n_total-петля, r_nut·r_bridge = +1) —
  питч-корректность мод (сейчас октавная ошибка на низких нотах даже после
  фикса: моды задаются bow_position, не длиной струны). День работы,
  отдельный план. Аудит: finding A3.
- Metal Kick (Vinnie Paul) note-72 клик и Granular Drone — конфиги патчей
  (аудит Task C), не код.
- Surge-«тихие» пресеты — настройки огибающих, не дефект движка.
