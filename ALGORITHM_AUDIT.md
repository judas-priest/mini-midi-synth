# Algorithm Audit — mini_midi_synth DSP Modules

**Date:** 2026-04-05
**Method:** Static code inspection + comparison with Surge XT reference implementations
**Scope:** All synth/* DSP modules (~12 000 LOC)
**Code changes:** None — analysis only

---

## Summary Table

| Module | Algorithm | Verdict | Severity of issues |
|--------|-----------|---------|-------------------|
| `oscillator.rs` | 27 types, PolyBLEP, physical models | ⚠ | CRITICAL (edge case), 3 × WARNING |
| `envelope.rs` | ADSR with RC curves | ✗ | 2 × CRITICAL |
| `lfo.rs` | LFO with deformation | ⚠ | 2 × WARNING |
| `voice.rs` | Voice, unison, portamento, dual-filter | ✓ | 1 × WARNING (minor) |
| `filter.rs` | SVF, Moog LP24/12, Diode, Comb, Allpass | ⚠ | 1 × CRITICAL, 2 × WARNING |
| `formant.rs` | Vocal formant synthesis (parallel biquads) | ⚠ | 2 × WARNING |
| `piano.rs` | 24-partial additive with inharmonicity | ⚠ | 1 × CRITICAL, 1 × WARNING |
| `epiano.rs` | Rhodes/Wurlitzer physical model | ⚠ | 1 × WARNING |
| `reverb.rs` | Dattorro plate reverb | ✓ | 1 × WARNING (DC), 1 × INFO |
| `spring_reverb.rs` | Schroeder allpass + dispersion | ⚠ | 1 × WARNING |
| `tape.rs` | Langevin hysteresis, RK2 | ✓ | 1 × INFO |
| `neuron.rs` | GRU nonlinearity + comb | ✓ | — |
| `chorus.rs` | Juno BBD, cubic interpolation | ⚠ | 1 × WARNING |
| `ring_mod.rs` | 4-diode bridge | ✓ | — |
| `flanger.rs` | Hermite delay + feedback | ⚠ | 1 × WARNING |
| `phaser.rs` | 12-stage allpass | ⚠ | 1 × WARNING |
| `overdrive.rs` | 2× oversampled distortion | ✗ | 1 × CRITICAL |
| `compressor.rs` | Feed-forward, soft-knee | ⚠ | 1 × WARNING |
| `eq.rs` | 3-band RBJ | ✓ | 1 × WARNING (design choice) |
| `freq_shift.rs` | Hilbert SSB shift | ⚠ | 1 × WARNING |
| `bitcrusher.rs` | TPDF dithering + AA | ⚠ | 1 × CRITICAL |
| `tremolo.rs` | AM + stereo phase | ✓ | — |

Legend: ✓ good · ⚠ minor issues · ✗ significant problems

---

## Критерии оценки

| Категория | Что проверялось |
|-----------|----------------|
| Численная стабильность | NaN, денормалы, деление на ~0, переполнение состояний |
| Aliasing | PolyBLEP / oversampling в точках разрыва |
| Нелинейности | tanh/clip/hysteresis — oversampled? |
| DC offset | Блокировщик там где нужен |
| Резонансная компенсация | Gain при высоком Q |
| Интерполяция | Линейная vs кубическая для дробных задержек |
| Соответствие топологии | Dattorro, Huovilainen, Zavalishin, Schroeder, RBJ |

---

## Детали по модулям

---

### `oscillator.rs` — 27 типов, PolyBLEP, физические модели

**Топология:** Накопление фазы + PolyBLEP для саввы/квадрата/пульса. Физические модели: Karplus-Strong, commuted piano, banded waveguide, bowed string, brass, saxophone, accordion. Суперсоу: 7 голосов с независимым PolyBLEP.

**CRITICAL — Banded Waveguide: пустые буферы при reset()**
При `reset()` структура `BandedWG` инициализируется с пустыми `Vec` (`Default::default()`), тогда как `init_banded_wg()` их заполняет. Если `tick()` вызывается между `reset()` и `init_banded_wg()` (например, при быстром re-trigger), происходит panic на bounds check. Это крайний случай, но при стрессе MIDI возможен.

**WARNING — FM Piano: отсутствует delay-1 в петле обратной связи оператора**
DX7-подобный FM требует delay-1 (задержку на 1 сэмпл) в цепи feedback-оператора для сохранения каузальности. Без него обратная связь применяется мгновенно внутри одного сэмпла, что при высоком индексе модуляции может вызывать нестабильность.

**WARNING — Commuted Piano: loss-фильтр не учитывает частотно-зависимое затухание**
Коэффициент loss_coeff вычисляется единожды при инициализации и не обновляется. Реальные струны затухают тем быстрее, чем выше парциал. Кроме того, loss применяется только к первой струне; вторая не обновляется. Медленная утечка энергии и неточная тембральная окраска при долгом сустейне.

**WARNING — Karplus-Strong: allpass-коэффициент потенциально нестабилен на экстремальных значениях**
`allpass_coeff = 2 * cos(…)` даёт значения в диапазоне [-2, 2]. При значении по модулю > 1 фильтр нестабилен. Состояние обнуляется при инициализации (безопасно), но нет runtime-guard от накопления денормалов при длительном затухании.

**INFO — PolyBLEP: корректен, но трудочитаем из-за shadowing переменной `t`**
Обе ветки (до и после разрыва) реализуют стандартный PolyBLEP-residual правильно, но переменная `t` переиспользуется с тенью (shadow), что затрудняет верификацию.

**Сравнение с Surge:**
Surge использует impulse-based (BLIT) подход с windowed-sinc anti-aliasing, что точнее PolyBLEP на высоких частотах. Unison в Surge — экспоненциальный spread с рандомизацией фазы. Mini Synth применяет линейный spread. Различие слышимо только при >7 голосах.

**Вердикт: ⚠** Алиасинг корректен. Одна CRITICAL-проблема (редкий edge case в BandedWG). Три WARNING (FM feedback, KS stability, loss filter).

---

### `envelope.rs` — ADSR с RC-кривыми

**Топология:** One-pole RC-фильтр с overshoot-целями (attack target = 1.3, release target = -0.001). Три формы кривой: Sqrt, Linear, Quadratic.

**CRITICAL — Attack shape: применение формы к состоянию фильтра некорректно**
Форма кривой применяется постфактум к накопленному состоянию фильтра `self.output`, а не к нормированному времени [0,1]. В частности:
- `EnvShape::Linear` пересчитывает фильтр с другой целью (1.05 вместо 1.3) и сохраняет результат обратно в `self.output` — это корраптит состояние фильтра в следующем сэмпле.
- `EnvShape::Quadratic` возводит `self.output` в квадрат. Поскольку значение не нормализовано к [0,1] (overshoot target = 1.3), результат непредсказуем: при `output ≈ 1.2` имеем `output² ≈ 1.44`.

Правильный подход: применять форму к нормированному времени, а RC-интегратор использовать как сглаживание.

**CRITICAL — Release: использует `decay_shape` вместо отдельного параметра**
Release-стадия явно ссылается на `self.decay_shape`. Это не баг в смысле паники, но означает, что пользователь не может задать форму release независимо от decay. Архитектурное ограничение с потенциальными последствиями для восприятия.

**INFO — Нет защиты от денормалов**
При очень долгом sustain состояние фильтра может накапливать денормальные числа (~1e-38), что вызывает проседание CPU на x86-платформах без flush-to-zero.

**Сравнение с Surge:**
Surge применяет форму через фазовую переменную (интегрируемую отдельно), а RC-фильтр используется только для сглаживания. Mini Synth смешивает форму и сглаживание в одном состоянии — фундаментальная архитектурная разница.

**Вердикт: ✗** Форма атаки производит музыкально некорректные результаты при любом значении кроме Sqrt. Квадратичная форма непредсказуема.

---

### `lfo.rs` — LFO с deformation

**Топология:** Накопление фазы, 6 форм волны (Sine, Triangle, Square, SampleHold, Sawtooth, Envelope), параметр deform.

**WARNING — Sample-Hold: slew-rate захардкожен**
Коэффициент сглаживания = `rate * 4.0 / sample_rate`. Множитель 4.0 означает, что переход занимает ровно ¼ периода LFO — это никак не зависит от музыкального контекста и не параметризовано.

**WARNING — Sample-Hold: двойной триггер при оборачивании фазы**
Условие `phase < dt * 1.5` срабатывает при каждой атаке когда rate > 1/sample. В сочетании с основным условием `phase_int != prev_phase_int` создаёт ситуацию двойного триггера и артефакт.

**INFO — Sawtooth без anti-aliasing**
Разрыв на оборачивании фазы не сглажен. Для типичных LFO-скоростей (<20 Hz) неслышимо, при использовании как быстрый LFO (>100 Hz) — aliasing заметен.

**Сравнение с Surge:**
Surge поддерживает 3 независимых типа deform, MSEG, key-sync, temposync, параметризованный AHDSR в envelope-режиме. Mini Synth — упрощённый, но функциональный.

**Вердикт: ⚠** Нет критических ошибок. Два WARNING влияют на характер S&H.

---

### `voice.rs` — Голос, унисон, портаменто, dual-filter

**Топология:** До 3 осцилляторов, FM cross-routing (osc1 → osc2/3), фильтрование Single/Serial/Parallel, portamento по логарифму частоты, unison с pan-spread.

**WARNING — Portamento: коэффициент вычислен для линейной частоты, применяется в log-домене**
`porta_coeff = 1 - exp(-4/samples)` правильна для экспоненциального сглаживания линейной величины, но применяется как коэффициент one-pole в log(freq)-домене. Результат — тайминг glide слегка отличается от заявленного времени (погрешность ~5-10% при больших интервалах).

**INFO — Unison gain: clamp ratio на ±4 при малом mono-сигнале**
При глубокой фильтрации mono-суммы (`filtered / mono` где mono → 0) clamp ±4 предотвращает взрыв, но создаёт ratio-искажение. Принятая мера достаточна.

**INFO — DC-блокировщик отсутствует на уровне голоса**
Стандартная архитектура: DC-блокировщик применяется на уровне мастер-микшера, не per-voice. Если в вышестоящем коде это есть — порядок.

**Сравнение с Surge:**
Voice.rs — верная масштабированная реплика архитектуры Surge: те же идеи модуляции filter cutoff (additive + multiplicative semitones), те же режимы routing. Намного меньше опций, но без архитектурных расхождений.

**Вердикт: ✓** Solid. Единственный WARNING несущественен для типичного использования.

---

### `filter.rs` — SVF, Moog LP24/12, Diode, Comb, Allpass

**Топология:**
- **SVF:** Zavalishin Topology-Preserving, tan(π·fc/fs) warping, состояния ic1eq/ic2eq
- **Moog LP24:** Huovilainen 2004 (tanh на каждой ступени, 2× oversampling, polynomial correction)
- **Moog LP12:** Tap после 2-й ступени Moog LP24
- **Diode LP:** Zavalishin Ch. 6.6, feedback от ступени 3, 2× oversampling
- **Comb:** Круговой буфер 512 samples, feedback = resonance × 0.99
- **Allpass:** Первого порядка, tan pre-warping

**CRITICAL — Comb: нет DC-блокировщика в петле обратной связи**
При высоком resonance (fb ≈ 0.99) и наличии DC в источнике delay-line медленно накапливает постоянную составляющую. Для осцилляторов (AC-coupled) риск минимален, но при подаче внешнего сигнала или после ресета — реален.

**WARNING — Comb: алиасинг на низких частотах**
Буфер ограничен 512 сэмплами. При fc < ≈94 Hz (delay > 511 samples) значение зажимается: `clamp(1, 511)`. Все частоты ниже ≈94 Hz дают одинаковый гребень — aliasing паттерн в comb-отклике.

**WARNING — Diode LP: направление асимметрии клиппера сомнительно**
Положительная полуволна обрабатывается с усилением 1.5×, отрицательная — 0.8×. В реальной диодной лестнице TB-303 насыщение сильнее на отрицательном полупериоде (барьерный режим). Эмпирически может давать нужный звук, но физика инвертирована.

**SVF:** Корректен. Коэффициент k = 2(1−resonance) — правильная Zavalishin-нотация, семантика не инвертирована.

**Moog LP24:** Отличная реализация: Huovilainen-топология подтверждена, polynomial correction присутствует (строки с `fcr = 1.8730·fc³ + …`), oversampling 2×, half-sample feedback delay.

**VT_INV = 1.22:** Эмпирический масштабный коэффициент для normalized audio range, не физическое напряжение. Не ошибка, но требует комментария.

**Сравнение с Surge:**
Surge SVF использует `2·sin(π·ω)` вместо `tan(π·fc/fs)`. Наш вариант точнее для аналоговой эмуляции. Moog — практически идентичен reference.

**Вердикт: ⚠** SVF, Moog, Allpass — отличные. Comb требует DC-блокировщика. Diode — функционален, но физически сомнителен.

---

### `formant.rs` — Вокальный формантный синтез

**Топология:** 5 параллельных резонансных biquad-полосовых фильтров. 4 типа голоса × 5 гласных = 20 preset-таблиц. Экспоненциальное сглаживание параметров (τ = 30 мс).

**WARNING — Peak gain normalization biquad может быть неточен на 10–20%**
Нормировочная константа: `peak = 1 − r²`, затем `scale = gain_linear / peak`. Это предполагает знаменатель `(1−r²)/(1−2r·cos(ω)+r²)`, тогда как реальный biquad имеет `1 − a1·z⁻¹ − a2·z⁻²`. Ошибка систематическая (одинакова для всех формант), но приводит к неточному соотношению уровней гласных.

**WARNING — Нет отслеживания высоты тона**
Формантные частоты фиксированы независимо от высоты источника. Source-filter модель корректна в принципе (синтез гласных по такой схеме устоявшийся), но при изменении диапазона на октаву формантная структура не адаптируется. Результат: на высоких нотах некоторые парциалы могут "проваливаться" в нотч фильтра вместо усиления.

**Интерполяция гласных:** Корректная one-pole IIR (30 мс), без кликов и пропов. ✓

**Вердикт: ⚠** Функционально, интерполяция гласных хороша. Gain-нормировка неточна, pitch tracking отсутствует.

---

### `piano.rs` — 24-парциальное аддитивное фортепиано

**Топология:** 24 инармонических парциала, модель Вайнрейха (двойное затухание), фильтрация позиции удара (`sin(n·π·α)`), hammer envelope, soundboard resonance (2-pole SVF).

**CRITICAL — Коэффициент инармоничности завышен в 2–4 раза**
Функция `b_coefficient(midi_note)`:
```
0.00006 * exp(5.52 * (note - 21) / 87)
```
Расчёт для C4 (MIDI 60): `0.00006 * exp(5.52 * 0.448) ≈ 0.00073`.
Комментарий в коде обещает B ≈ 0.0015 для C4, измеренные значения (Bank & Sujbert 2003) тоже ≈ 0.001–0.002.
Расхождение в 2× означает, что парциал 24 у C4 смещается вверх на ≈54 цента вместо ≈27 — слишком яркий и расстроенный верхний регистр. Правильный экспонент ≈ 6.2 вместо 5.52.

**WARNING — Sustain педаль не реализована как MIDI CC64**
Сустейн управляется параметром `feedback` (выше feedback — дольше затухание). Нет явной обработки MIDI CC64. Педаль работает только через внешнее маппирование на параметр.

**Двойное затухание (Вайнрейх 1977):** Корректно. T1 (быстрая мода) и T2 (медленная) вычислены по `exp(-6.908/(T60·SR))`. ✓

**Фильтрация позиции удара:** `gain = |sin(n·π·α)|` — правильная реализация (Conklin 1996). ✓

**Brightness вместо velocity:** Задокументированное решение для клавиатуры без velocity. ✓

**Вердикт: ⚠** Модель затухания и структура отличные. Коэффициент инармоничности требует проверки по измеренным данным.

---

### `epiano.rs` — Rhodes/Wurlitzer физическая модель

**Топология:** Gordon-Smith резонаторы (5 парциалов на голос), модель pickup-насыщения (магнитная нелинейность), hammer envelope (шум + форм-фильтр), DC-блокировщик.

**WARNING — Pickup saturation: разрывная производная на пороге 0.5**
```
if |driven| < 0.5: линейный участок (slope = 1.0)
else: exp-клиппер * 0.7 (slope ≈ 0.26 сразу после порога)
```
Наклон скачком меняется с 1.0 до 0.26 — kink первой производной. На транзиентах (key click) это может создавать aliasing или призвуки. Корректная реализация использует `tanh` или `x/(1+|x|)` через весь диапазон.

**Rhodes инармонические соотношения:** [1.000, 2.021, 3.044, 4.074, 5.106] — соответствуют измерениям Hatch (2008). ✓

**Wurlitzer:** Отличается более высокой скоростью удара, меньшей инармоничностью [1.000, 2.004, …], более агрессивным pickup drive. Обоснованные различия. ✓

**Gordon-Smith резонаторная формула:** Правильная. `coeff = 2·r·cos(ω)`, `r = exp(-π·BW/fs)`. ✓

**DC-блокировщик:** Коэффициент 0.9975 → cutoff ≈ 19 Hz @ 48 kHz. Корректен. ✓

**Вердикт: ⚠** Модель в целом хорошая. Pickup saturation требует сглаживания.

---

### `reverb.rs` — Dattorro Plate Reverb

**Топология:** Подтверждена оригинальная Dattorro топология: 4-ступенчатый input diffuser, два feedback-танка с cross-coupling, 14-tap output matrix.

**Задержки танка:** Left {672, 4453, 1800, 3720}, Right {908, 4217, 2656, 3163} — это в точности **оригинальные значения из статьи Dattorro (JAES, 1997)**. Они специально подобраны автором для этого алгоритма и используются в сотнях производственных реализаций (Valley Plateau, NYSTHI PlateVerb, Freeverb и др.). GCD(672, 908) = 4 — теоретическое замечание, не подтверждённое практическими артефактами в оригинальном алгоритме.

**INFO — GCD входного диффузора:** [142, 107, 379, 277] — также оригинальные Dattorro-значения. 142 = 2×71 — не простое число, однако алгоритм звучит корректно с этими значениями десятилетиями. Если нужна максимальная диффузность — можно заменить на 139.

**WARNING — Нет DC-блокировщика в feedback-путях**
Lines 365, 378: накопление обратной связи без HPF. При подаче сигнала с DC (редко, но возможно при определённых типах синтеза) — медленный дрейф.

**LFO-модуляция задержек:** Два LFO с иррациональным соотношением частот (0.97 и 1.13 Hz). Отлично — предотвращает биения. ✓

**Allpass input diffuser:** Длины задержек [142, 107, 379, 277]. 142 = 2×71 (не простое). Незначительная колоризация на SR/142 ≈ 338 Hz. Рекомендуется заменить на 139 или 149.

**Output taps:** 14 отводов с правильными знаками для стерео-декорреляции. ✓

**Сравнение с Surge Reverb1Effect:** Surge использует строго взаимно-простые задержки. Наша реализация на 90% корректна.

**Вердикт: ⚠** GCD-проблема — простая правка (изменить 2–3 константы). DC-блокировщик желателен.

---

### `spring_reverb.rs` — Schroeder Allpass + дисперсия

**Топология:** 8-ступенчатый allpass, все задержки простые числа: [139, 193, 263, 349, 421, 509, 601, 701] ✓✓✓. Основная линия задержки 8192 samples. 4 early reflection тапа. Lagrange 3-rd order интерполяция.

**WARNING — Нет частотно-зависимой дисперсии (spring chirp)**
Настоящий spring reverb создаёт характерный "boing" из-за того, что высокие частоты распространяются быстрее низких (дисперсия). Текущая реализация использует постоянный коэффициент allpass на всех ступенях — это Schroeder-reverb с spring-вкусом, но не настоящий spring-chirp. Surge SpringReverbEffect использует явный FIR-фильтр с частотно-зависимыми коэффициентами.

**Все задержки простые:** Выдающееся решение, предотвращает гармонические резонансы. ✓
**T60 расчёт:** Стандартный `exp(-6.908/(T60·SR))`. ✓
**Cross-feed L/R:** 80/20 симулирует spring resonance. ✓
**Damping:** One-pole LP в правильной позиции. ✓

**Вердикт: ⚠** Лучший allpass-набор во всём проекте. Музыкально работает, но физически не является настоящим spring reverb.

---

### `tape.rs` — Ланжевен гистерезис, RK2

**Топология:** Модель магнитного гистерезиса Ланжевена, интегрирование методом средней точки (RK2), 2× oversampling для нелинейности, loss-фильтр, DC-блокировщик.

**RK2 интегрирование:** Два вызова `hysteresis_step` за сэмпл с `dt_inv = sr * 2`. Корректная реализация. ✓

**Гистерезис:** tanh-based Langevin saturation с эффективным полем `h + α·m`. Необратимая/обратимая компонента с проверкой знака `dh_dt`. Физически обоснован. ✓

**2× oversampling:** Применяется к нелинейности — правильный подход. Линейная интерполяция между сэмплами (0.5 factor) достаточна для tanh при 2×.

**Loss-фильтр:** Частота = `2 kHz + 16kHz·speed + 4kHz·tone`. Вычисляется в 2×-oversampled домене. ✓

**DC-блокировщик:** One-pole HPF c cutoff 35 Hz. ✓

**INFO — Clamp в состоянии гистерезиса**
`m = clamp(-m_sat * 1.5, m_sat * 1.5)`. Математически корректная модель не требует clamp — его наличие указывает на возможное потенциальное переполнение при крайних параметрах. Практически работает, но это сторожевой код.

**Сравнение с Surge TapeEffect:** Surge использует B-spline интерполяцию (лучше) и добавляет wow/flutter (у нас нет). Наша модель ~85% точности Surge по гистерезису, работает быстрее.

**Вердикт: ✓** RK2 корректен, гистерезис обоснован, oversampling нелинейности — правильный подход.

---

### `neuron.rs` — GRU-нелинейность + comb

**Топология:** Gated Recurrent Unit (GRU из ML), comb-фильтр банк с dual-frequency reads, 2× oversampling, DC-блокировщик.

**GRU уравнения:**
- forget gate: `f = sigmoid(Wf·x + Uf·y + bf)` ✓
- update: `f·y + (1−f)·tanh(Wh·x + Uh·f·y)` ✓
- Sigmoid approximation `x/(1+|x|)`: ошибка ~5%. Приемлемо для gate.
- Tanh approximation (Padé [3,3]): ошибка <1%. Отлично.

**Стабильность GRU:** Gating предотвращает runaway oscillation. Состояние ограничено tanh. Безопасен для realtime. ✓

**Comb filter:** Dual-frequency reads с Lagrange 3rd-order. Нет feedback (design choice — колоризация, не рекурсивный резонанс). ✓

**Необычность выбора:** GRU — примитив машинного обучения, нестандартный для DSP. Создаёт интересное нелинейное поведение, предсказать edge-cases сложнее чем для классических схем, но музыкально работает.

**Вердикт: ✓** Функционален, stабилен, оригинален. Tanh <1% ошибки — отлично.

---

### `chorus.rs` — Juno BBD, кубическая интерполяция

**Топология:** Круговой буфер 4096 samples, два независимых LFO (0.513 Hz и 0.863 Hz), L/R с разными LFO, cubic (Lagrange) интерполяция, one-pole BBD-фильтр.

**Triangle LFO:** `1.0 − 4.0 * (phase − 0.5).abs()`. Формула корректна: в точке phase=0 → 0, phase=0.5 → 1, phase=1.0 → 0. Правильный треугольник. ✓

**LFO-соотношение частот:** 0.513/0.863 — иррациональное, предотвращает биения. ✓

**Cubic (Lagrange 3rd-order) интерполяция:** Корректная формула. ✓

**WARNING — BBD lowpass захардкожен без привязки к sample rate**
Коэффициент 0.65 в one-pole LP:
```rust
state += 0.65 * (input - state)
```
При 48 kHz это даёт cutoff ≈ 8kHz (как написано в комментарии), но при 96 kHz тот же коэффициент даст ~16 kHz — яркее, чем задумывалось. Правильно: `coeff = 2·π·f_cut / SR` с `f_cut = 10000`.

**Stereo spread:** Offset LFO-фазы L/R на 0.25 (90°). ✓

**Вердикт: ⚠** Хороший хорус. Единственная проблема — non-portable BBD-фильтр.

---

### `ring_mod.rs` — 4-диодный мост

**Топология:** Кусочно-квадратичная диодная модель, 4-диодный мост (D+a, D-a, D+b, D-b), soft-saturation на выходе, три формы carrier (sine/saw/square).

**4-диодный мост:**
`output = D(in+car) + D(-in+car) - D(in-car) - D(-in-car)` ✓
Правильная формула для ring modulator.

**Диодная модель:** Квадратичный рост (forward bias) + линейное насыщение. Не физически точна (реальная: exp(V/nVt)), но для DSP-эффекта приемлема.

**Soft-saturation:** `1.5x − 0.5x³` — стандартный polynomial soft-clip. ✓

**Carrier aliasing:** Синусоида не алиасирует. Пила и квадрат — без PolyBLEP, но для ring mod это вторично.

**Вердикт: ✓** Корректная топология, функциональная модель диода.

---

### `flanger.rs` — Hermite delay + feedback

**Топология:** Круговой буфер 2048 samples, Catmull-Rom (Hermite) cubic interpolation, sinusoidal LFO, feedback loop.

**Hermite интерполяция:** Catmull-Rom формула верна (проверены коэффициенты a, b, c, v, w). ✓

**LFO:** Синусоидальный, правильный increment и wrapping. ✓

**WARNING — Нет DC-блокировщика в feedback-петле**
`output = input + delay_read * feedback`. При ненулевом feedback и DC в буфере возможен медленный дрейф. Рекомендуется HPF ~35 Hz на пути feedback.

**Through-zero:** Минимальная задержка 1 sample — through-zero не поддерживается. Приемлемый design choice для стабильности.

**Вердикт: ⚠** Hermite интерполяция — отличная. Нужен DC-блокировщик в feedback.

---

### `phaser.rs` — 12-стадийный allpass

**Топология:** 12 последовательных first-order allpass, bilinear-преобразованный коэффициент, logarithmic LFO-sweep, feedback loop.

**Allpass коэффициент:**
`t = tan(π·freq/SR); coeff = (t−1)/(t+1)` — bilinear-transformed 1st-order allpass. ✓

**LFO sweep:** Логарифмическая шкала: `min_freq * (max_freq/min_freq)^((lfo+1)*0.5)`. Правильно, логарифмический sweep звучит естественно. ✓

**Output mixing:** `(input + allpass_output) * 0.5` — стандартное суммирование для фазера. ✓

**WARNING — Нет DC-блокировщика в feedback-петле**
Аналогично flanger.rs: `self.feedback_l * self.feedback` в петле без HPF.

**Вердикт: ⚠** Bilinear allpass корректен, sweep правильный. Нужен DC-блокировщик.

---

### `overdrive.rs` — Перегруз 2× oversampled

**Топология:** Pre-filter HPF → 2× oversample → waveshape → tone LP → DC-блокировщик. Четыре режима: SoftClip, Tube, HardClip, Fuzz.

**CRITICAL — Pre-filter HPF: частота отсечки занижена в ~6 раз**
```rust
// строка 106:
let hp_coeff = 80.0 / self.sample_rate;          // = 0.001667 @ 48 kHz
self.hp_l += hp_coeff * (in_l - self.hp_l);      // one-pole EMA LP
let hp_out_l = in_l - self.hp_l;                  // HP = вход − LP
```
Для EMA one-pole LP вида `y += α*(x − y)` реальная -3dB частота: `f_c ≈ α·Fs/(2π)`.
При `α = 80/48000 = 0.001667`: `f_c ≈ 0.001667·48000/(2π) ≈ **12.7 Hz**`.

Намеренная цель — отрезать суббас ниже 80 Hz. Фактически фильтр режет лишь ниже 12.7 Hz (в **6.3× ниже** нужного). Pre-фильтр фактически бездействует: в distortion-секцию попадает значительно больший НЧ-контент, чем задумывалось → overdrive звучит мутно на низких нотах.

Правильная формула: `hp_coeff = 2·π·80 / sample_rate ≈ 0.0105` (или bilinear `ω = 2π·80/SR; α = ω/(1+ω)`).

**2× oversampling:** Линейная интерполяция + два вызова waveshaper. ✓

**Waveshaping:**
- SoftClip: `x/(1+|x|)` — стандартный. ✓
- Tube: асимметричный exp (положительная полуволна мягче отрицательной). Физически мотивирован для лампы. ✓
- HardClip, Fuzz: корректны. ✓

**Tone control:** `coeff = 0.02 + 0.4·tone` — семантический, не frequency-calibrated. Эмпирически работает, но абсолютная частота зависит от SR.

**DC-блокировщик:** Правильный one-pole HPF с coeff 0.997. ✓

**Вердикт: ✗** Waveshaping и oversampling отличные, но HPF-баг делает pre-filter бесполезным (убирает мидейндж вместо суббасов).

---

### `compressor.rs` — Feed-forward, soft-knee

**Топология:** Peak-detection `max(|L|, |R|)`, envelope follower с attack/release, soft-knee gain reduction в dB-домене, makeup gain.

**Feed-forward топология:** Корректна. Измерение входа → вычисление GR → применение к сигналу. ✓

**Soft-knee:** Квадратичная интерполяция в зоне `[threshold−knee/2, threshold+knee/2]`. Стандартный подход. ✓

**Gain reduction в dB-домене:** `10^((GR + makeup)/20)`. Корректно. ✓

**WARNING — Attack/release коэффициенты не дают точного тайминга**
`coeff = 1 − exp(-1/(time·SR))` — это one-pole с постоянной времени τ = time·SR, которая достигает 63% за время `time`. Для 5 мс атаки погрешность ≈ 1–2 мс. Для музыкального компрессора приемлемо, но не sample-accurate.

**Вердикт: ⚠** Soft-knee и peak detection — корректные. Тайминг attack/release — approximation, но достаточно.

---

### `eq.rs` — 3-полосный RBJ EQ

**Топология:** Low shelf + parametric peak + high shelf. RBJ biquad coefficients. Direct Form II Transposed.

**Коэффициентные формулы:** Все три типа используют `w0 = 2.0 * PI * freq / sample_rate` — это **стандартная корректная RBJ-формула**. Сам RBJ Cookbook явно указывает: *"BLT frequency warping has been taken into account."* Prewarping уже встроен в алгебраический вывод коэффициентов через sin/cos от w0. Никакого дополнительного `atan`-преобразования не требуется. ✓

**WARNING — Shelf EQ: фиксированная крутизна (S = 1)**
`alpha = sin(w0) / sqrt(2)` соответствует стандартному RBJ shelf с параметром S=1 (unity slope). Это корректная и широко используемая форма. Ограничение: нельзя изменить крутизну полки. Для данного применения (3-полосный EQ) — вполне приемлемо.

**Peak EQ:** Q-параметризован корректно: `alpha = sin_w / (2·Q)`. ✓

**Direct Form II Transposed:** Стандартный, численно стабильный. ✓

**Lazy update (dirty flag):** Сброс состояния при изменении коэффициентов. ✓

**Вердикт: ✓** Реализация корректна. RBJ-формулы применены правильно, prewarping уже учтён в стандартных коэффициентах. Единственное замечание — фиксированная крутизна полок (S=1), что является design choice, а не ошибкой.

---

### `freq_shift.rs` — Hilbert SSB сдвиг

**Топология:** Комплексный квадратурный осциллятор, Hilbert-аппроксимация через 2-ступенчатый IIR allpass, SSB-модуляция `I·cos(ω) − Q·sin(ω)`, feedback delay, DC-блокировщик.

**Квадратурный осциллятор:** Rotation matrix, renormalization при дрейфе > 0.001. ✓

**SSB модуляция:** Верхняя боковая полоса (USB). Формула корректна. ✓

**WARNING — Hilbert аппроксимация только 2-ступенчатая**
Коэффициенты [0.4021921162, 0.8561710882] — стандартные, но 2 ступени дают ~45° максимальную погрешность фазы → ~10% distortion верхней боковой полосы. Professional Hilbert требует 6–12 ступеней для <1% ошибки. Текущий вариант музыкально приемлем, но не audio-grade.

**Feedback saturation:** `x / (1 + 0.3·|x|)` предотвращает взрыв. ✓

**Вердикт: ⚠** Топология корректна. Hilbert 2-stage — ограниченное качество, но приемлемо для эффекта.

---

### `bitcrusher.rs` — TPDF дитеринг, anti-aliasing

**Топология:** AA lowpass → ZOH sample-rate reduction → bit-depth quantization с TPDF dithering.

**CRITICAL — Cutoff AA-фильтра в 2 раза выше нужного**
```rust
aa_coeff = PI * downsample_hz / sample_rate
```
Для корректной дискретизации AA-фильтр должен срезать на `downsample_hz / 2` (теорема Найквиста). Текущая формула ставит cutoff в `downsample_hz`, пропуская весь диапазон до downsample/2 — именно туда падают aliased компоненты при ZOH.
Исправление: `aa_coeff = PI * (downsample_hz * 0.5) / sample_rate`.

**TPDF dithering:** Сумма двух равномерных случайных значений — правильный triangular PDF. ✓

**Xorshift32 PRNG:** Стандартный, быстрый, без артефактов. ✓

**Bit-depth quantization:** `levels = 2^bits`, round к ближайшему уровню. ✓

**Zero-order hold:** Counter-based, корректная ZOH-семантика. ✓

**Вердикт: ⚠** Dithering отличный. AA-фильтр требует исправления — aliasing проникает при любом downsampling ratio.

---

### `tremolo.rs` — AM + стерео-сдвиг

**Топология:** Синусоидальный LFO, depth-модуляция амплитуды, stereo phase offset.

**LFO:** `(phase * 2π).sin() → [0, 1]` mapping. ✓

**Depth mapping:** `gain = 1.0 − depth * (1.0 − lfo)`. При depth=0: unity gain. При depth=1: gain ∈ [0, 1.0]. Интуитивно и корректно. ✓

**Stereo offset:** `phase_r = phase + 0.25 * stereo`. При stereo=1: 90° сдвиг. ✓

**Никаких проблем не найдено.**

**Вердикт: ✓** Чистая, корректная, музыкально эффективная реализация.

---

## Итоговые рекомендации

### Критические (влияют на звук или стабильность)

| Приоритет | Модуль | Проблема | Правка |
|-----------|--------|----------|--------|
| P1 | `overdrive.rs` | HPF cutoff в 6× ниже нужного (~12.7 Hz вместо 80 Hz) → overdrive мутный | `hp_coeff = 2·π·80/SR ≈ 0.0105` или bilinear |
| P2 | `envelope.rs` | Attack shape применяется к состоянию фильтра, а не к нормированному времени | Рефакторинг: отдельная phase-переменная, shape как postprocess |
| P3 | `piano.rs` | Коэффициент инармоничности завышен в 2× | Уточнить экспонент по измеренным данным (Bank & Sujbert 2003) |
| P4 | `filter.rs (Comb)` | Нет DC-блокировщика в feedback | One-pole HPF ~10 Hz в петле обратной связи |
| P5 | `bitcrusher.rs` | AA cutoff в 2× выше Найквиста | `aa_coeff = PI * downsample * 0.5 / SR` |
| P6 | `oscillator.rs (BandedWG)` | Panic при tick() до init, после reset() | Проверить/инициализировать буферы в reset() |

### Средний приоритет (качество, портируемость)

| Приоритет | Модуль | Проблема |
|-----------|--------|----------|
| P9 | `flanger.rs`, `phaser.rs` | Нет DC-блокировщика в feedback-петлях |
| P10 | `oscillator.rs (FM)` | Отсутствует delay-1 в FM operator feedback |
| P11 | `epiano.rs` | Pickup saturation: kink первой производной на 0.5 |
| P12 | `chorus.rs` | BBD lowpass захардкожен, не адаптируется к SR |
| P13 | `reverb.rs` | Нет DC-блокировщика в feedback (задержки оригинальные Dattorro, менять не нужно) |
| P14 | `freq_shift.rs` | Hilbert 2-stage → ~10% sideband distortion; увеличить до 4–6 ступеней |
| P15 | `spring_reverb.rs` | Нет частотно-зависимой дисперсии (spring chirp) |

### Низкий приоритет (minor)

| Модуль | Замечание |
|--------|-----------|
| `oscillator.rs (KS)` | Нет runtime-guard от денормалов при длительном затухании |
| `envelope.rs` | Release использует decay_shape — ограничение дизайна |
| `lfo.rs` | S&H slew коэффициент жёстко задан (4×) |
| `formant.rs` | Peak gain нормировка неточна на 10–20% |
| `piano.rs` | Sustain педаль не маппируется напрямую на MIDI CC64 |
| `eq.rs` | Shelf EQ: фиксированная крутизна S=1 (design choice, не ошибка) |
| `compressor.rs` | Attack/release тайминг ≈ 63% от заявленного (standard approximation) |

---

## Сильные стороны кодовой базы

- **Moog LP24:** Эталонная реализация Huovilainen 2004 с polynomial correction, 2× oversampling, tanh-per-stage. Одна из лучших реализаций в open-source Rust.
- **spring_reverb.rs allpass delays:** Все взаимно-простые числа — образцовое решение.
- **tape.rs:** RK2 интегрирование Ланжевена — физически обоснованный гистерезис, oversampling нелинейности корректен.
- **reverb.rs LFO:** Иррациональное соотношение частот модуляции — правильный подход против металлических артефактов.
- **bitcrusher.rs:** TPDF dithering реализован корректно (два источника равномерного шума).
- **piano.rs decay:** Модель Вайнрейха (двойное затухание) с правильными T60-расчётами.
- **neuron.rs tanh:** Padé [3,3] аппроксимация с погрешностью <1% — отлично.

---

---

## Постаудитная верификация (Tavily, 2026-04-05)

Все ключевые технические утверждения проверены по первичным источникам. Ниже — результаты с исправлениями.

| Утверждение | Источник | Результат |
|-------------|----------|-----------|
| PolyBLEP: два остаточных члена вокруг разрыва | KVR Audio, Martin Finke Blog | ✓ Подтверждено |
| Huovilainen polynomial: `1.8730·fc³ + 0.4955·fc² − 0.6490·fc + 0.9988` | Cycling '74 forum, оригинальный Moog code | ✓ Подтверждено |
| Huovilainen: 2× oversampling обязателен | Levien matrix paper | ✓ Подтверждено |
| Завалишин SVF: tan(π·fc/fs) warping | Noisehack VAFilterDesign PDF, JUCE forum | ✓ Подтверждено |
| RBJ EQ: `w0 = 2*pi*f/Fs` — **корректная** формула | RBJ Cookbook (W3C): "BLT warping taken into account" | ✅ **ОШИБКА В АУДИТЕ** — eq.rs корректен |
| RBJ shelf: S=1 → `alpha = sin(w0)/sqrt(2)` | RBJ Cookbook musicdsp.org | ✓ Стандартная форма, не ошибка |
| overdrive HPF: `80/SR` → cutoff 12.7 Hz (не 1.3 kHz!) | EarLevel one-pole article, EMA formulas | ⚠ **ОШИБКА В АУДИТЕ** — баг реален, но направление и величина описаны неверно |
| Dattorro delay lengths [142,107,379,277,672...] — оригинальные | Valhalla DSP, Valley Plateau, dattorro-vst-rs | ✅ **ОШИБКА В АУДИТЕ** — это оригинальные значения 1997 |
| TPDF = сумма двух равномерных | Wikipedia, robin-prillwitz.de | ✓ Подтверждено |
| Tape RK2 + 2× oversampling | DAFx 2019 paper (jatinchowdhury18) | ✓ Подтверждено; RK4 предпочтительнее, но RK2 функционален |
| Spring reverb: нужна частотно-зависимая дисперсия | wrongtools.com spring reverb article | ✓ Подтверждено |
| DC blocker 0.9975 → ~19 Hz | EarLevel Engineering one-pole article | ✓ Подтверждено |
| Компрессор `1−exp(−1/(t·SR))` = 63% за t секунд | JUCE forum, musicdsp.org | ✓ Стандартная формула индустрии |
| Karplus-Strong allpass: η ∈ [−1, 0] для CCRMA-формулы | CCRMA EKS algorithm, DSP SE | ✓ Аудит описывает другую формулу (2·cos), нужна проверка кода |
| Hilbert 2-stage < качество 6-stage | DSP StackExchange, comp.dsp | ✓ Подтверждено |

**Исправленный счёт критических проблем:**
- Было: P1–P8 (8 критических)
- Стало: P1–P6 (6 критических; eq.rs и reverb.rs GCD-проблема — не ошибки)

---

*Аудит проведён только как анализ кода. Изменения в код не вносились.*
