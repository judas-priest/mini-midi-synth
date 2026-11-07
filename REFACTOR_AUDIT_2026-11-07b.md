# Refactoring re-audit — mini_midi_synth (2026-11-07, вечер)

**База:** master 5fde7d1, после S4-lib (#96), S4-synth (#98), S4-gui (#99), S6b (#101), F3 (#102) — 36 коммитов с момента первого аудита.
**Метод:** 7 параллельных аудиторов (synth-core, synth-fx, instruments, gui, plumbing, architecture, tooling) → агрегация → лид-верификация всех Critical/High по исходникам (прямые чтения якорей).

Предыдущие аудиты: REFACTOR_AUDIT_2026-11-07.md (утро, до S4/F3), REFACTOR_AUDIT.md (2026-10-30).

## Architecture mental model (обновлённая)

1. `init.rs` (`EngineHandles`/`CommonInit`) → три точки входа (`lib.rs` 219 строк, `headless.rs`, `android_jni.rs`) → один движок через кольцо `ControlEvent` (512 слотов, RT-контракт, heap → trash).
2. `SynthEngine` = `handle_event` (MIDI) + 6 подсистем `hc_*` (S4-сплит, чисто) + `tick_block` (~540 строк: looper-capture машина 130 строк внутри, dual FX-цепочка) — хэндлеры вынесены, tick_block остался монолитом.
3. `Part::update_mods` (control-rate precompute) заменил borrow-хак; `EngineModInputs` Copy-struct — но протекает: `lfo_rate` двигателя берётся из первой включённой части.
4. FX: legacy-цепочка безусловна (mod.rs:2509-2625) + 16-слотовая поверх (2628-2640) — двойное суммирование и double-tick живы; рецепт фикса в TECHDEBT.md:766-774.
5. GUI: `App` ~180 полей (методы разнесены по файлам, состояние — нет); персист через единственный `flush_config` (F3 — граница держится, но без тестов и с дырами).

## Stacks

Rust 2021, egui 0.31, cpal vendored, rustysynth.

Tooling run:
- `cargo clippy --all-targets -- -D warnings` — PASS, 0 warnings.
- `cargo check --lib --no-default-features` — PASS, 1 warning: dead поля `console_rx`/`flash_rx` (init.rs:100-101).
- `cargo test` — 210 lib + интеграционные, зелёные. `rt-assert` rt_safety — 16/16 (8 ignored).
- `cargo deny check` — PASS (yanked `clipboard-win 5.4.1` транзитивно, без действий).
- 0 TODO/FIXME; 28 `#[allow(dead_code)]` (без свежих, blame — апрель/октябрь).
- Pedantic clippy: 2929 warnings, ~85% — cast-шум DSP (airwindows.rs 646, oscillator.rs 191, params.rs 158).
- fmt-дрейф ~30.8k строк — информационный, политика не форматировать.
- 227 сайтов `exp/ln/powi/sqrt/powf` в src/synth/ — размер S6-бэклога.

## Backlog

### Quick wins (S, делать)

| # | Что | Якорь | Верификация |
|---|---|---|---|
| QW1 | **flush_config глотает ошибку записи и снимает dirty** — сбой диска (EACCES, read-only, диск полон) = изменение потеряно навсегда, без лога. Фикс: `log::warn!`, dirty остаётся true | gui/mod.rs:1893-1895 | CONFIRMED |
| QW2 | **LFO1 у всех частей тикает на rate первой включённой части** (LFO2-4 — свои `self.params`). Фикс: `self.params.lfo_rate` в part.rs:929, удалить `EngineModInputs.lfo_rate` + док | mod.rs:2119-2122, part.rs:929, 192-194 | CONFIRMED (код) |
| QW3 | **Tempo-sync голосовых LFO1-4 сломан**: в tick уходит `rate*bl`, при sync аргумент трактуется как индекс (`as u8`) → не тот темп (индекс 4 = 1/4 → 128 → default-ветка). Сценевые LFO делают правильно (raw/`*bl` условие) — перенести | part.rs:929/939/945/949, lfo.rs:249-256, сценарий part.rs:978-997 | CONFIRMED (код, runtime не воспроизведён) |
| QW4 | **Slot WaveShaper Drive мёртв выше 12.5%**: слот пишет `p0*4+0.5` (0.5..4.5), движок клампит 0..1 → 87% диапазона = max 32x | mod.rs:2685, wave_shaper.rs:206 | CONFIRMED |
| QW5 | **Mod-matrix: 24 из 30 destination вычисляются и никогда не читаются** — `update_mods` берёт только pitch/amplitude/filter_cutoff (part.rs:1047-1060); GUI предлагает все 30 → 27 маршрутизаций — тихие no-op. **Решение юзера: подключить (дорого) или спрятать/обрезать до подключения** | mod_matrix.rs:477-509, part.rs:1047-1060 | CONFIRMED |
| QW6 | **Undo пропущен для mod-matrix** (source/dest/depth) — Ctrl+Z откатывает мимо правки матрицы, redo-цепь битая | params.rs:1261-1293 | код-верифицирован |
| QW7 | Дедуп `evaluate`/`evaluate_poly_at_delta` (30-arm match ×2) через `accumulate(dest, offset, &mut ModOffsets)` — заодно закрывает фикс QW5 в одном месте | mod_matrix.rs:477-557 | CONFIRMED |
| QW8 | `Filter::reset_for_sr` — 66-строчная копия `new` поле-в-поле (док сам признаёт) → `init_state()`; дрейф = мусорное состояние фильтра после смены SR | filter.rs:201-343 | CONFIRMED |
| QW9 | Миграция SF2-remap расходится: startup `.min(55)` vs kit-load `.clamp(30,55)` (значение из gap 16..29 принимается на старте, клампится при загрузке кита); + вызвать `reset_kit_names()` вместо инлайн-копии | gui/mod.rs:1830-1841 vs sf2.rs:187-203; mod.rs:648-657 vs sf2.rs:97-104 | CONFIRMED |
| QW10 | 4 мутации конфига мимо `mark_config_dirty` (rebind/unbind/reset keybinds, позиция окна браузера) — переживают выход только случайно, теряются при краше | gui/mod.rs:932, 2081, 2094; browser.rs:368 | CONFIRMED |
| QW11 | Seq-stop drain блок ×2 verbatim → `seq_stop_drain()` | mod.rs:1974-1985 vs 2005-2016 | CONFIRMED |
| QW12 | `save_config` после F3 не сохраняет — переименовать (`snapshot_preset_and_mark_dirty`); 32 call site'а читают имя как запись на диск | gui/mod.rs:2392-2398 | CONFIRMED |
| QW13 | Slot Bitcrusher — no-op: `bits`/`downsample` не пишутся нигде (дефолт 16-bit/48k прозрачен), слот не экспонирует параметров; GUI предлагает эффект без звука. Подключить p0/p1 или спрятать | fx_chain.rs:153, mod.rs:2713-2715 | CONFIRMED |
| QW14 | Оптимистичная запись `drum_current_pattern` в console action тихо откатывается атомом через кадр при потере события в кольце — без фидбека. Удалить записи или задокументировать семантику | gui/mod.rs:2436, 2485 vs 959-962 | код-верифицирован |
| QW15 | Мелкий батч: dead-code warning init.rs:100-101 (cfg_attr); `save_patch_in` → `save_json` (preset.rs:590-598); `note_name` ×2 (mod.rs:288, keyboard.rs:10); PAD-таблицы ×2 (keyboard.rs:29 vs settings.rs:21); `PARTS_PER_ZONE` ×2 (mod.rs:7, layers.rs:23); keybinds «unbind-all» восстанавливает дефолты при рестарте (keybinds.rs:24-26); пустой Pitch Bend фрейм (params.rs:1641-1654); `nav_delta`/`pm_before` мусор (settings.rs:402/503, layers.rs:468/483); doc-дрейф (modal.rs:81-92, wave_shaper.rs:193 mode 0..7 vs 25) | — | partial-выборка |

### Strategic (порядок: зависимости)

1. **Голден-свип слотовых параметров** (M) — перед любым FX-фиксом: голден заморозил только дефолтные параметры слота (31 тип × {default, mix 0.5}); свип `fx0_p*` не подтверждён. Без него QW4/fix double-tick/конвертер меняют звук незамеченным. Первым шагом разрешить противоречие «31/31 slot hashes» (fx-аудитор) vs «5/31» (старый аудит).
2. **ST4 dual-FX** (M) — открыт, оба Critical на месте: двойное суммирование (mod.rs:2509-2640), double-tick Overdrive/Phaser/Flanger/Tremolo/Bitcrusher (2511/2518-2521 + apply_fx_slot 2667-2716). Закрытие 2fcd4c3 — деферрал («user hears no doubling»), рецепт готов в TECHDEBT.md:766-774. Конвертер по-прежнему 15/31. Зависимость: пункт 1.
3. **Извлечь looper-capture машину из tick_block** (M-L) — 7 полей `looper_*` (mod.rs:246-267) + 130-строчная машина (mod.rs:2142-2269) в самом нечитаемом месте движка, 67 ссылок на `looper_*`. `MidiLooper::capture_tick()` в looper.rs, возвращает actions. Блокирует любые правки tick_block. Пререквизит: characterization-тесты capture-семантики (сейчас ноль).
4. **8 волноводных семейств + Twist аллоцируют на RT note-on** (M) — pool-clobber: `return_buf` перезаписывает слот пула, dropping ~28k Vec в колбэке (oscillator.rs:467-526); `TwistVoice::new` Box на первом note-on (oscillator.rs:2869-2871). Пины — `#[ignore]`-тесты (tests/rt_safety.rs:287-348), в TECHDEBT/роадмапе отсутствуют → никогда не разморозятся. Fix: swap-пулы (fixed `[Option<Vec<f32>>;4]`) + eager TwistVoice в `Oscillator::new`; вернуть пины.
5. **Drum undo/redo/clear — `Box::new([DrumPattern;8])` ×2 на аудиопотоке** (M) — drum.rs:858-915, каждый жест = 2 снапшота ~2KB в handle_control. Лупер решён swap-ом; у DrumSeqUndo нет rt_safety-жеста.
6. **ProgramChange ≠ LoadPatch** (M, баг) — ProgramChange (mod.rs:1079-1104) пропускает macro_vals sync, arp set_params/latch, lfo_step_seq, mseg, wavetable → после MIDI program change арп/матрица/MSEG в противоречии с патчем. `Part::apply_patch_params()` общий для LoadPatch/ProgramChange/SetPartFull.
7. **PartFullState литерал ×3 с расхождением источников** (S-M) — perf-путь берёт enabled/mute/volume/pan из структуры перфа, остальные поля из `parts[i]`; сегодня совпадает по порядку кода, ничего не enforcement'ит. Конструктор `PartFullState::from_part` рядом со struct + тест паритета полей (это тот же класс бага #40-43a). gui/mod.rs:2214-2226, 2285-2296; performance.rs:116-127; layers.rs:257-272.
8. **F3-харнесс** (M) — конфиг-путь неинъекцируем (config.rs:334-336, `dirs::config_dir()`) → throttle/ошибки/обходы тестировать нельзя. После харнесса: свернуть `global_dirty`+`last_config_save` двойную машину в flush (gui/mod.rs:491-498, 916-920, 1887-1902); `list_json` молча теряет битые JSON (preset.rs:127-136).
9. **ST3-остаток SSOT** (L) — ~76 рукописных arm'ов `set_param` (part.rs:790-879) + `build_voice_params` 111 копий (part.rs:420-494); разблокирует производный тест `VoiceParams::from(&PatchParams)` (value-equality, не только key-sync). S2b/S2c остаются declined.
10. **App-состояние в сабструкры** (L) — методы разнесены, ~180 полей нет (gui/mod.rs:296-579); + LED-socket lifecycle (gui/mod.rs:412-431, 1650-1779) → led_console.rs; + params.rs `draw_params_editable` 1430 строк разрезать механически (`osc_section/fx_section/...`, behavior-preserving).
11. **CommutedPiano velocity→timbre** (S-M, баг по правилу проекта) — клавиатура шлёт всегда 127; hardness/noise/amp/LP зависят от velocity (oscillator.rs:738-868) → миди-файлы с тихими velocity звучат иначе, чем живая игра; рецепт PianoModel уже есть (piano.rs:13-14). Drive hardness от `ks_brightness`, velocity — только амплитуда.
12. **GUI-thread I/O** (M) — парс SF2 (sf2.rs:107-119) и midi-файла (midi_seq.rs:281-320) синхронно в UI-потоке; фриз пропорционален размеру. Worker + mpsc, паттерн `midi_seq_file_pick` уже есть.
13. **Per-sample инварианты** (Low-Med, кластер; важно для Android): exciter powf+exp/сэмпл (exciter.rs:169-170 при готовом паттерне кэша :67-76), vocoder 2×exp/сэмпл (vocoder.rs:178-181), bbd_ensemble exp/сэмпл ×3 (:59,90), resonator set_freq/set_decay 8×/сэмпл + SVF коэфф/сэмпл (mod.rs:2538-2542, resonator.rs:76-80), mute_fade_step + pan sqrt в сэмпл-цикле (mod.rs:2460, 2475-2477). Кэш на param-change по паттерну `Exciter::update_coeffs`.

### Decline

- bonsai `dc_block` (инвертированная конвенция, bonsai.rs:96-107) и neuron `fast_tanh` (Padé 27,9 vs канон 135,62) — **канон №2 CLAUDE.md запрещает слияние разных shape'ов**; задокументировать, не мержить.
- ST8 headless divergence — закрыто юзером (5fde7d1); дубли headless↔gui (SF2-парс headless.rs:256-263 vs sf2.rs:107-119, сидинг) — оставить до ревизии headless.
- EngineHandles обратно размотан в поля App (gui/mod.rs:625-787) — не отдельная работа, входит в пункт 10.
- Reverb2 8 мёртвых LFO-фаз/сэмпл (reverb2.rs:169-173) — удалить при попутном touch, не отдельным PR.
- Хвост Low-дублей gui: browser engine-toggle ×2 (browser.rs:88-99 vs 394-403), remap-хелперы ×2 (drums.rs:528/629), `is_sf2`/`engine_now` (browser.rs:11/14), dead-комментарии (macros.rs:42-44, browser.rs:209/310) — при попутном touch.
- Dead pub surface: ~20-30 pub-полей SynthEngine (mod.rs:180-289, только тесты снаружи), `sampler_release_scale` (mod.rs:457-459), `pub use sf2::*` re-export (gui/mod.rs:284-285), piano.rs:48/397, midi_player.rs:146, `_disp_b0/b1` (oscillator.rs:224-225) — при попутном touch, не батчем.
- Repo-wide fmt, pedantic cast-шум, clipboard-win yanked, крейт-дубли — как в прежних аудитах.
- Тестовые невоспроизводимые классы: `Vec::push` на RT-нотных путях (амортизированный, при попутном touch), drum swing-формула дубль (drum.rs:926 ↔ step_seq.rs:229, закомментировано).

## Per-module health

| Модуль | LOC | Churn (6м) | C/H/M/L | Тема долга |
|---|---|---|---|---|
| synth/mod.rs + part.rs | ~3900 | 104 | 0/3/5/7 | tick_block-монолит + dual-FX + mod-matrix мёртвые dest |
| gui/mod.rs (+сабы) | ~2770+ | 163 | 0/0/4/11 | App 180 полей + F3-граница без харнесса |
| fx_chain + FX-файлы | ~10000 | — | 0/1/3/5 | dual-chain + конвертер 15/31 + слот-контракты (WaveShaper/Bitcrusher/Compressor) |
| oscillator.rs | 2927 | — | 0/1/2/4 | pool-clobber + 43MB пулов + god-file (29 движков) |
| lib/init/headless/jni | ~920 | 71 | 0/0/1/4 | чисто после S4 (один warning) |
| config/preset/cc_map | ~1200 | — | 0/0/3/3 | дефолты ×3 места + list_json молчит |
| mod_matrix/lfo | — | — | 0/2/1/2 | 24 мёртвых dest + tempo-sync + LFO1 rate |
| drum.rs | — | 23 | 0/1/1/1 | undo Box на RT + атомы/сэмпл |
| dsp_utils | — | — | 0/0/1/1 | три канона dc_block (один задокументировать, не мержить) |

## Статус прежних Strategic-предметов

- **ST1 (EngineHandles) — ГОТОВО** (#96), init.rs:43-79, `handles()` — единственная точка сборки.
- **ST2 (транзакции) — ГОТОВО** (SetPartFull, events.rs:49-92; swap = 2 транзакции, perf = 1/part). Остаток: cross-part атомарность — не требуется.
- **ST5 (сплиты) — ГОТОВО** по lib/synth/gui (spлит чистый, move-only, расхождений не найдено); осциллятор-сплит не делался (низкий приоритет).
- **ST6 (S6-дедуп)** — S6a/S6b ГОТОВО и bit-safe верифицировано (xorshift 39 сайтов → канон, dc_block 10 сайтов); остатки — в Decline по канону №2.
- **ST7 (F3) — ГОТОВО** (#102), 29 точек → 1 flush; находки этого аудита (QW1/QW10, пункт 8) — доводка границы.
- **ST8 — CLOSED** юзером.
- **ST3/S2a — частично**, ST4/S3 — открыты (см. Strategic 1-2).

## Refuted / PARTIAL (claims этого аудита)

- «Twist аллоцирует на каждый note-on» — PARTIAL: `Box::new` только на первом Twist note-on за жизнь Oscillator (`twist_voice: None` лишь изначально, oscillator.rs:2869-2871); формулировка в ignored-пине устарела, аллокация реальна.
- fx-аудитор: «goldens incl. 31/31 slot hashes зелёные» — противоречит «5/31» старого аудита; лидом не проверено → UNVERIFIABLE, разрешить первым шагом Strategic-1.
- «goldens 5/31 prereq ST4» (старый аудит) — возможно устарел, см. выше.
- «`_disp_b0/_disp_b1` мёртвое состояние» — не перепроверено (Low, residual).

## Residual risks / UNVERIFIABLE

- S4/F3/S6b не прогнаны на Android-сборке end-to-end (read-only; `./android/build_rust.sh` пишет в target). Cfg-гейтинг `android_main` верифицирован только чтением.
- QW2/QW3 (LFO) и QW14 (pattern-revert) — логика верифицирована, runtime не воспроизведён (нужен 2-частный патч с разными `lfo_rate` / tempo-sync голосовой LFO).
- Pool-clobber у 7 семейств: пины `#[ignore]` (роняют процесс по дизайну), статически свободный путь не восстановлен до конца; severity опирается на runtime-находку S0 2026-10-30.
- Dual-use `lfo_rate` (Hz 0.1..20 vs sync-индекс 0..15) — сама двойная семантика часть путаницы QW3; при фиксе решить, разъезжаются ли CC-диапазоны (по канону №3 — pin).
- flush при `on_exit` не гарантирован на Android (kill фоном) — до ~2 c настроек может теряться независимо от F3; принятый трейд-офф (коммит F3).
- aarch64 float-codegen гольденов отличается от x86_64 (документировано тестом); Plaits `Voice::render` (vendored) на аллокации не аудировался.
- rt_safety не гоняет LoadKeysSoundFont под assert_no_alloc (старый residual, не проверялся).
- Характеризационные пробелы, блокирующие правки: master-capture машина, `handle_event` транспорт-роутинг (mod.rs:913-994), SetPartFull-паритет (mod.rs:1376-1454), `Voice::note_on`/`Part::note_on` play-mode матрица (latch/poly-high/poly-low steal), SF2 mono-stack re-key (mod.rs:505-610), `apply_performance`/browser seed (4 UX-регрессии, 0 тестов), `reconnect_audio` (settings.rs:199-396, 200 строк повторного init-порядка).
