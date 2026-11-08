# Scene LFO Envelope: автозапуск AD-цикла из idle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (база = master eb3cb0b; якоря сняты с
пост-QW3-состояния; суть дефекта подтверждена quality-ревью QW3 #106)

**Goal:** Scene LFO (slfo1/slfo2) с waveform=Envelope звучит как циклический
AD-контур с момента старта, а не константа −1.0 навсегда.

**Architecture:** stage-машина Envelope (0=idle,1=attack,2=decay,3=перезапуск
атаки) недостижима из 0 для scene LFO: `env_retrigger()` зовётся только на
keyed note-on (part.rs, 3 сайта trigger_mode). Фикс — stateless автостарт:
в scene-секции update_mods при wf==Envelope и env_stage==0 дёрнуть
env_retrigger(); дальше stage 3 циклит AD сам, stage 0 больше не наступает →
автостарт срабатывает ровно один раз за включение волны. Keyed LFO 1-4 не
трогаем: их Envelope по-прежнему ждёт ноту (one-shot семантика).

**Tech Stack:** Rust; юнит-тесты в lfo.rs; goldens: slfo-waveform-5 пресетов
нет (проверено QW3-ревью, 0 файлов) — ожидается байт-стабильность.

**Проверенные факты (HEAD eb3cb0b; re-checked на d351ce9 — 1 коммит между
ними, part.rs/lfo.rs/tests.rs не трогает):**
- lfo.rs: env_stage init 0; пишет 1 только `env_retrigger` (254-259); stage 3
  → рестарт атаки (AD-цикл, QW3 Task 3); stage 0 arm — `env_value = 0.0`
  (константа −1.0 bipolar / 0.0 unipolar); stage 4 никем не пишется.
- part.rs:1002-1027: scene-секция update_mods (slfo1-блок 1003-1014, slfo2
  1016-1027) — slfo1_wf/slfo2_wf читаются из params КАЖДЫЙ блок; comment
  «always tick regardless of note state, never retrigger» на 1002 (после
  фикса устаревает); scene_lfos: [Lfo; 2] (part.rs:119).
- Harness для part-level теста СУЩЕСТВУЕТ: Part::new pub(super)
  (part.rs:201), EngineModInputs fields pub (184-198), src/synth/tests.rs
  уже лезет в Part internals (sounding(&Part) на 599).
- Аудит-факт (QW3 quality-ревью + re-ревью): 0 файлов используют
  slfo*_waveform: 5 (ключ отсутствует целиком) в ~/.local/share пресетах,
  repo presets/, FACTORY_PRESETS, tests/golden — golden байт-стабилен.
- Аудит-факт (QW3 quality-ревью): 0 пресетов используют slfo*_waveform: 5 —
  баг спит, но это тот же класс «GUI-вранья», что и починенные.

---

## Test Cases (from the requirement)

- TC1: свежий Lfo — env_idle() == true.
- TC2: после env_retrigger() — env_idle() == false и остаётся false после
  5000 тиков (AD-цикл не возвращает stage 0).
- TC3 (part-level, ОБЯЗАТЕЛЬНЫЙ — harness существует, см. факты): Part::new,
  `part.params.slfo1_waveform = 5.0`, один update_mods-блок →
  `!part.scene_lfos[0].env_idle()`; негатив: wf≠5 остаётся idle. (Полный
  цикл >0.9/<−0.5 на part-уровне не нужен — покрывает TC2.)

## Negative Matrix

- waveform ≠ 5 — автостарт не срабатывает (гвард == Envelope).
- keyed LFO 1-4 — сайт не трогается: Envelope ждёт ноту (по дизайну one-shot).
- повторный вход: после рестарта env_stage ∈ {1,2,3} — env_idle() false,
  гвард не дёрнет второй раз.
- slfo2 — зеркальный сайт, тот же код.
- ~~~timeout/clock~~ — n/a; ~~~конкурентность~~ — n/a (однопоточный
  update_mods).

---

### Task 1: lfo.rs — env_idle() + тесты

**Files:** Modify `src/synth/lfo.rs` (метод рядом с env_retrigger ~256; mod tests).

- [ ] Step 1: failing-тест в mod tests:
```rust
    #[test]
    fn env_idle_true_until_retrigger_and_never_returns() {
        let mut lfo = Lfo::new(48000.0);
        assert!(lfo.env_idle());
        lfo.env_retrigger();
        assert!(!lfo.env_idle());
        let rate = 5.0 * 128.0;
        for _ in 0..5000 {
            lfo.tick_with_deform(rate, LfoWaveform::Envelope, 0.0);
        }
        assert!(!lfo.env_idle(), "AD loop returned to idle");
    }
```
- [ ] Step 2: `cargo test env_idle` → FAIL (метода нет — compile; добавить
  заглушку `pub fn env_idle(&self) -> bool { true }` для красного теста).
- [ ] Step 3: реальное тело:
```rust
    /// Stage 0 (idle) — Envelope ждёт запуска (keyed: нота; scene: автостарт
    /// снаружи). AD-цикл (QW3) никогда не возвращает stage 0.
    pub fn env_idle(&self) -> bool { self.env_stage == 0 }
```
- [ ] Step 4: PASS; `cargo test --lib lfo`; golden_fx стабилен.
- [ ] Step 5: коммит `feat: Lfo::env_idle — expose envelope idle state (scene-LFO autostart prereq)`.

### Task 2: part.rs — автостарт scene-Envelope

**Files:** Modify `src/synth/part.rs:1002-1027` (scene-секция update_mods), `src/synth/tests.rs` (part-level тест).

- [ ] Step 1: slfo1 и slfo2 симметрично — ВСТАВИТЬ только if-блок ПОСЛЕ
  существующей привязки `let slfo1_wf = ...` (не дублировать let):
```rust
        // Envelope на scene LFO: нет note-on ретрига — запускаем AD-цикл
        // из idle один раз (дальше stage 3 циклит сам; audit QW3-review).
        // Смена волны на другую и обратно НЕ перезапускает — цикл
        // продолжается с текущей фазы (mid-cycle continuation, by design).
        if slfo1_wf == LfoWaveform::Envelope && self.scene_lfos[0].env_idle() {
            self.scene_lfos[0].env_retrigger();
        }
```
  и зеркально для slfo2_wf / scene_lfos[1]. Устаревший комментарий 1002
  («always tick regardless of note state, never retrigger») заменить на:
  «Scene LFOs — always tick regardless of note state; Envelope
  автозапускается из idle (см. if ниже), иначе не ретригается».
- [ ] Step 2: part-level тест в src/synth/tests.rs (рядом с существующими
  Part-тестами; сигнатуру Part::new и способ подстановки params списать с
  соседних тестов файла):
```rust
    #[test]
    fn scene_lfo_envelope_autostarts_from_idle() {
        let mut part = Part::new(/* как в соседних тестах */);
        part.params.slfo1_waveform = 5.0; // Envelope
        let inputs = EngineModInputs { /* дефолтный набор, как в соседних тестах */ };
        part.update_mods(&inputs, /* индекс слоя, напр. 0 */);
        assert!(!part.scene_lfos[0].env_idle(), "autostart failed");
        // негатив: волна ≠ 5 не стартует
        let mut part2 = Part::new(/* как выше */);
        let inputs2 = EngineModInputs { /* те же */ };
        part2.update_mods(&inputs2, /* те же */);
        assert!(part2.scene_lfos[0].env_idle(), "non-envelope started");
    }
```
  ВАЖНО: в tests.rs НЕТ соседа с update_mods/EngineModInputs — образец
  сборки inputs брать из ЕДИНСТВЕННОГО production-сайта mod.rs:2303-2313
  (все поля pub; block_len/sample_rate лежат В EngineModInputs, поля
  193-194). Сигнатура: `update_mods(&mut self, inputs: &EngineModInputs,
  _li: usize)` — второй аргумент это ИНДЕКС СЛОЯ (напр. 0), НЕ bl/sr.
  Part::new(48_000.0) — как в соседних тестах tests.rs:1043/1068.
  (Конкретные аргументы — по фактическим сигнатурам; семантика теста
  фиксирована: wf=5 → после одного блока не-idle, wf≠5 → idle.)
- [ ] Step 3: gates: `cargo clippy --all-targets -- -D warnings`; `cargo test`;
  `cargo test --test golden_fx` (0 slfo-5 пресетов → стабилен);
  `cargo test --features rt-assert --test rt_safety`.
- [ ] Step 4: коммит `fix: scene LFO Envelope autostarts AD loop from idle — was constant -1.0 (audit QW3-review)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт (clippy/test/rt_safety/deny/golden `git status tests/golden`).
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.

## Out of scope
- Keyed LFO 1-4 Envelope «ждёт ноту» — by design (one-shot), не баг.
- GUI-индикация «Envelope циклит» на scene — не запрошено.
- Mid-cycle continuation: смена волны на другую и обратно к Envelope НЕ
  перезапускает цикл (фаза продолжается) — принятая семантика; также LoadPatch
  между двумя wf=5 патчами продолжает фазу (scene_lfos не сбрасываются).
