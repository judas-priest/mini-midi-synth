# Аудит пресетов — похожие/дублирующие звуки

## 🔴 Почти неотличимы на слух

### 1. Saw Lead × 3
`saw_lead` / `mono_lead` / `detuned_lead`

Все три: один saw-осциллятор, LP фильтр, быстрая атака, sustain ~0.8.
Разница только в detune (0.1 / 0.006 / 0.022) и cutoff (3000 / 2200 / 3500).

**Рекомендация:** оставить `mono_lead`, удалить два других.

---

### 2. Acid Bass × 3
`acid_bass` / `tb303_acid` / `tb303_acid_v2`

Все три: saw, Diode/LP фильтр с высокой резонанцией (~0.82–0.85), короткий decay, no sustain.
`tb303_acid_v2` точнее всего — добавляет portamento и key tracking.

**Рекомендация:** оставить `tb303_acid_v2`, удалить два других.

---

### 3. TB-303 Square × 2
`tb303_square` / `tb303_square_v2`

Буквально один пресет. v2 добавляет portamento — это незначительно.

**Рекомендация:** оставить `tb303_square_v2`, удалить `tb303_square`.

---

### 4. Warm Pad = Chorus Pad
`warm_pad` / `chorus_pad`

Идентичная архитектура (3× saw, osc2_detune: 0.01).
`chorus_pad` — это `warm_pad` с `chorus_mix: 0.6`. Не отдельный пресет.

**Рекомендация:** оставить `warm_pad`, удалить `chorus_pad`.

---

### 5. FM Bell × 2
`fm_bell` / `bell_chime`

Одинаковые `fm_ratio: 3.5`, `fm_index: 5.0`. Разница только в `amp_decay` (1.5 vs 2.5).
`bell_chime` дольше звучит — чуть лучше как tubular bell.

**Рекомендация:** оставить `bell_chime`, удалить `fm_bell`.

---

## 🟡 Очень похожи — стоит оставить только лучший

### 6. Медленный тёмный пад × 2
`ambient_pad` / `ethereal_pad`

Оба: slow attack (~1.4–1.5 с), low cutoff (800–900 Гц), 3× saw, долгий release.
`ethereal_pad` использует BandPass вместо LP — незначительно разное на слух.

**Рекомендация:** оставить `ambient_pad`, удалить `ethereal_pad`.

---

### 7. Synth Stab × 2
`synth_stab` / `scooter_hard_stab`

Оба: short decay, no sustain, filtered stab-звук.
`scooter_hard_stab` богаче (3 осциллятора, overdrive) — но концепция та же.

**Рекомендация:** оставить `scooter_hard_stab`, удалить `synth_stab`.

---

### 8. Струнный пад × 2
`string_pad` / `cs80_strings`

Оба: медленная атака, фильтрованный saw пад "под струнные".
`cs80_strings` богаче (unison 5 голосов, LFO) — явно лучше.

**Рекомендация:** оставить `cs80_strings`, удалить `string_pad`.

---

### 9. Supersaw пад × 3
`jp_supersaw` / `supersaw_trance` / `scooter_rave_pad`

Все три: supersaw, схожий cutoff и envelope — "рейв пад".
`jp_supersaw` — самый открытый (cutoff 10000), `supersaw_trance` — средний (3000), `scooter_rave_pad` — с реverbом.
`supersaw_pad` тоже сюда относится.

**Рекомендация:** оставить `jp_supersaw` + `supersaw_pad`, удалить `supersaw_trance` и перенести логику в scooter категорию.

---

### 10. Moog-style Bass × 2
`moog_bass` / `scooter_hard_bass`

Оба: saw + square, Moog 24dB фильтр, похожий envelope.
`scooter_hard_bass` добавляет overdrive — заметнее, но территория одна.

---

### 11. Synth Brass × 2
`synth_brass` / `synth_brass_80s`

Оба: 3× saw, медленноватая атака, LP фильтр с envelope.
`synth_brass_80s` использует osc3=square + Moog filter + unison — заметно богаче.

**Рекомендация:** оставить `synth_brass_80s`, удалить `synth_brass`.

---

### 12. Iconic Brass × 2
`prophet_brass` / `jump_brass`

Оба: "синтетическая медь" с пиком filter envelope, в одной категории (Iconic).

---

## 🟢 Похожи архитектурно, но реально разные на слух — оставить оба

| Пара | Почему разные |
|------|---------------|
| `blade_runner` vs `ambient_pad` | blade_runner темнее, LFO pitch+filter, unison 4 |
| `moog_lead` vs `minimoog_lead` | minimoog тройной osc + portamento |
| `odyssey_lead` vs `sync_lead` | hard sync звучит принципиально иначе |
| `reese_bass` vs `wobble_bass` | reese = детюнированный дрон, wobble = фильтр-движение |
| `dx7_bass` vs `fm_bass` | разный характер FM (fm_index 5.5 vs 3.5, разный decay) |

---

## Итог: кандидаты на удаление

| Удалить | Оставить |
|---------|----------|
| `saw_lead` | `mono_lead` |
| `detuned_lead` | `mono_lead` |
| `tb303_acid` | `tb303_acid_v2` |
| `acid_bass` | `tb303_acid_v2` |
| `tb303_square` | `tb303_square_v2` |
| `chorus_pad` | `warm_pad` |
| `fm_bell` | `bell_chime` |
| `ethereal_pad` | `ambient_pad` |
| `synth_stab` | `scooter_hard_stab` |
| `string_pad` | `cs80_strings` |
| `synth_brass` | `synth_brass_80s` |
| `supersaw_trance` | `jp_supersaw` + `supersaw_pad` |

**Итого: 12 пресетов к удалению из ~130 активных.**
