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
| 11 | looper-sustain-capture | in_progress | fix/looper-sustain-capture | — |
| 1 | scene-lfo-envelope | pending | — | — |
| 2 | golden-hermeticity | pending | — | — |
| 3 | sf2-data-small-fixes | pending | — | — |
| 7 | per-sample-exp-powf | pending | — | — |
| 6 | gui-io-worker | pending | — | — |
| 4 | rhythm-low-batch | pending | — | — |
| 8 | tick-block-extract | pending | — | — |
| 10 | params-split | pending | — | — |
| 9 | app-substructs | pending | — | — |
| 5 | rt-allocations | pending | — | — |

## Журнал
- 2026-11-09: конвейер запущен; план 11 начат (Task 1 — characterization).
