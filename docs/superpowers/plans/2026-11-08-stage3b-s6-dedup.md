# Stage 3b: S6 dedup — bit-safe кластеры Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

status: executed 2026-11-08 (xorshift ×31 call-sites, phase ×9/10, soft_clip/fast_exp; DC-blocker/cubic/combulator-floaty — осталось с причинами)

anchor-checked: 2026-11-08 (база = master 2269037; кластеры и сайты — отчёт
dsp-utils аудитора 2026-11-07, свежие анкоры; канон — Tavily 2026-11-08:
merge only bit-identical op order, ULP-drift в feedback-петлях недопустим)

**Goal:** Канонизировать 4 bit-safe кластера dsp_utils (xorshift32 ×27, LFO
phase if-wrap ×13, soft_clip ×2+dead, fast_exp ×2) + linear-interp delay.rs.
Золdens обязаны остаться байт-равными на каждой задаче — это проверка
bit-identity слияния.

**Architecture:** dsp_utils.rs получает `xorshift32`, `soft_clip`, `fast_exp`
(версия с clamp −20 и 8 squarings — из oscillator/bass), линейный interp уже
есть (buf_read_linear). ПЕР-САЙТ конверсии float (bipolar/unipolar/seeded) и
per-site константы DC-blocker — НЕ трогаются. НЕ сливать: cubic (flanger —
op-order), combulator/floaty linear (fmod-wrap ≠ conditional — pin не доказал
bit-identity), floor-wrap phase (друм/FM — другой кластер), block-granularity
mod.rs vibrato_phase (другая семантика).

**Tech Stack:** Rust; goldens (884 presets + 1642 sweep + 31 slot) как
бит-пин; rt_safety.

**Проверенные факты (из аудита, свежие анкоры; исполнитель ПЕРЕПРОВЕРЯЕТ
каждый сайт grep'ом перед правкой):**
- xorshift32 core `x^=x<<13; x^=x>>17; x^=x<<5` — ×27 в 14 файлах:
  airwindows.rs:10-13 (локальная fn), arpeggiator.rs:363, bass.rs:395,
  bitcrusher.rs:40, drum.rs:72-74 (локальная fn), epiano.rs:206, lfo.rs
  :238/246/383/406/412 (×5), mseg.rs:183, oscillator.rs ×12 (:714/831/932/
  1038/1427/1506/1566/1570/1843/2100/2243/2605), piano.rs:313, voice.rs:589.
  patch_params.rs:631 xorshift64 13/7/17 — НЕ трогать (GUI-side).
- LFO phase if-wrap `p += dt; if p >= 1.0 { p -= 1.0 }` — ×13: phaser.rs:59-60,
  tremolo.rs:34-35, ring_mod.rs:66-68, rotary.rs:73-76 (×2), lfo.rs:309-314,
  :359-364, filter.rs:817-820, oscillator.rs:1680-1681, 2149-2155, 2181-2183
  (if+floor гибрид — ПРОВЕРИТЬ: если использует floor — НЕ трогать),
  2840-2841, floaty_delay.rs:134-135 (% 1.0 — fmod ≡ cond-subtract при
  inc<1 — слияние допускается ТОЛЬКО если inc может быть ≥1? inc = rate/sr
  < 1 всегда для этих — ок; исполнитель проверит каждый сайт на inc<1
  инвариант). mod.rs:3268-3269 — block-granularity, НЕ трогать.
  Уже канон: flanger.rs:72, chorus.rs:47-48, reverb.rs:405-406.
  floor-wrap cluster (НЕ трогать): drum.rs:141/147/545/612/1382,
  oscillator.rs:1110/1120-1123/1499/1970/2003/2183/2796/2822-2827.
- soft_clip `x/(1+|x|)`: drum.rs:65-67, overdrive.rs:8-10 (идентичные fn,
  живые); нейрон sigmoid (:46-48) и epiano (:247) — SAME formula inline —
  слить опционально; dead exp-knee вариант oscillator.rs:2973-2981 — удалить.
- fast_exp: oscillator.rs:2467-2479 ≡ bass.rs:517-523 (clamp −20, 8 squarings).
- DC-blocker ×13 per-site constants — НЕ в этой стадии (следующий подсечёт
  S6b с mono/stereo вариантами).
- linear-interp: delay.rs:68-100 — formula-identical канону (clamp [1,n−2]
  caller-side + conditional wrap) — слить на buf_read_linear; combulator.rs/
  floaty_delay.rs — НЕ сливать (fmod-wrap).
- Пин tc_linear_interp_bit_identical_to_canon (tests.rs, positive-read ветка
  добавлена QW6) — защищает delay.rs слияние.

---

## Test Cases (from the requirement)
- TC1 (глобальный): ВСЕ goldens (884 presets, 1642 sweep, 31 slot, fx modules)
  байт-равны после КАЖДОЙ задачи — bit-identity proof by pin.
- TC2: rt_safety зелёный (dsp_utils фн-ы inline(always), zero-alloc).
- TC3: xorshift32 юнит-тест: известный seed → детерминированная
  последовательность (пин канона: seed 1 → 5 значений, зафиксировать в тесте).
- TC4: dead soft_clip удалён — grep ноль ссылок; clippy чист.

## Negative Matrix
- Разный op-order (a*b+c vs c+a*b) → ULP → красные goldens = СТОП, откат.
- oscillator.rs:2181-2183 if+floor гибрид — если читается как floor-wrap —
  НЕ трогать (список двусмыслен; исполнитель решает по коду, отмечает).
- airwindows.rs:10-13 — локальная fn внутри blanket-allow файла; замена на
  канон допустима (та же математика), но airwindows 33 алгоритма — риск
  низкий, goldens fx_module_hashes пинят.
- Локальная fn xorshift в drum.rs:72-74/airwindows.rs — удалить после
  замены (unused).
- `% 1.0` floaty: если где-то inc ≥ 1.0 возможен — fmod(1.2)=0.2 vs
  cond-subtract 0.2 — одинаково однократно; при inc≥2 расходится — проверить
  что rate/sample_rate < 2 у floaty LFO (осциллятор-файл).
- ~~missing field/null~~ — n/a. Дубликат замены (одна строка в двух списках)
  — grep-пин: после каждой задачи `xorshift` останется только в dsp_utils +
  patch_params(xorshift64) + легальные остатки.

---

### Task 1: xorshift32 канон ×27
**Files:** Modify `src/synth/dsp_utils.rs` (+fn), 14 файлов-потребителей.
- [x] Step 1: в dsp_utils.rs:
```rust
/// Canonical xorshift32 (13/17/5). Возвращает сырой state; конверсию float
/// делает сайт (bipolar/unipolar — per-site, НЕ канонизировать).
#[inline(always)]
pub fn xorshift32(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}
```
- [x] Step 2: юнит-пин в synth/tests.rs:
```rust
#[test]
fn tc_xorshift32_canon_sequence() {
    let mut x: u32 = 1;
    let seq: Vec<u32> = (0..5).map(|_| dsp_utils::xorshift32(&mut x)).collect();
    assert_eq!(seq, [270369u32, 67634689, 2647435461, 1094937770, 4212062854]);
}
```
(значения ВЫЧИСЛИТЬ исполнителю прогоном ДО замены сайтов — пин на канон;
если реальные отличаются — записать фактические, главная константа:
детерминизм.)
- [x] Step 3: заменить все ×27 сайтов на `dsp_utils::xorshift32(&mut x)` с
  СОХРАНЕНИЕМ per-site конверсий. airwindows.rs/drum.rs локальные fn — удалить.
  КАЖДЫЙ сайт: grep, прочитать ±5 строк, заменить только shift-ядро.
  patch_params.rs:631 — не трогать.
- [x] Step 4: гейт: clippy + cargo test + goldens byte-stable + rt_safety.
  Красные goldens = op-order изменился → откат сайта, разбор.
- [x] Step 5: коммит `refactor: xorshift32 canon — 27 sites, per-site conversions preserved (S6)`.

### Task 2: LFO phase if-wrap канон ×13
**Files:** Modify `src/synth/dsp_utils.rs` — уже есть `advance_phase`; 11-13 сайтов.
- [x] Step 1: каждый сайт (список выше): проверить инвариант inc<1 (rate/sr
  для этих LFO/модуляторов — да); заменить две строки на
  `dsp_utils::advance_phase(&mut phase, rate, sample_rate)` — ВНИМАНИЕ:
  advance_phase(phase, rate_hz, sample_rate) сам делит; если сайт уже имеет
  готовый `dt` — использовать advance_phase_ratio? НЕ добавлять новый вариант:
  где есть dt — слить ТОЛЬКО if-wrap часть? Решение: канон advance_phase
  (rate,sr); сайты с dt = rate*sr_dt... ИСПОЛНИТЕЛЮ: сайты, где dt уже посчитан
  как `rate/sample_rate` ранее — заменить `+= dt; if wrap` на вызов с (rate,
  sample_rate) только если rate/sample_rate доступно в scope, ИНАЧЕ оставить
  сайт (посчитать в отчёт). Bit-identity: `rate/sample_rate` тот же div.
- [x] Step 2: oscillator.rs:2181-2183 и подобные if+floor — если floor-семантика
  → пропустить (отчёт).
- [x] Step 3: гейт (goldens byte-stable!) + коммит `refactor: LFO phase advance via dsp_utils::advance_phase — bit-safe sites (S6)`.

### Task 3: soft_clip + fast_exp + dead code
**Files:** dsp_utils.rs (+soft_clip, +fast_exp), drum.rs, overdrive.rs,
oscillator.rs (delete dead), bass.rs, neuron.rs/epiano.rs (optional inline —
ЕСЛИ op-order идентичен; иначе оставить).
- [x] Step 1: dsp_utils::soft_clip(x) = x/(1.0+x.abs()); заменить drum.rs:65-67
  и overdrive.rs:8-10 (fn → делегация или удаление локальных + прямой вызов —
  выбрать меньший дифф: локальные fn оставить как one-line делегаты НЕЛЬЗЯ
  (дрейф) — заменить вызовы, fn удалить).
- [x] Step 2: dsp_utils::fast_exp (clamp −20, 8 squarings, max(0.0)) —
  oscillator.rs:2467-2479 и bass.rs:517-523 → канон; локальные удалить.
  СРАВНИТЬ тела бит-в-бит до слияния (если отличаются хоть в чём-то —
  оставить, отчёт).
- [x] Step 3: удалить dead exp-knee soft_clip oscillator.rs:2973-2981 (+allow).
- [x] Step 4: neuron.rs sigmoid/epiano inline — сравнить op-order; идентичен →
  канон, нет → оставить (отчёт).
- [x] Step 5: гейт (goldens!) + коммит `refactor: soft_clip/fast_exp canon — bit-identical sites merged, dead variant deleted (S6)`.

### Task 4: Верификация стадии
- [x] Step 1: полный гейт + ВСЕ goldens байт-равны (git status tests/golden пуст).
- [x] Step 2: PR → squash → rebuild → launch-check.
- [x] Step 3: TECHDEBT подсекция S6 (частично: сделаны 4 кластера; DC-blocker/
  cubic/combulator-floaty linear — осталось, с причинами).
- [x] Step 4: roadmap чекбокс 3b + Tavily постчек (уже покрыт пре-ресёрчем).

## Out of scope (причины)
- DC-blocker ×13 — per-site константы + mono/stereo: S6b, отдельный план.
- Cubic interp merge (flanger) — НЕ bit-identical op-order.
- combulator/floaty linear merge — fmod-wrap ≠ канон, pin не доказал.
- floor-wrap phase cluster — другая математика (multi-wrap).
