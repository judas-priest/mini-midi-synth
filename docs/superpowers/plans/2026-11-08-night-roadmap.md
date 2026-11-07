# Ночной рефакторинг 2026-11-08 — роадмап и состояние (карт-бланш юзера)

Юзер спит. Работать автономно, шаг за шагом. Каждая стадия: тесты зелёные →
Tavily-исследование лучших практик → план (writing-plans, ревью субагентом,
гейты) → исполнение субагентами (implementer + spec review + quality review на
каждую задачу) → PR squash merge (НИКОГДА локальный merge в master) → обновить
этот файл → следующий шаг. Любой шаг откатываем через PR — слитый шаг = отдельный PR.

## Стадии (порядок фикс, «не двигать код дважды»)

- [x] 0. QW-батч — PR #91, master 7f30b52 (2026-11-07)
- [x] 1. ST2+ST1 — ГОТОВО (PR #92, master 1f50c19, 2026-11-08). SetPartFull:
      свап 44-46→2 событий, спавн/реконнект 13-21→1, apply ~110→9. EngineHandles:
      17 пуллов × 3 сайта → handles(). Trash::PartFull новый вариант, rt_safety
      +1 кейс (SetPartFull+patch в assert_no_alloc). План: 2026-11-08-stage1-
      transactions-handles.md (executed). Tavily-постчек: boxed payload + trash —
      приоритет 1 канона (исследование 2026-11-08), расхождений нет.
- [~] 2. ST3 — S2: подсечение 2a ГОТОВО (PR #93, master ecf283d, 2026-11-08):
      cc_patch_params! SSOT (31 ключ → cc_value_of/cc_set_param/ranges),
      check_pickup 4/32 → все ключи (behavior fix: pickup от реального
      значения), пины registry↔таблица (mutation-verified). ОСТАЛОСЬ 2b
      (GUI-слайдеры 150 мест — требует визуальной проверки, УТРОМ) и 2c
      (build_voice_params/CachedFxParams генерация). Range-дрейф ks_brightness
      живёт в GUI-слайдерах — уйдёт в 2b.
- [~] 3. ST4 — S3: ПРЕГОТОВКА ГОТОВА (PR #94, slot goldens 31/31). Behavior
      change (байпас legacy — Critical дублирование) ОТЛОЖЕН НА ДЕНЬ: требует
      diff-ревью звука юзером. Всё готово к одному PR.
- [x] 3b. S6 дедуп — ГОТОВО (PR #95, master 02d33ac, 2026-11-08): xorshift32
      канон ×31 call-site (14 файлов), phase advance ×10 (7 файлов),
      soft_clip/fast_exp канон + dead вариант удалён. Goldens байт-стабильны
      на каждой задаче. Осталось S6b: DC-blocker ×13, cubic, combulator/floaty
      linear (причины в плане stage3b).
- [ ] 4. ST5 — S4 разрез god-файлов: ночью ограничиться lib.rs сплитом
      (init/headless + App::new) — compiler-enforced; gui/synth сплиты — день.
- [ ] 4. ST5 — S4 разрез god-файлов: lib.rs → init/headless/android_jni + App::new;
      synth/mod.rs → Part::update_mods extract + handle_control по подсистемам;
      gui/mod.rs → console_lifecycle/browser/sf2/tracks/performance. ПОСЛЕ S2/S3.
- [ ] 5. ST6 — S6 дедуп dsp_utils (bit-safe кластеры: xorshift ×27, phase ×13,
      delay.rs linear, soft_clip, fast_exp, DC ×13 по coeff-группам; НЕ сливать
      cubic и combulator/floaty linear).
- [ ] 6. ST7 — персист: Persistence::flush() + dirty + debounce (55 точек → 1).
- [ ] 7. ST8 — headless init consolidate с send_initial_presets + characterization.

## Правила ночи

- СПЯЩИЙ ЮЗЕР: не задавать вопросов; блокер → пометить в этом файле «BLOCKED:
  причина», перейти к следующей стадии (порядок 1→2→3 ломать можно, skip — ок).
- Приложение ЗАПУСКАТЬ можно и нужно (проверка запускаемости после сборки);
  юзер проверять ночью не будет — после каждой стадии добавлять пункт в список
  «УТРОМ: что проверить на экране» в конце этого файла (awaiting screen check).
- Гейт каждой задачи/стадии: `cargo clippy --all-targets -- -D warnings &&
  cargo test && cargo test --features rt-assert --test rt_safety && cargo deny check`.
- Behavior-preserving шаги: goldens красные = СТОП и разбор, не регенерация.
  Behavior-changing (S3): регенерация явная, отдельный PR, diff-ревью звука.
- План каждого шага: docs/superpowers/plans/2026-11-08-<step>.md; после
  исполнения — status: executed + чекбоксы закрыты + TECHDEBT подсекция.
- Сеть: `unset LD_PRELOAD` перед gh/git push.
- Tavily-исследование лучших практик — карт-бланш В ЛЮБОЕ ВРЕМЯ: обязательно
  ДО написания плана стадии и ПОСЛЕ каждой итерации/PR (сверка сделанного с
  каноном; расхождение → пометка в roadmap + фикс следующим шагом).
- Если кольцо rtrb/RT-контракт затрагивается — rt_safety обязателен в гейте.
- НЕ трогать старую QW-нумерацию в TECHDEBT (:767) — своя нумерация у каждой
  секции.
- Если что-то пошло не так и PR не сливается/конфликты — не форсить; фикс в
  новой ветке поверх origin/master.

## Инфра ночи
- Cron 7602003c «ночной цикл» каждые 20 мин, session-only: живёт пока жива эта
  сессия Claude (не переживает перезапуск терминала/машины). Итоговый отчёт —
  в конце этого файла по завершении всех стадий.


### После стадии 3b (PR #95): звука НЕ коснулось (bit-identity goldens) —
проверять нечего, но прогнать пресеты на слух по фану можно.

### После стадии 2a (PR #93)
6. MIDI-learn: назначить CC на chorus_mix (или любой из ~27 ранее
   непокрытых ключей) — pickup подхватывает БЕЗ прыжка в ноль.

## УТРОМ: что проверить на экране

### После стадии 1 (PR #92)
1. Свап партов драгом (⠿): поменялись местами, инструменты/громкость/мьют едут
   со слотами, активный парт следует.
2. Спавн нового парта («+ Add part»): появился с дефолтным патчем, играет.
3. Settings → Reconnect Audio: состояние партов (мьюты, громкость, SF2-режимы,
   диапазоны) сохранилось после реконнекта.
4. Применение перфа из списка: патчи/диапазоны/мьюты применились.
5. Транзакции не должны были изменить ЗВУК — если что-то звучит иначе, это баг.


# ИТОГОВЫЙ ОТЧЁТ НОЧИ 2026-11-08 (для юзера)

## Слито: 5 PR (все через PR, откат любого — revert одного squash-коммита)
1. **PR #92 — stage 1**: транзакции SetPartFull (свап парта 44-46 событий → 2,
   спавн/реконнект парта → 1, apply перфа ~110 → 9; теперь разрыв кольца не
   даёт half-swap) + EngineHandles (17 хэндлов × 3 сайта вместо 6).
2. **PR #93 — stage 2a**: SSOT-таблица 31 CC-ключа; **behavior fix**: MIDI-pickup
   теперь работает на всех ключах (было 4 из 32 — ручка требовала прогрузки в 0).
3. **PR #94 — stage 3-prep**: golden-пины всех 31 FX-слотов (было 5) — S3 готов
   к behavior-PR.
4. **PR #95 — stage 3b (S6)**: дедуп ×50 сайтов (xorshift32/advance_phase/
   soft_clip/fast_exp) — ЗВУК НЕ ИЗМЕНИЛСЯ (goldens байт-в-байт на каждом шаге).
5. **PR #96 — stage 4a (S4-lib)**: lib.rs 1090→219 (init/headless/android_jni/
   App::new). Android-сборка верифицирована; попутно починен СТАРЫЙ баг:
   android-gui сборка была сломана с round-5 (4ce2635).

## Звук
Bit-identity: PR #95/96/94/92 не меняют рендер (goldens байт-в-байт).
ЕДИНСТВЕННОЕ поведенческое изменение — PR #93 (pickup) и потенциально
неслышимое (транзакции).

## УТРОМ: что проверить на экране
1. (PR #92) Свап партов ⠿-драгом; спавн парта; Settings→Reconnect Audio —
   состояние партов сохранилось; apply перфа.
2. (PR #93) MIDI-learn на chorus_mix (или др. из ~27 ключей) — pickup без
   прыжка в 0.
3. (PR #96) Приложение запускается и работает как раньше (сплит lib.rs —
   звук/поведение идентичны); android-APK по желанию (jniLibs пересобран).
4. Общее: прогнать пару перфов/пресетов на слух.

## ДЕНЬ (в порядке приоритета)
1. S3 behavior PR: байпас legacy-цепочки (сейчас эффекты дублируются — Critical
   из аудита); goldens 31/31 готовы; нужен твой diff-ревью звука.
2. S4-gui: gui/mod.rs 3567 → console_lifecycle/browser/sf2/tracks/performance.
3. S4-synth: synth/mod.rs Part::update_mods + handle_control по подсистемам.
4. S2b: GUI-слайдеры на таблицу (150 мест) — нужна визуальная проверка.
5. F3 персист-граница; ST8 headless; S6b DC-blocker.
