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
| 6 | gui-io-worker | in_progress | fix/gui-io-worker | — |
| 4 | rhythm-low-batch | pending | — | — |
| 8 | tick-block-extract | pending | — | — |
| 10 | params-split | pending | — | — |
| 9 | app-substructs | pending | — | — |
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
