# UI/UX Аудит: Virtual Synthesizers Research

Дата: 2026-04-14

---

## Источники исследования

Vital, Surge XT, Phase Plant (Kilohearts), Serum, Arturia Pigments, u-he Zebra2, Bitwig Studio, Ableton Live Instrument Rack, Roland Zenology/System-8, KORG Gadget.

---

## Ключевые паттерны индустрии

### Паттерн 1: Range-bar на линейке для зон (Ableton — золотой стандарт)

Key range и velocity range показываются как горизонтальная закрашенная полоса на пианоруллере (0-127). Края полосы перетаскиваются. Fade zones — скошенные края. Несколько Parts — стопка полос на одной линейке.

**НЕ делать:** два числовых слайдера `Lo C2 ... Hi G8`. Пространственный параметр требует пространственного представления.

### Паттерн 2: Modulation dot/ring на destination (Serum, Vital, Pigments)

Каждый модулируемый параметр показывает цветной индикатор (кольцо, точка, дуга). Цвет = конкретный LFO/env. Размер/угол = глубина. Несколько источников → стопка индикаторов. Без этого пользователь не может читать состояние патча с первого взгляда.

### Паттерн 3: Always-visible modulation bar (Pigments)

Горизонтальная полоса на всю ширину, показывающая ВСЕ модуляторы активного Part одновременно — всегда видна, не прячется за таб. Клик на модулятор → режим назначения. Это ключевой паттерн Pigments.

### Паттерн 4: Blue overlay on slider при routing mode (Surge XT)

При выборе источника модуляции — поверх каждого назначаемого слайдера появляется цветной overlay-слайдер глубины. Видно и base value и modulation depth одновременно.

### Паттерн 5: Active-only modules (Zebra2, Phase Plant)

Никогда не показывать пустые слоты для неактивных Part/Layer/Module. Только активные + кнопка "Add" после последнего. Пустые слоты создают ложное ощущение сложности.

### Паттерн 6: Цвет = функциональная идентичность (Pigments, Vital, Bitwig)

Цвет присваивается сущности (LFO, Part, Voice) один раз и появляется ВЕЗДЕ где эта сущность фигурирует: в своём виджете, во всех кольцах на destination knobs, в строке матрицы. Цвет никогда не декоративный — всегда несёт информацию.

### Паттерн 7: Play View / Synth View duality (Pigments)

Два режима интерфейса: упрощённый (перформанс — только essential controls) и полный (sound design). Разные цели пользователя — не должны компрометировать друг друга.

### Паттерн 8: State в заголовке таба (Roland, Serum)

Таб показывает не только имя, но: LED enabled/muted, мини level meter, иконку типа синтеза. Таб-бар читается без переключения.

### Паттерн 9: Routing strip — collapsible (Pigments, Roland)

Routing (vol/pan/key range/vel range) сворачивается в одну компактную строку при работе с синт-параметрами. Раскрывается по клику `▾`. Экономит critical vertical space.

### Паттерн 10: Homogeneous building blocks (Phase Plant)

Один паттерн взаимодействия для всего: добавить/переместить/удалить. Никаких modal differences между oscillator/filter/envelope/effect. Снижает learning curve даже для сложных интерфейсов.

### Паттерн 11: Preset browser — persistent sidebar (universal)

Правая боковая панель с текстовым поиском, деревом категорий, preview по клику, стрелки prev/next в хедере. Никогда отдельное окно.

---

## Что не делать

1. **Modal interfaces** — редактирование key range в отдельном окне
2. **Uniform empty slots** — 8 Part-табов из которых 6 пустых
3. **Text-only spatial concepts** — `C2–G5` вместо piano bar
4. **Unindicated modulation destinations** — knob под модуляцией без визуального маркера
5. **Inconsistent interaction paradigms** — разные паттерны для похожих операций
6. **Decorative color** — цвет без семантики
7. **Non-collapsible routing strips** — всегда-видимые 3 строки routing

---

## Оценка текущего UI mini_midi_synth

| Элемент | Best practice | Текущий UI | Оценка |
|---------|--------------|------------|--------|
| Part tabs | Цветные + имя пресета + LED + мини-волна | `[1]` только цифра | ❌ |
| Routing strip | Collapsible в 1 строку | Всегда 3 строки | ❌ |
| Key range | Piano bar с перетаскиваемой зоной | Два слайдера с цифрами | ⚠️ |
| Vel range | Аналогично | Два слайдера | ⚠️ |
| Active-only parts | Только активные видны prominently | Не реализовано | ❌ |
| Color per part | Везде propagates | Только в minimap | ⚠️ |
| State in tab | LED + мини-инфо | Нет | ❌ |
| Modulation indicators | Dot/ring на knob | Нет | ❌ |
| Modulation bar | Always-visible strip | Нет | ❌ |
| Preset browser | Persistent sidebar + search | ✅ | ✅ |
| Global controls | Header bar | ✅ | ✅ |

---

## Приоритетный план улучшений

### P1 — Routing strip collapsible (критично)

```
[■ Pipe Organ] [■ SF2 Piano 🔇] [+ Part]    Drums | MIDI Seq | FX Chain
Part 1  [Mute] Vol ──●──  [C₋₂████████G₈]  [▾]
════════════════════════════════════════════════
[synth parameters — полная высота]
```

`▾` раскрывает полный editor: Pan/Tr/Key/Vel/перформ.

### P2 — Miniature piano zone widget

80px piano bar с цветной полосой вместо двух слайдеров. Края drag-to-resize.

### P3 — Color per Part + state в табах

```
[■ 1: Pipe Organ] [■ 2: Piano 🔇] [+ Part]
```
`■` = цветной квадрат (8 цветов), `🔇` = muted LED.

### P4 — Modulation dot на knob (долгосрочно)

Маленькая цветная точка под каждым knob под модуляцией. Цвет = LFO/env источник.

### P5 — Always-visible modulation strip (долгосрочно)

Горизонтальная полоса между routing strip и синт-параметрами — все активные LFO/ENV видны всегда.

---

## Референсы

- [Vital Audio](https://vital.audio/)
- [Surge XT Manual](https://surge-synthesizer.github.io/manual-xt/)
- [Phase Plant Docs](https://kilohearts.com/docs/phase_plant)
- [Arturia Pigments](https://www.arturia.com/products/software-instruments/pigments/overview)
- [Ableton Rack Manual](https://www.ableton.com/en/manual/instrument-drum-and-effect-racks/)
- [Bitwig Modulators](https://www.bitwig.com/learnings/an-introduction-to-modulators-45/)
- [Basic UX Patterns for Synth Plugins — Voger Design](https://vogerdesign.com/blog/basic-ux-patterns-in-the-development-of-synthesizer-plugins-awesome/)
