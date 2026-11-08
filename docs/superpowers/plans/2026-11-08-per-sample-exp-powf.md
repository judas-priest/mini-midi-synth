# Per-sample exp/powf: bit-exact hoists Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 8411332; ключевой сайт bbd_ensemble.rs:90
подтверждён чтением; остальные именованные кластером сайты (exciter 73-75/169-170,
vocoder 179-181, resonator at_alpha) оказались per-BLOCK — проверены, не
нарушители; полный скоуп уточняет Task 2-аудит)

**Goal:** убрать инвариантные exp/powf вычисления из per-sample циклов —
ТОЛЬКО бит-точными hoist'ами (коэффициент зависит от sr/параметров, не от
сэмпла → вычислить раз на блок/ноту). Аппроксимации (LUT/fast_exp) — ЗАПРЕЩЕНЫ
без юзер-решения: goldens бит-точны, airwindows порт 1:1.

**Architecture:** hoist инварианта из цикла в локальную переменную перед
циклом / в per-block set_params. Zero sound change → goldens байт-стабильны.

**Tech Stack:** Rust; goldens как главный гейт; тесты не нужны (pure hoist,
компилятор+goldens).

**Проверенные факты (8411332):**
- bbd_ensemble.rs:84-96: per-sample цикл; строка 90
  `let bbd_out = 1.0 - (-2.0 * PI * 8000.0 / sr).exp();` — комментарий сам
  говорит «SR-independent»: аргумент константен, exp пересчитывается на
  КАЖДУЮ итерацию тапа (3 тапа × сэмпл; значение общее для обоих каналов).
  HOIST = вынести перед циклом.
- ВАЖНО (review C1 — исправлено): НОВЫЕ подтверждённые per-sample
  нарушители (все три tick() зовутся из per-sample цикла mod.rs:2486+, exp
  считается БЕЗУСЛОВНО на каждый вызов, аргумент инвариантен — только
  параметры+sr):
  * exciter.rs:169-170 (pres_cutoff/pres_coeff, внутри tick 138-181);
  * vocoder.rs:179-181 (follow_coeff/att, внутри tick 166-181);
  * bbd_ensemble.rs:59 (bbd_coef, внутри tick "Process mono input").
  exciter.rs:73-75 — update_coeffs с дедуп-гвардом `(freq-last).abs()<1e-5`
  (68): срабатывает на смене параметра, не на сэмпле — НЕ трогать (формулировка
  «per-block» неточна, но безвредна).
- uLaw Bitcrusher (airwindows.rs ~2298-2301): `256f64.powf(sl.abs())` на
  сэмпл — НАРУШИТЕЛЬ по скорости, но порт 1:1: аппроксимация/LUT с
  интерполяцией меняет биты → goldens красные. LUT с ТОЧНЫМИ значениями
  возможна (после crush_and_return значения лежат на сетке increment_b) —
  но это отдельный оптимизационный таск, НЕ в этом плане (см. Out of scope).
- voice.rs:14 прецедент: fast 2^(x/12) уже существует для hot-path (был
  сделан осознанно ранее — для НОВЫХ аппроксимаций нужно юзер-решение).

---

## Test Cases (from the requirement)

- TC1: goldens байт-стабильны после каждого hoist (главный и достаточный
  гейт — hoist меняет ТОЛЬКО точку вычисления, не значение).
- TC2: код-ревью: аргумент вынесенного выражения не зависит от сэмпла
  (только sr/параметры).
- TC3: grep после Task 2: в per-sample циклах effects-файлов нет
  exp/powf с инвариантными аргументами (аудит-таблица в commit message).

## Negative Matrix

- Аргумент зависит от сэмпла (случайные LFO-модуляции частоты внутри цикла)
  — hoist ЗАПРЕЩЁН (изменит звук) — правило проверки в Task 2.
- f64 vs f32: вынесенное выражение обязано сохранить исходную точность и
  порядок операций (review-чек).
- ~~~timeout/clock/конкурентность~~ — n/a (чистая арифметика).

---

### Task 1: bbd_ensemble — hoist bbd_out

**Files:** Modify `src/synth/bbd_ensemble.rs:84-96`.

- [ ] Step 1: перед per-sample циклом (строка ~84, рядом с вычислением
  center_samp/depth_samp):
```rust
            // BBD output LP per tap (~8 kHz), SR-independent — hoist из
            // per-sample цикла (константный аргумент, audit exp/powf #bbd)
            let bbd_out = 1.0 - (-2.0 * std::f32::consts::PI * 8000.0 / sr).exp();
```
  и удалить строку 90 (внутри цикла остаётся использование `bbd_out`).
- [ ] Step 2: gates: `cargo test --test golden_fx` (бит-точность — главный);
  clippy; cargo test.
- [ ] Step 3: коммит `perf: hoist invariant bbd LP coefficient out of per-sample loop (audit exp/powf)`.

### Task 2: систематический аудит-проход + hoists

**Files:** Verify-и-fix: src/synth/{exciter,vocoder,bbd_ensemble,resonator,
bitcrusher,reverb2,spring_reverb,chorus,flanger,phaser,...}.rs + airwindows.rs
порты. ПОДТВЕРЖДЁННЫЕ дополнительные сайты (review #4): reverb2.rs:146
(`fb = (-7.0*delay_time_s/decay_s).exp()` + damp_coeff :129 — внутри per-sample
tick, инвариантны); spring_reverb.rs:148 — в set_params, ок (не трогать);
phaser.rs:66 powf — аргумент зависит от сигнала (lfo) — НЕ трогать
(негатив-матрица).

- [ ] Step 1: построить таблицу: для каждого файла — grep `.exp()|.powf()|`
  → для каждого вхождения определить: (а) внутри per-sample цикла? (б)
  аргумент инвариантен к сэмплу? Таблица (файл:строка, цикл?, инвариант?) —
  в отчёт. Инструмент: grep + чтение контекста ±30 строк.
- [ ] Step 2: каждый подтверждённый инвариантно-цикловой сайт → hoist по
  образцу Task 1; КАЖДЫЙ hoist — отдельный коммит с golden-гейтом.
  ОБЯЗАТЕЛЬНО перед коммитом — TC2 sign-off: ревью аргумента (инвариантность
  к сэмплу) зафиксировать в commit message одной строкой.
  Аппроксимации — ЗАПРЕЩЕНЫ (см. шапку). Сайт с зависящим от сэмпла
  аргументом — оставить, пометить комментарием «per-sample exp: argument is
  signal-dependent (audit)».
- [ ] Step 3: gates после каждого: golden_fx бит-точность; в конце полный
  гейт: clippy / cargo test / rt_safety / deny.
- [ ] Step 4: коммиты `perf: hoist invariant exp/powf out of per-sample loop — <file> (audit exp/powf)` на сайт/группу.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens.
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.
- [ ] Step 3: в PR-описание — таблица аудита (Step 1 Task 2): сколько
  сайтов под hoist, сколько осталось per-sample по необходимости (uLaw).

## Out of scope
- uLaw Bitcrusher точный LUT — реальная оптимизация, требует дизайна сетки
  (юзер-решение по приоритету; Bitcrusher и так в slot_dead_scalings-списке).
- fast_exp-аппроксимации для любых port-путей — sound change, юзер-решение.
- Voice-часть hot path — уже оптимизирована (voice.rs:14 прецедент).
