# Ночной рефакторинг 2026-11-08 — роадмап и состояние (карт-бланш юзера)

Юзер спит. Работать автономно, шаг за шагом. Каждая стадия: тесты зелёные →
Tavily-исследование лучших практик → план (writing-plans, ревью субагентом,
гейты) → исполнение субагентами (implementer + spec review + quality review на
каждую задачу) → PR squash merge (НИКОГДА локальный merge в master) → обновить
этот файл → следующий шаг. Любой шаг откатываем через PR — слитый шаг = отдельный PR.

## Стадии (порядок фикс, «не двигать код дважды»)

- [x] 0. QW-батч — PR #91, master 7f30b52 (2026-11-07)
- [ ] 1. ST2 — транзакции: `ControlEvent::SetPartFull { part, Box<PartStateMsg> }`
      (свап парта = до 32 событий → 1; reset_and_push_part 13-15 → 1) + ST1
      EngineHandles struct (18 хэндлов × 6 мест → 3). См. REFACTOR_AUDIT_2026-11-07.md F2/F4.
- [ ] 2. ST3 — S2: SSOT-реестр параметров (таблица key→label/range/log/scope/
      default/transform; kill set_param 107 arm / build_voice_params 111 копий /
      CachedFxParams 111 / слайдеры 150 мест / check_pickup 4/32 / range-дрейф
      ks_brightness). Поверхность защищена sweep-golden (1642 хэша).
- [ ] 3. ST4 — S3: FX консолидация. ПРЕГОТОВКА: slot goldens 31/31 (сейчас 5/31,
      golden_engine_fx_chain_slots). Потом: байпас legacy при активной цепочке
      (Critical: эффекты дублируются), нормализованные контракты 5 семейств,
      конвертер 15/31+43 поля. ЗВУК ИЗМЕНИТСЯ ОСОЗНАННО — отдельные PR на
      преготовку и на behavior change, diff ревью, goldens регенерация явная.
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
- ПРИЛОЖЕНИЕ НЕ ЗАПУСКАТЬ и не перезапускать (юзер закрыл сам; screen-check
  стадии «юзером» → отметить «awaiting screen check» в этом файле).
- Гейт каждой задачи/стадии: `cargo clippy --all-targets -- -D warnings &&
  cargo test && cargo test --features rt-assert --test rt_safety && cargo deny check`.
- Behavior-preserving шаги: goldens красные = СТОП и разбор, не регенерация.
  Behavior-changing (S3): регенерация явная, отдельный PR, diff-ревью звука.
- План каждого шага: docs/superpowers/plans/2026-11-08-<step>.md; после
  исполнения — status: executed + чекбоксы закрыты + TECHDEBT подсекция.
- Сеть: `unset LD_PRELOAD` перед gh/git push.
- Если кольцо rtrb/RT-контракт затрагивается — rt_safety обязателен в гейте.
- НЕ трогать старую QW-нумерацию в TECHDEBT (:767) — своя нумерация у каждой
  секции.
- Если что-то пошло не так и PR не сливается/конфликты — не форсить; фикс в
  новой ветке поверх origin/master.

## Инфра ночи
- Cron 7602003c «ночной цикл» каждые 20 мин, session-only: живёт пока жива эта
  сессия Claude (не переживает перезапуск терминала/машины). Итоговый отчёт —
  в конце этого файла по завершении всех стадий.
