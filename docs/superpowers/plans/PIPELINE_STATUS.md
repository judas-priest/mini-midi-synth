# Pipeline status (карт-бланш конвейер, юзер 2026-11-09)

Порядок: 11 → 1 → 2 → 3 → 7 → 6 → 4 → 8 → 10 → 9 → 5.
Правило: каждый план = worktree-ветка от master, субагенты (имплементер +
спек + quality на таск), финал-ревью, push, **gh pr create БЕЗ merge**.
Файлы планов: 2026-11-08-*.md (scene-lfo-envelope, golden-hermeticity,
sf2-data-small-fixes, rhythm-low-batch, rt-allocations, gui-io-worker,
per-sample-exp-powf, tick-block-extract, app-substructs, params-split,
looper-sustain-capture).

| # | План | Статус | Ветка | PR |
|---|------|--------|-------|----|
| 11 | looper-sustain-capture | DONE (PR #107, не смержен) | fix/looper-sustain-capture | #107 |
| 1 | scene-lfo-envelope | DONE (PR #108, не смержен) | fix/scene-lfo-envelope | #108 |
| 2 | golden-hermeticity | DONE (PR #109, не смержен) | test/golden-hermeticity | #109 |
| 3 | sf2-data-small-fixes | DONE (PR #110, не смержен) | fix/sf2-data-small | #110 |
| 7 | per-sample-exp-powf | DONE (PR #111, не смержен) | perf/exp-powf-hoists | #111 |
| 6 | gui-io-worker | DONE (PR #112, не смержен) | fix/gui-io-worker | #112 |
| 4 | rhythm-low-batch | DONE (PR #113, не смержен) | fix/rhythm-low-batch | #113 |
| 8 | tick-block-extract | DONE (PR #114, не смержен) | refactor/tick-block-extract | #114 |
| 10 | params-split | DONE (PR #115, не смержен) | refactor/params-split | #115 |
| 9 | app-substructs | DONE (PR #116, не смержен) | refactor/app-substructs | #116 |
| 5 | rt-allocations | pending | — | — |

## Журнал
- 2026-11-09: конвейер запущен; план 11 начат (Task 1 — characterization).
- 2026-11-09: план 11 ГОТОВ → PR #107 (не смержен). Ревью поймало 2 HIGH
  (dedup хранил не-держателя ноты; split-кража записи) — оба с регресс-тестами
  RED→GREEN. Начат план 1.
- 2026-11-09: план 1 ГОТОВ → PR #108 (не смержен). Ревью APPROVE с первого
  раза. Начат план 2 (golden hermeticity: fixture-пресеты + положительный
  контроль утечки на СТАРОМ коде до фикса).
- 2026-11-09: план 2 ГОТОВ → PR #109 (не смержен). Позитивный контроль RED
  доказал утечку; 884→282 строк, factory-хэши 1:1; fakehome-грязь → GREEN
  без реген. Начат план 3.
- 2026-11-09: план 3 ГОТОВ → PR #110 (не смержен). Ревью APPROVE: все 6
  фиксов верифицированы, гейты зелёные. Начат план 7 (exp/powf hoists,
  бит-точные).
- 2026-11-09: план 7 ГОТОВ → PR #111 (не смержен). 19 коммитов, 24 сайта
  hoist'нуто (14 synth + 10 airwindows), кэш с NaN-сентинелом признан sound;
  1 LOW (ButterComp2) довжён. Начат план 6 (GUI I/O worker).
- 2026-11-09: план 6 ГОТОВ → PR #112 (не смержен). Ревью APPROVE: worker
  purity, guards, startup drums-очередь (sf2_pending_drums) подтверждены.
  Начат план 4 (rhythm-LOW батч: #14-16,19-21).
- 2026-11-09: план 4 ГОТОВ → PR #113 (не смержен). Ревью APPROVE; известное
  ограничение зафиксировано (per-channel bitmask не различает ноты).
  Начат план 8 (tick_block extract: characterization 3 пробела → verbatim move).
- 2026-11-09: план 8 ГОТОВ → PR #114 (не смержен). Verbatim-дифф: ровно один
  дельта (step → параметр); characterization tc15-18; tick_block 546→414.
  Начат план 10 (params.rs split: фаза A секции-методы + pin-гвард, фаза B
  перенос в params_*.rs).
- 2026-11-09: план 10 ГОТОВ → PR #115 (не смержен). 9 секций байт-идентичны
  (скрипт-проверка ревьюера), pin 87 маркеров; params.rs 1666→668. Отклонение
  по канону #8: pin tc_ks_ranges путь. Начат ПОСЛЕДНИЙ план 9 (App → 12
  сабструктов, compiler-driven).
- 2026-11-09: план 9 ГОТОВ → PR #116 (не смержен). 169 полей → 13 сабструктов
  (comm пусто), android exit 0, ~1500 ссылок переписано, lib.rs дифф пуст.
  КОНВЕЙЕР ЗАВЕРШЁН: 11/11 PR открыты.
