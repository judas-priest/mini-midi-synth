# Дневной рефакторинг 2026-11-08 — роадмап (продолжение ночного)

Тот же цикл, что и ночью (юзер утвердил): тесты → Tavily → план (writing-plans,
ревью субагентом) → субагенты (implementer + spec review + quality review) →
гейты → PR → обновить этот файл. КАРТ-БЛАНШ на Tavily до/после каждой итерации.
Порядок задан юзером: начинать с разреза (S4-synth).

## Стадии

- [x] 1. S4-synth — ГОТОВО (PR #98, master 55ee839, 2026-11-08): handle_control
      → 5 subsystem handlers (verbatim, script-verified); Part::update_mods
      (borrow-хак растворён, EngineModInputs); events.rs 333 / part.rs 1063 /
      cached_fx.rs 198; mod.rs 4340→2800. План: 2026-11-08-day1-s4-synth.md.
- [ ] 1-old. S4-synth — разрез synth/mod.rs (4174): (a) extract Part::update_mods
      (растворяет borrow-хак layer_mods + snapshot), (b) handle_control (699
      строк, 88 вариантов) по подсистемам (part/drums/sf2/looper/transport/seq),
      (c) events/GlobalParams/Part в отдельные файлы (events.rs / part.rs) —
      как разрешат borrow'ы. ПОРЯДОК b → a → c: b минимально рискован
      (ручные sub-методы), a требует EngineModInputs, c механика.
- [x] 2. S4-gui — ГОТОВО (PR #99, master 1b48ae9, 2026-11-08): console lifecycle
      → 4 метода; browser.rs 433 / sf2.rs 238 / tracks.rs 292 / performance.rs
      144; mod.rs 3812→2740. Панели не тронуты (пин зелёный). План:
      2026-11-08-day2-s4-gui.md.
- [ ] 2-old. S4-gui — разрез gui/mod.rs (3812): console_lifecycle (786-1038),
      browser (1593-2018), sf2 (2113-2785), tracks (2982-3264), performance
      (3274-3403) → файлы impl App. Панельный пин (ui_layout_contract) уже
      сканирует все gui-файлы — move-safe.
- [x] 3. S2b — ЗАКРЫТО С ОТКЛОНОМ (PR #100, 2026-11-08): «дрейф»
      ks_brightness/ks_feedback — ЛОЖНАЯ ТРЕВОГА: одна пара ключей используется
      ДВУМЯ движками с осознанно разными диапазонами (KarplusStrong vs
      PianoModel — разные ручки/лейблы). Единая таблица key→range сломала бы
      их; контексты (движок, ключ) уже закодированы в call-site литералах.
      Пин осознанных диапазонов добавлен (tc_ks_ranges_intentionally_diverged).
      Полный переписывание 150 слайдеров — отклонено (нулевая поведенческая
      выгода, высокий визуальный риск). Пользовательская проверка НЕ нужна.
- [~] 4. S2c — ОТКЛОНЕНО С ОБОСНОВАНИЕМ: макро-генерация требует таблицы
      transforms (osc_ws_drive*4.0+0.5, u32-casts, эпизодические особые случаи)
      — сложность макроса выше выгоды ~200 строк. Риск класса «добавил параметр
      — забыл копию» остаётся; компенсация — существующие goldens (render
      проверяет ВСЕ параметры через sweep). Дублирование задокументировано.
- [ ] 5. S6b — DC-blocker ×13 в канон (per-site константы сохраняются,
      mono/stereo варианты; wave_shaper linear vs bonsai exp — НЕ сливать).
- [ ] 6. F3 — персист-граница: Persistence::flush() + dirty + debounce
      (55 точек записи → 1). ST8 — headless init consolidate (после 6).

## Правила (как ночью)

- Гейт каждой задачи: clippy -D warnings + cargo test + rt_safety + goldens
  byte-stable (git status tests/golden пуст). Красные goldens = СТОП/откат.
- Behavior-preserving везде: S2b не меняет звук (только GUI-диапазоны к
  единому источнику); ни один PR не меняет рендер без явной регенерации.
- План каждой стадии: 2026-11-08-day-<step>.md; executed-статус + TECHDEBT.
- Сеть: unset LD_PRELOAD. Пользователь нужен ТОЛЬКО на стадии 3 (глазами).
- Cron session-only; по завершении всех стадий — итоговый отчёт здесь + CronDelete.

## УТРОМ/ВЕЧЕР: что проверить на экране
(заполняется по стадиям)
- После стадии 2 (S4-gui): UI выглядит/работает как раньше (панели, браузер,
  треки, настройки).
- После стадии 3 (S2b): слайдеры выглядят как раньше; диапазоны прежние.
