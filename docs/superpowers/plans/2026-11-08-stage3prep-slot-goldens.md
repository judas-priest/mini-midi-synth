# Stage 3-prep: slot goldens 31/31 (S3 prerequisite) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

anchor-checked: 2026-11-08 (база = master 37dcec0)

**Goal:** Golden-пин рендера для ВСЕХ 31 слотовых типов FxChain (сейчас 5/31) —
пререквизит S3 (аудит: без него байпас legacy/двойной тик не проверяем per-family).

**Architecture:** tests/golden_fx.rs `CHAIN_SLOTS` (:284-287) расширить с 5 до
31 записей (все варианты FxSlotType 1..=31, fx_chain.rs:12-49; None=0 НЕ
пинится — пустой слот). Генерация: `MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test
golden_fx golden_engine_fx_chain_slots`; существующие 5 хэшей Delay/Chorus/
SpringReverb/Nimbus/Vocoder ОБЯЗАНЫ остаться байт-равными (behavior-preserving
— расширение списка не меняет рендер существующих).

**Проверенные факты:** FxSlotType 1..=31: Overdrive, Tape, Neuron, Bonsai,
WaveShaper, Airwindows, Chorus, BbdEnsemble, Flanger, Phaser, Tremolo, Rotary,
Delay, FloatyDelay, Reverb, Reverb2, SpringReverb, ConvReverb, Nimbus, RingMod,
FreqShift, Bitcrusher, Resonator, Combulator, Treemonster, Vocoder, GraphicEq,
Compressor, MsTool, Conditioner, Exciter.

## Test Cases
- TC1: после регенерации fx_chain_engine_hashes.txt содержит 31 запись
  fx_chain_<Name> (без fx_chain_None).
- TC2: 5 старых записей (Delay, Chorus, SpringReverb, Nimbus, Vocoder) — хэши
  НЕ изменились (diff старого/нового файла).
- TC3: полный гейт зелёный (без MINI_MIDI_GOLDEN_UPDATE повторный прогон
  стабилен).

## Negative Matrix
- Слот, который молчит при дефолтных params (например Compressor no-op knob —
  аудит S3): hash может совпасть с baseline — это ЛЕГАЛЬНО (пин фиксирует
  текущее поведение; расхождение отловит БУДУЩЕЕ изменение).
- Non-finite output → render_preset_hash паникует на assert — код-вопрос
  найдётся на генерации.

### Task 1: расширение + регенерация
**Files:** Modify `tests/golden_fx.rs:284-287`, regenerate `tests/golden/fx_chain_engine_hashes.txt`.
- [ ] Step 1: CHAIN_SLOTS → 31 запись (тип как f32 литерал + имя = варианту).
- [ ] Step 2: `MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx golden_engine_fx_chain_slots 2>&1 | tail -3`
- [ ] Step 3: TC2 — `git diff tests/golden/fx_chain_engine_hashes.txt` — строки
  5 старых типов идентичны (только +26 новых). Красный diff старых → STOP/BLOCKED.
- [ ] Step 4: повторный прогон БЕЗ env → 6 passed, стабильно дважды.
- [ ] Step 5: полный гейт (clippy/test/rt_safety).
- [ ] Step 6: коммит `test: slot goldens 31/31 — S3 prerequisite (behavior-preserving, 5 legacy hashes unchanged)`; PR → squash → merge.

## Out of scope
- Сам S3 behavior change (байпас legacy) — ДНЁМ с юзером (diff-ревью звука).
