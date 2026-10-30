# Refactoring audit — mini_midi_synth (2026-10-30)

Полный аудит: 6 параллельных аудиторов (synth-core, synth-fx, synth-instruments,
gui, app-plumbing, architecture + tooling runner), 36 682 LOC, все находки
с якорями file:line. Критические/высокие находки проверены независимо по
исходникам (CONFIRMED). Read-only аудит: ничего не изменено.

> **Сверка 2026-10-30 (вторая итерация):** все 19 пунктов (QW1–10, S0–S8)
> перепроверены независимыми субагентами по исходникам + best practices
> (JUCE prepareToPlay, nih-plug derive(Params), Bencina/Doumler RT-гайды,
> assert_no_alloc). Итог: 12 подтверждены полностью, 6 с поправками, 1
> под-пункт аудита НЕВЕРЕН (QW8/`Config::save` — живой, 17 вызовов).
> Поправки внесены в текст ниже.

## Architecture mental model

1. Entry: `src/lib.rs` — `init_common` wires rings/atoms/engine; `desktop_main`/`run_gui`/`run_headless`/`android_main`+JNI on top.
2. Audio: cpal callback (`src/audio.rs`) drains ControlEvent/MidiEvent rings → `SynthEngine::tick_block` (src/synth/mod.rs) renders 16 parts × voices + FX. RT contract: no alloc/locks/IO; heap → `trash.rs`.
3. Params: stringly `BTreeMap<String,f32>` → `PatchParams` (patch_params.rs) → per-site plumbing in ~8 places; presets = JSON Preset→Part→Split→Performance.
4. Instruments: DSP voices (oscillator.rs), SF2 sampler (rustysynth), drum engine + step seq, looper, MIDI player, arp — all driven from `tick_block`.
5. GUI: egui `App` (gui/mod.rs), mirror state + feedback atoms; #1 churn hotspot.

Churn топ-5 (6 мес): `src/gui/mod.rs` (81), `src/synth/mod.rs` (67), `src/lib.rs`
(39), `src/gui/settings.rs` (34), `src/main.rs` (31).

## Stacks inventory

Rust 2021, eframe/egui 0.31 (два релиза позади 0.33), cpal 0.15 vendored с
AAudio-патчем (0.16 вышла, апгрейд дорог из-за патча), midir 0.11, rtrb 0.3.6,
rustysynth + mi-plaits-dsp vendored.

Tooling run:
- `clippy --all-targets` — **чисто с `-D warnings`** (подтверждено).
- pedantic clippy: 2538 warnings, 87% — cast-шум DSP; топ: airwindows.rs (647),
  synth/mod.rs (312), oscillator.rs (167), gui/params.rs (158).
- `cargo build --features rt-assert` — чисто, ноль warnings.
- `cargo deny` — ok (yanked транзитивный `clipboard-win 5.4.1` через arboard,
  ~25 дублей версий — шум).
- **287 тестов зелёные в 21 suite**.
- `rt_safety` suite feature-gated, в аудите не прогонялся.
- 44× `#[allow(dead_code)]`, 18× `too_many_arguments`, ноль TODO/FIXME.
- Нет секции `[lints]` в Cargo.toml.

## Backlog

### Quick wins (S, делать сейчас)

1. **Wavetable alloc на каждом note-on** — `oscillator.rs:641`
   `Arc::new(Vec::new())` + `oscillator.rs:2545-2572` пересобирает всю таблицу
   8×256 на каждое нажатие (путь: `voice.rs:245, 325-334`; только osc1 и только
   built-in таблицы — кастомные идут через дешёвый `Arc::clone`). Хуже
   заявленного: `take_buf` (:2552) mem::take `pool_a`, а Wavetable-буферы
   исключены из recycle (:470-472) → после первого нажатия каждый note-on =
   8KB heap alloc + Arc alloc + 13 312 `sin()` + dealloc. Кэшировать built-in
   таблицу в `OnceLock<Arc<Vec<f32>>>` с прогревом в `init_common` (не лениво —
   иначе первый билд уйдёт в callback); статическая пустая таблица для reset
   безопасна (таблица за Arc immutable). Сопроводить rt-assert тестом NoteOn с
   Wavetable-патчем — именно пробел в покрытии позволил багу жить. *CONFIRMED
   (сверка: хуже заявленного).*
2. **LFO sine LUT лениво аллоцируется на аудиопотоке** — `lfo.rs:6-18`
   `OnceLock::get_or_init` 8KB (`Box<[f32; 2048]>`) на первом тике. Триггер
   шире заявленного: `apply_deform` (lfo.rs:286) зовёт `sine_lut` при
   `deform >= 0.5` для ЛЮБОЙ волны — прогрев в `init_common` (lib.rs:142,
   до создания аудиострима) должен быть безусловным, не привязанным к
   wave==Sine. `init_common` подходит: `desktop_main`/`run_gui` зовут его
   строго до `AudioBackend::new`. *CONFIRMED.*
3. **Drain drift / латентный баг** — `MidiSeqLoad`/`MidiSeqPlay`-stop
   (`mod.rs:2561-2565, 2592-2596`) чистят только голоса, в отличие от
   `SetPartEnabled`/`SetPartMute` (`mod.rs:2146-2155, 2177-2180`).
   ПЕРЕФОРМУЛИРОВАНО по сверке: воскрешения нот через `note_stack`/`latch_held`
   НЕТ — оба seq-хендлера зовут `set_sustain(0)` (mod.rs:2557, 2587), который
   их чистит. Реальный остаток: `set_sustain(0)` НЕ чистит
   `root_held`/`root_active` → ghost-rekey на root_only-патче после MIDI-stop
   (код сам документирует опасность, mod.rs:2190-2191). Тот же пробел у
   `AllNotesOff` (mod.rs:2431-2435), в отличие от twin CC120/123 (1972-1984).
   Фикс: вынести `Part::drain_notes(&mut self, sampler)` — per-part cleanup-блок
   дублируется уже 4× (2146, 2166, 2475, 1972), хелпер закроет оба пробела
   сразу. S–M. *Сверка: эффект переоценён, пробел `root_held` — новинка.*
4. **`check_pickup` знает только 4 ключа** — `mod.rs:1697-1705`
   (filter_cutoff/resonance, amp_attack/release), остальное → `.unwrap_or(0.0)`:
   ручку надо увезти в минимум, потом параметр скачет с патчевого значения до
   min_val. Бонус-баг из сверки: читает только `parts.first()` (part 0), а
   применяет ко всем частям (mod.rs:2030) — при расхождении частей pickup
   неверен даже для 4 известных ключей. `PatchParams::get(key)` — это новый
   ~230-arm match, т.е. ещё один shotgun-site; делать внутри/после S2 (аудит
   S2 уже заявляет поглощение), не отдельно. S. *CONFIRMED.*
5. **Perf-load расходится по поверхностям** — `gui/layers.rs:279-322` инлайн-
   копия vs `gui/mod.rs:2131 fn load_performance_by_name`: копия в layers
   восстанавливает `root_only` (:301), но не `split_point` (восстанавливается
   только в `gui/mod.rs:2151-2158`); копия в mod.rs — наоборот. Сверка: третей
   копии нет, остальные 4 сайта загрузки идут через `load_performance_by_name`;
   слияние тривиально (Reset-кнопка layers.rs:258 уже зовёт loader). Решения
   при слиянии: (а) семантика `root_only` — `persist_parts_state`
   (mod.rs:2113-2122) считает mute/root_only конфиг-состоянием «NOT part of
   performance files' semantics», а Performance `root_only` хранит; (б) взять
   tail-loop вариант mod.rs (2197-2202) вместо pre-disable-all в layers.
   *CONFIRMED (drift проверен в обе стороны).*
6. **Looper атомы на каждый сэмпл** — `looper.rs:577-585` (`update_atoms`):
   4 atomic stores + f64 деление на сэмпл, зовётся из всех веток `tick`,
   включая Idle (:472), где ничего не меняется. Цифра скорректирована: ~48k
   div/s при 48 kHz (~200k только на 192 kHz). `state`/`event_count`/
   `layer_count` меняются только по control-границам; `position` u8-квантован
   (шаг ~loop/256), GUI читает ≤60 Гц → per-block записи визуально неотличимы.
   Лучший фикс — write-on-change с кэшем последних значений на `MidiLooper`:
   wrap 255→0 обрабатывается сам (позиция считается после wrap-reset,
   :528-542), но кэш надо force-обновлять в restore/clear/stop (:357-358).
   *CONFIRMED.*
7. **Docs/assert rot (пачкой)**: `mod.rs:1125` сообщение ассерта винит
   `GlobalParams::set` (это `Part::set_param`; настоящий `GlobalParams::set`
   молча игнорирует, `mod.rs:425`); `mod.rs:1189` "0-7" vs 16 частей;
   `layers.rs:1-9` документация зон + хардкод четвёрок (`layers.rs:149+151,
   384`) vs
   `PARTS_PER_ZONE=8`; `filter.rs:202` "32 samples" vs каждые 8. S.
8. **Dead code sweep** — `gui/mod.rs:2211-2272` `_save_window_size` (мёртвый,
   underscore-префикс глушит lint; fs::write в /tmp на :2231 — GUI-поток, не RT),
   `gui/mod.rs:1249-1251` пустая ветка, `preset.rs:500-512`
   placeholder-комментарии, `SynthEngine::load_patch`/`Part::load_patch`
   (`mod.rs:1665, 577` — зовут друг друга, внешних вызовов ноль; тесты идут
   через `ControlEvent::load_patch_from`), мёртвые `save_to_params` ×3 —
   `mseg.rs:264`, `mod_matrix.rs:578` и **`step_seq.rs:335` (третий, в аудите
   пропущен)** — 0 вызовов, каждый прикрыт `#[allow(dead_code)]`.
   **ПОПРАВКА по сверке:** пункт «`config.rs:288-299 Config::save` — ноль
   вызовов» НЕВЕРЕН — живой, 17 вызовов (gui/mod.rs ×13, settings.rs ×4);
   удалить только устаревший `#[allow(dead_code)]` на config.rs:288. S.
9. **Запись конфига каждый кадр при драге** — `gui/mod.rs:913`
   (sustain-release слайдер) и `:923` (SF2 rel DragValue) зовут
   `save_global_to_config()` на каждый changed-кадр, вообще мимо debounce;
   `:905` — одноразовый клик, не драг. Фикс тривиален: заменить на
   `self.global_dirty = true` — debounce (mod.rs:573) уже есть, оба поля уже
   сериализуются (`save_global_to_config` :1505-1506). Цена: весь Config в
   pretty-JSON (drums_remaps, keybinds, pad_perf_map, ...) = 2 аллокации +
   write+rename на кадр при 60 fps — заметно на Android FS. Плюс тост при
   ошибке `config.save()`: молчаливых `let _` сайтов **17**, не ~25
   (gui/mod.rs ×13, settings.rs ×4). S.
10. **Вычистить carried-список docs/TECHDEBT.md** — ПОПРАВКА по сверке:
    исправлены ВСЕ 7 пунктов, не 5 (аудит пропустил `seq_program_set` →
    sampler.rs:498-508, с регресс-тестом; `SetPartEnabled` double-cleanup →
    mod.rs:2138-2158, с комментарием). Остатки сверки: mute gate mod.rs:3036-3051,
    CC120 kill mod.rs:1971-1990, arp flush mod.rs:1541-1546, sustain atoms
    mod.rs:2557, rtrb 0.3.6 в lock. По-прежнему открыты (реально аллоцируют в
    callback): `Synthesizer::new` при SF2 (sampler.rs:104-118) и
    `MidiPlayer::load` (midi_player.rs:119-134). Обновить цифры LOC в
    CLAUDE.md: 36 682, не ~32k.

### Strategic (порядок зависимостей)

**S0. Characterization-тесты (предпосылка для всего ниже).**
Golden-signal харнесс для FX + физических моделей (по сверке: 0 тестов в
epiano/bonsai/neuron/combulator/resonator/exciter/reverb/chorus/fx_chain/
spring_reverb/vocoder/nimbus и др.); `assert_no_alloc` вокруг note-on для
каждого семейства осцилляторов и tick_block с активным LFO/FxChain и
`set_sample_rate`. Поправка: покрытие НЕ нулевое — `tick_block` под guard есть
(rt_safety.rs), но только с дефолт-патчем, а note-on стоит ВНЕ guard
(rt_safety.rs:22); нет guarded-кейсов wavetable note-on, LFO, FxChain,
`set_sample_rate` в callback (audio.rs:199-205 под rt_guard). Тайминг looper
`tick/record_event/quantize_tick/undo` — не покрыт (drum-seq undo покрыт, это
другой код); voice-stealing — частично (`steal_prefers_releasing_voice` есть,
oldest-first и Poly-High/Low — нет); perf-load последовательность — нет;
re-send в `reconnect_audio` — нет (audio_hotplug.rs покрывает только
ReconnectPolicy). **CI-пункт из сверки:** под обычным `cargo test` rt_safety
молча компилируется в 0 тестов (feature-gate) — добавить `--features
rt-assert` в обязательный прогон. Suite прогнан в сверке: 5 passed. Готовый
скелет для golden-харнесса: `run_preset_chain` / `all_presets_no_nan_no_silence`;
golden-тесты сначала пинуют детерминизм (фикс. SR, denormals, без wall-clock).
M–L. **Блокирует S1–S5.**

**РЕАЛИЗОВАНО 2026-10-30** — **S1. RT-safe `set_sample_rate` (расширение TECHDEBT #12).**
`audio.rs:199-205` вызывает его в колбэке (динамическая смена SR в JACK,
реагирует на `jack_sr` atomic из error-callback); реаллоцирует 32 эффекта
(`reverb.rs:298-300` — `*self = Self::new(sr)` + vec! delay lines, `chorus.rs:73`,
`spring_reverb.rs:82-85`) и делает танец save/restore на 13 полей (+2 клир) в
`drum.rs:1054-1090` (следующее добавленное поле молча сбросится; вдобавок
`*self = Self::new` в колбэке само аллоцирует 3 `Arc` — drum.rs:1009-1011).
**ПОПРАВКА по механизму (сверка):** engine владеется колбэком by-value
(audio.rs:124), control-thread in-place rebuild без мьютексов невозможен;
«паттерн trash.rs сэмплера» — это in-callback rebuild с отложенным free
(`Sampler::rebuild` зовётся из handle_control, старый heap уходит в GUI через
`Displaced`), а не control-thread. Лучший вариант: allocation-free reset в
колбэке (preallocated-буферы / resize в capacity, без `*self = Self::new`).
Полный rebuild стрима (существующий путь settings.rs:239-305) работает, но
теряет лупер-буферы и живое состояние — не годится для рутинной смены SR.
Направление совпадает с контрактом JUCE prepareToPlay (rebuild на control
потоке при остановленном callback'е — здесь это эквивалентно rebuild'у стрима).
M. После S0.

**S2. Реестр параметров (главная находка shotgun surgery).**
Один параметр = 7–8 рукописных мест, счётчики по сверке: patch_params.rs ×5
списков (266 полей; 310 `p("key", default)` в from_map, 279 `s!` в to_map),
match в `Part::set_param` (mod.rs:1012-1131, 107 arm), `build_voice_params`
(mod.rs:769-867, ~108 копий + magic scalings вида `osc_ws_drive * 4.0 + 0.5` —
таблице нужна колонка transform), `CachedFxParams::from_preset`
(mod.rs:1280-1400, 111 записей с повторно объявленными дефолтами),
`check_pickup` (4 arm), cc_map.rs (38 registry-entries, ключи/range повторно в
default_map), gui/params.rs (106 слайдеров с повторными range); global-набор
ключей объявлен в ≥6 местах, не 5. Декларативная таблица (key →
default/range/getter/setter) — gross ~1270 строк, чистая экономия реалистично
500–900 (не ~1500); убивает целый класс silent-typo багов: `GlobalParams::set`
(mod.rs:429) молча игнорит неизвестные ключи, теста полноты нет. Массивы
(drawbars 1-индексные vs macro 0-индексные) генерить циклом, расхождение
индексации сохранить для совместимости пресетов. L. Поглощает: quick-win #4,
утроение дефолтов, нестыковку scope `delay_mix` И `reverb_mix` (registry =
Patch, но GLOBAL_PARAM_KEYS/дефолтный CC/GUI = Global; learned binding пишет в
part.params, а `from_preset` читает global-first → ручка кажется мёртвой).
Практика: nih-plug `#[derive(Params)]`, JUCE APVTS — SSOT-таблица стандарт для
этого паттерна. После S0 (тест полноты `set_param`).

**S3. Консолидация FX.**
Две параллельные FX-системы (legacy chain mod.rs:3069-3186 + 16-слотовый
`apply_fx_slot` mod.rs:3216-3344) делят синглтон-инстансы ~30 эффектов —
слоты мутируют `.mix`/`.drive`, которые читает legacy-путь (mod.rs:3225-3228,
3256-3272; flanger/phaser/tremolo/bitcrusher тикаются legacy-веткой без
аргументов, т.е. читают испорченные слотами поля); два слота одного типа
тикают один инстанс дважды за сэмпл (двойной LFO-продвиг). **БЛОКЕР миграции
(сверка):** `FxChain::from_legacy_preset_active` (fx_chain.rs:311) НЕ подключён
к load (единственный вызов — GUI-кнопка gui/fx_chain.rs:20) и конвертирует
только 15/30 эффектов; legacy-полей в PatchParams ~121, не ~90 — удаление до
расширения конвертера и wiring'а в load молча деградирует пресеты. Звук при
конверсии изменится: legacy mix применяется внутри `tick(...)`, слоты
hardwire mix=1.0 + внешний crossfade ≠ внутренний mix; `delay.ping_pong`
hardwire false (mod.rs:3275); часть legacy-параметров теряется (neuron
asym/bias, spring spin/chaos и др.). План: дескриптор слота
`{label, range, default, to_dsp}` для magic scalings (mod.rs:3225, 3242,
3299…), вынос `FxRack` из mod.rs (~460 строк). L. После golden-тестов S0.
Сперва решить дизайн-вопрос: движок берёт параметры FX из первой *включённой*
части (mod.rs:2911), а GUI правит per-part цепочки (gui/fx_chain.rs) — правка
неактивной части не слышна, подтверждённый UX-баг; per-part DSP-инстансы на
Orange Pi 5/Android не влезут по CPU — нужен дизайн с шарингом/лимитами.

**S4. Разделение god-файлов.**
`synth/mod.rs` (3357: events 139-369 / Part 454-1129 / MIDI dispatch 1730-2045 /
control 2053-2645 / fx mix 2978-3344, сначала re-export шимы); `gui/mod.rs` —
вынос из App (console lifecycle 642-798, audio watch 496-542, preset panel
1047-1292; порядок регистрации панелей load-bearing — сперва расширить
tests/ui_layout_contract.rs); `lib.rs` (init.rs / headless.rs / android_jni.rs +
конструктор `App::new(common)`, чтобы убить 170-строчный struct literal
lib.rs:712-888); `drum.rs` — вынос StepSequencer. Каждый M, суммарно L.
**Поправки по сверке:** границы секций подтверждены (mod.rs ровно 3357;
literal 712-888 = 177 строк); fx-mix — это срез ВНУТРИ `tick_block`
(2692-3345), не секция — самый рискованный кусок разделения (пересечение
borrow'ов, после — rt-assert прогон); ui_layout_contract.rs — ручное зеркало,
само НЕ детектит расхождение порядка панелей (сначала научить его читать
реальный порядок из gui/mod.rs); 5 интеграционных тестов пинают пути
`synth::drum::*` → `pub use` шимы обязательны; у App нет `Default` —
конструктор обязан быть исчерпывающим; android-`#[cfg]`-статики lib.rs
проверять через `./android/build_rust.sh`, не только desktop cargo check;
вынос `Part` в отдельный файл потребует `pub(super)` на полях (дочерние модули
mod.rs видят приватное, родитель — нет). После S0; после S2/S3, чтобы код не
двигать дважды.

**S5. Предпосылки #32 (looper/transport). — ВЫПОЛНЕНО 2026-10-30 (plan docs/superpowers/plans/2026-10-30-refactor-s5-clock-sync.md).**
Looper BPM задаётся только через `LooperSetBpm` (mod.rs:2362), тогда как
arp/LFO/pitch_seq получают BPM секвенсора каждый блок (mod.rs:2714-2715,
источник — `drum_engine.sequencer.bpm`, писатели только GUI: DrumSeqBpm,
DrumLoadKit) — транспорт-работа разсинхронизирует quantize-сетку; длина бара
лупера хардкодит 4/4 (`looper.rs:211-213`) vs `DrumPattern.beats_per_bar`
(drum.rs:718). Синхронизировать clock + прокинуть beats_per_bar **до**
реализации #32. **Уточнения по сверке:** GUI маскирует проблему флагом
`looper_sync_bpm` (config.rs:142, default on) через 4 дублированных пуша
(drums.rs:281/381/416/898) — сам флаг и есть smell; и даже он ломает quantize
при смене BPM mid-loop: сетка считается от нового bpm (looper.rs:215-229),
позиция loop-а заморожена в сэмплах от старого (:237). MIDI-clock хендлеров
нет вовсе (0xF8/0xFA — ноль вхождений). #32 подтверждён (TECHDEBT:579-597,
phase 1 планирует ОДНО поле BPM — решение о каноническом clock-источнике
нужно до него). Практика: Ableton Link передаёт quantum (beats_per_bar)
вместе с tempo. M. После looper-таймингов из S0.

**S6. Sweep дедупликации в dsp_utils.**
**Счётчики исправлены по сверке:** DC-blocker ×9 руками, не ×4 (voice.rs:12,
phaser.rs:79/flanger.rs:85/filter.rs:529, oscillator.rs ×3, wave_shaper.rs:68,
bonsai.rs:96) — коэффициенты у сайтов разные (0.9997/0.9995/0.995/π-формулы),
слить в `DcBlocker` можно только с сохранением per-site констант + моно-вариант;
xorshift ×7 same-constant 13/17/5 (lfo.rs ×3 в одном файле — лёгкая победа;
oscillator.rs:2504 с 7/5/3 — легитимная chip-noise вариация, НЕ трогать;
patch_params.rs:624 xorshift64 — GUI-side, не трогать); LFO phase ×6-8 — все
bit-identical к существующему `advance_phase`, самый безопасный кластер;
linear-interp ×3 (combulator.rs:53, floaty_delay.rs:51, delay.rs:92-100) —
bit-identical к `buf_read_linear`, тривиально; soft_clip ×2 идентичных
(drum.rs:65, overdrive.rs:8) + 1 мёртвый (oscillator.rs:2786 — удалить, не
дедупить); кубическая интерполяция ×4 — Hermite и cubic-Lagrange математически
совпадают, но НЕ bit-identical после слияния (переупорядочение float-операций
→ ULP-drift, в feedback-петлях spring/neuron/flanger накапливается) — только
с bit-comparison golden-харнессом или не трогать; biquad-процессоры ×3
(TDF2 vs DF1 vs Butterworth — выбор канона отдельное решение, опционально;
коэффициенты RBJ уже шарятся). M. После S0; после любой правки — rt-assert
прогон, `#[inline(always)]` сохранить.

**S7. Треугольник engine handles.**
lib.rs:208-233 ↔ gui/settings.rs:263-276 оба вытаскивают **одинаковый набор из
14** хэндлов через pub поля движка; каждый новый хэндл трогает **6** мест, не 4:
CommonInit field decl + engine pull + CommonInit literal + App literal + App
field decl + reconnect_audio (**поправка:** `CommonInit` — struct, не trait).
`SynthEngineHandles`-struct + `App::new(common)` — перемещаемо, RT-зону не
затрагивает (вызовы только GUI-поток / между стримами); вместо 14 per-field
`#[cfg(feature = "gui")]` — один гейт на struct. `App::new(common)` убивает
literal lib.rs:712-888 и 14 повторных пулов в settings.rs:263-276. M. Вместе
с S4 (разделение lib.rs).

**S8. Баг маршрутизации pitch-bend в сэмплере.**
`sampler.rs:510-517, 525-530`: `part >= 8` молча отбрасывается, канал =
`part % 16` вместо `part_channel_of` (sampler.rs:349-351; melodic-маппинг
пропускает ch9). **Поправка по влиянию (сверка):** единственный вызывающий
(mod.rs:1901, 1910) шлёт только parts 0–1, поэтому `part >= 8` сейчас
недостижим, «половина движка» — преувеличение. Реальный баг: при non-identity
аллокации (part 1 → ch 0, когда part 0 не SF2-enabled) bend/mod уходит на
чужой канал. Второй баг на вызывающем: цикл `0..2` — части 2–15 без bend
вовсе. Фикс: `let Some(ch) = self.part_channel_of(part) else { return }` в
обоих fn + расширить цикл вызывающего до всех частей. Тест через capture-хук
на `MidiKeys::midi` (покрытия пути нет, sampler-тесты явно note: keys.synth
is None). M с тестом. *CONFIRMED (влияние скорректировано).*

### Decline (отклонено, перечислено для видимости)

- `filter.rs` enum-of-states — 39 типов фильтров в одном struct расточительно,
  но работает и покрыт NaN/stability-тестами; L без выгоды.
- Verbity/Galactic FDN dedup (airwindows.rs) — vendor-ported код, «drift» — это
  различие звука алгоритмов; трогать только с golden-тестами, без срочности.
- piano/epiano hammer-burst dedup — звуки легитимно различаются; M ради малой
  выгоды.
- Разделение `airwindows.rs` на каталог — механика, файл стабилен; отложить до
  следующей правки airwindows.
- play_mode/seq_route f32→enum по всей базе — включить в S2, не отдельно.
- Кодификация pedantic clippy — 87% cast-шума; не годится как CI-гейт.
- Апгрейды eframe 0.33 + cpal 0.16 — реальный version debt, но цена
  переприменения vendored-патча; планировать отдельно.
- `looper_layer_mute [bool;256]`, дубль `note_name`, дубль PAD-таблиц, дубль
  `PARTS_PER_ZONE`, `save_patch_in`≡`save_json`, const для socket-path —
  тривиальности, одной cleanup-коммитом при желании.

## Per-module health table

| Module | LOC | Churn rank | Findings (C/H/M/L) | Debt theme |
|---|---|---|---|---|
| synth/core | ~10.5k | #2 (mod.rs) | 0/6/12/10 | god file + 7-site param plumbing + live RT allocs |
| synth/fx | ~10k | low | 0/2/10/8 | две параллельные FX-системы на общих инстансах; копипаста DSP utils |
| synth/instruments | ~5k | rising (#32 incoming) | 0/2/8/10 | нетестированный looper/sequencer clock; риск сброса состояния при смене SR |
| gui | ~8.2k | **#1 (mod.rs 81)**, #4 (settings.rs) | 0/1/8/10 | god App; drift mirror-state (perf load); нетестированный reconnect |
| app plumbing | ~4k | #3 (lib.rs) | 0/1/7/8 | lib.rs god file; молчаливые error paths; headless без тестов |
| cross-module | — | — | 0/3/8/5 | параметрическая система = 4 файла, 0 реестра; hardware-ноты внутри движка |

## Tooling findings folded in

Clippy `-D warnings` чист — **подтверждено**. cargo-deny ok. 287 тестов зелёные.
44 `allow(dead_code)` подтверждают dead-code находки выше. Неиспользуемых
зависимостей не найдено. Yanked `clipboard-win` — реален, но безвреден
(цепочка arboard/egui), действий не требует.

## Refuted by audit

- **rtrb RUSTSEC-2026-0274 как живая проблема** — REFUTED: Cargo.lock на 0.3.6
  (проверено); остался только устаревший текст в TECHDEBT.
- **"Mute not applied in mix", "CC120 not kill", "arp panic flush",
  "SetPartEnabled double-cleanup", "sustain-atom divergence",
  "seq_program_set desync"** (carried-список TECHDEBT) — REFUTED, по каждому
  актуальный якорь (mod.rs:3040, 1971, 1994, 2141, 2557, sampler.rs:371).

## Residual risks / UNVERIFIABLE

- `run_headless` и `reconnect_audio` без тестов — помечены как блокеры S0/S7,
  самостоятельно не прогонялись.
- Android-сторона (JNI-контракт, покрытие `no_denormals` на Android) —
  ревью чтением.
- CPU-оценки (per-sample `apply_tone` sin, convolution reverb ~50M MAC/s,
  piano 24 partials) — арифметика, не профилирование; подтвердить в рамках
  flamegraph-работы TECHDEBT #19.
- То, что пресеты с `fx_chain.active` всегда оставляют legacy-миксы на 0 —
  вывод из дефолтов, не доказано по всем пресетам.
- `rt_safety` suite прогнан в сверке 2026-10-30: **5 passed** (feature-gated —
  под обычным `cargo test` молча даёт 0 тестов, см. CI-пункт в S0); гонять
  до/после любого quick-win, затрагивающего note-on пути.
- Сourcing FX из первой включённой части (дизайн-вопрос S3) может быть
  осознанным поведением — нужно подтверждение владельца.
