# Refactoring Audit — mini_midi_synth (2026-11-07)

**База:** master ed992a0 (после PR #90). **Метод:** 7 параллельных аудиторов (synth-core, synth-fx, gui, entry-points, dsp-utils, architecture, tooling) → агрегация → анкор-верификация всех Critical/High аудитом-лидом → Tavily-проверка каждой рекомендации против лучших практик.

Предыдущий аудит: REFACTOR_AUDIT.md (2026-10-30) — статусы его S/QW-предметов внизу.

## Architecture mental model

1. GUI-поток владеет `App` (168 pub-полей), движку пишет только через `ControlEvent` (rtrb, 512 слотов, zero-alloc RT-контракт, `#[must_use]` heap-return).
2. Обратно: `ParamFeedback` + ~18 атомиков, `LooperDisplay` Mutex (известный RT-долг).
3. `SynthEngine::tick_block` (713 строк) — диспетчер всего: looper-capture state machine → per-part mod precompute (borrow-хак) → voice ticks → две параллельные FX-системы (legacy безусловная + 16 слотов поверх) → master tone.
4. Параметры: `PatchParams` (267 полей × 5 рукописных списков), FX берутся из первой включённой части, GUI правит активную.
5. Персист: 4 вложенных враппера, ~55 точек записи, SSOT-границы нет (перф — исключение, там чисто).

## Stacks

Rust stable 1.96 / edition 2021, egui 0.31.1, cpal (vendored, патченный), rustysynth. Tooling: clippy 0 warnings, cargo deny PASS (yanked `clipboard-win` транзитивно, ~34 дублей крейтов — платформенные, не actionable), 0 TODO/FIXME, 40 `#[allow(dead_code)]` (2 с комментариями).

## Критические/высокие находки (все анкор-верифицированы)

### Critical
- **Dual-chain FX summation** (synth/mod.rs:3819-3951): активная 16-слотовая цепочка НЕ байпасит legacy-цепочку — эффекты дублируются уже сегодня; GUI «From preset» пишет слоты, оставляя legacy `_mix` ключи → импортированный эффект играет дважды.
- **Double-tick 5 слотовых типов** (mod.rs:3982+3822; 4010/4014/4018/4025+3829-3832): Overdrive/Phaser/Flanger/Tremolo/Bitcrusher тикают один синглтон дважды на сэмпл при mix=1.0 (2× LFO, двойная сатурация). Аудит-фикс 2026-11-01 починил только stale-mix симптом.

### High
- **CC scope flip** (cc_map.rs:50,55 vs :147,155; from_config :173): reverb_mix/delay_mix в PARAM_REGISTRY = Patch, в default_map = Global → после первого save/load cc_map фейдеры умирают (check_pickup :2186-2199 не знает ключей → pickup на дне диапазона). Три источника правды расходятся.
- **LFO1 мёртв в mod-matrix** (synth/mod.rs:3455-3459, update_routing_cache :681-694): кэш lfo2/3/4_routed есть, lfo1_routed нет — патч с маршрутизацией Lfo1 и нулевыми direct depths не тикает LFO.
- **cached_* write-only поля Part** (mod.rs:620-622, 657-675, 3592-3595): ноль чтений, записи 750×/с.
- **GM-pick затирание layer_b** (gui/mod.rs:1996-1997): `else` ловит части 1-15 → выбор GM на парте ≥2 перезаписывает seed части 1 + save.
- **Non-atomic multi-event gestures** (gui/mod.rs:2510-2531): свап парта = до 32 событий в кольцо (`let _ =`), разрыв кольца = half-swap без детекции; apply_performance ~100+ событий.
- **Headless init divergence** (lib.rs:439-517 vs gui/mod.rs:2021-2111): headless omits parts_state restore, метроном, ремапы, last_performance — с тем же конфигом звучит иначе.
- **gui-less сборка сломана** (lib.rs:365): `drum_undo_atom` без `#[cfg(feature = "gui")]` при gui-gated поле и локале.

### Medium (ключевые)
- 5 FX-семейств с несовпадающими масштабами legacy↔слот (Combulator offsets 5/-7 vs 0..1, MsTool, Conditioner Hz vs 0..1, Treemonster, Exciter) — расширение конвертера даст ерунду.
- Slot Compressor — no-op knob (fx_chain.rs:159, mod.rs:4083-4085); GraphicEq 11 полос → 3 псевдо (8 unrecoverable).
- check_pickup покрывает 4 из 32 CC-ключей.
- Слайдеры: 150 мест, 0 канонических range'ов; живой дрейф ks_brightness (0.05-1.0 vs 0.0-1.0) и ks_feedback — learnable range зависит от последней отрисованной панели.
- 9 raw `egui::Slider` в params.rs бypass'ят MIDI-learn/CC/double-click-reset.
- Wheel-step идиом ×12 с дрейфом (Volume wheel не пишет undo, Pan/Tr — пишут).
- persist: 55 точек записи, каждый чекбокс парта = полный JSON serialize на диск.
- Vec::push на RT-потоке в нотных путях (note_stack :759, latch_held :732, sf2_mono_stack :1829) — amortized, консистентнее со стек-дисциплиной root_on.
- CachedFxParams: 109 рукописных дефолтов (дрифт = 0 сейчас, но класс бага).
- Pedal-up release блок verbatim ×2 (:1740-1780, :2670-2708); latch-mismatch ×2 (:2394-2401, :2536-2543); panic-drains near-dup ×2.

## Backlog

### Quick wins (выполнены по плану docs/superpowers/plans/2026-11-07-refactor-qw-batch.md)
QW1 CC scope SSOT (bug) · QW2 GM-pick layer_b (bug) · QW3 cached_* delete · QW4 cfg-дрейф · QW5 CachedFxParams defaults · QW6 linear-interp pin gap · QW7 13 dead pub API · QW8 честные debug_assert · QW9 панельный пин hardening · QW10 extract pedal/latch дублей · QW11 LFO1 matrix (bug).

### Strategic (порядок: зависимости)
| # | Эпик | Усилие | Практика (Tavily 2026-11-07) |
|---|---|---|---|
| ST1 | S7-lite: EngineHandles struct (17-18 хэндлов × 6 мест → 3) | M | DI-struct паттерн |
| ST2 | F2-транзакции: SetPartFull boxed event; perf-apply транзакция | M / M-L | command pattern с батчингом — канон DAW |
| ST3 | S2: SSOT-реестр параметров (set_param 107 arm / 1 call site; build_voice_params 111 копий; CachedFxParams 111; слайдеры 150 мест) | L | JUCE APVTS / nih-plug derive Params — канон |
| ST4 | S3: байпас legacy при активной цепочке; нормализованные контракты 5 семейств; конвертер 15/31; ПРЕ-РЕК: slot goldens 31/31 (сейчас 5/31) | L | insert-семантика: один путь на эффект |
| ST5 | S4: lib.rs → init/headless/android_jni + App::new (литерал 207 строк); gui/mod.rs → 5 кусков (console/LED 786-1038, browser 1593-2018, SF2 2113-2785, tracks 2982-3264, performance 3274-3403); synth/mod.rs → delete cached_* → Part::update_mods (растворяет borrow-хак :3416/:3607) → handle_control по подсистемам; drum.rs → StepSequencer | L | tests first, small steps, рефактор и behavior — отдельные PR |
| ST6 | S6: bit-safe дедуп (xorshift ×27, phase ×13, delay.rs linear, soft_clip, fast_exp ×2, DC ×13 по coeff-группам); НЕ сливать cubic (flanger) и combulator/floaty linear до QW6 | M | bit-exactness в feedback-петлях |
| ST7 | F3-персист: Persistence::flush() + dirty + debounce (55 точек) | M | periodic batch save — middle ground |
| ST8 | E3-headless: consolidate с send_initial_presets + characterization | M | — |

### Decline
airwindows split · дубли крейтов/clipboard-win · 150ms JACK sleep · led_companion_cmd путь · LFO3/4/slfo вне sweep (харнесс) · два идиома окон · Vec::push RT (при попутном touch) · переименования underscore-полей.

## Per-module health

| Модуль | LOC | Churn (8 нед) | C/H/M/L | Тема долга |
|---|---|---|---|---|
| synth/mod.rs | 4111 | 89 | 2/3/5/4 | god-file + два FX-пути + дефолт-троение |
| gui/mod.rs | 3567 | 127 | 1/2/5/4 | god-file + 168-полевой App + slider-дубли |
| lib.rs | 1141 | 60 | 0/2/2/2 | 5-в-1 + хэндл-fanout + cfg-дрейф |
| oscillator.rs | 2981 | — | 0/0/2/3 | дубли примитивов ×12 движков |
| fx_chain + FX leg | ~700 | — | 2/1/2/2 | dual-chain + 15/31 конвертер |
| gui/params.rs | 1677 | 5 | 0/1/1/0 | range-at-call-site, 2 пути слайдеров |
| dsp_utils + кластеры | — | — | 0/0/3/4 | dedup ×60+ сайтов |

## Refuted / PARTIAL (старые claim'ы нового аудита)

- «oscillator.rs:2504 xorshift 7/5/3» — REFUTED (строка = `reed_x: 0.0`, варианта нет).
- «DC-blocker ×9» — PARTIAL: ×13. «xorshift ×7» — PARTIAL: ×27. «phase ×6-8» — PARTIAL: ×13 (плюс отдельный floor-wrap кластер — НЕ сливать).
- «FX полей ~90» — PARTIAL: ~119 + fx_chain. «30 семейств» — PARTIAL: 31 слотовый тип.
- App-литерал 177 строк — PARTIAL: 207. Хэндлы 14 — PARTIAL: 17-18.
- «GlobalParams::set молчит только в release» — REFUTED: молчит во всех сборках; паника с врётным сообщением — в Part::set_param (mod.rs:1284).
- combulator/floaty linear «bit-identical к канону» — PARTIAL: wrap через fmod ≠ conditional-subtract, ≤1 ulp в rare-случаях; пин-тест не ловил (write_pos=0 only) — закрыто QW6.

## Residual risks / UNVERIFIABLE

- Android-сплит — нужен прогон ./android/build_rust.sh после S4-lib.
- LFO1-matrix bug — логика верифицирована, runtime не воспроизведён.
- ULP-drift sustained-нот — 0.25s goldens не видят накопление.
- rt_safety не гоняет LoadKeysSoundFont под assert_no_alloc.
- GUIless-билд после QW4 — проверить повторно `cargo check --lib --no-default-features`.

## Статусы старого аудита (REFACTOR_AUDIT.md 2026-10-30)

- S0/S1/S5 — ГОТОВО (было), S8 (pitch-bend routing) — ГОТОВО (закрыто фиксами #55, part_channel_of везде).
- QW1/QW2/QW3/QW6/QW8/QW9/QW10 — ГОТОВО (было); QW4/QW5/QW7 — НЕ НАЧАТО (старая нумерация).
- S2/S3/S4/S6/S7 — подтверждены этим аудитом с исправленными счётчиками; порядок: QW-батч → ST1 → ST2 → S2 → S3 → S4 (+S6 параллельно).
