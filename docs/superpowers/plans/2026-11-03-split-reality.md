# Split-Reality (TECHDEBT #51) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-03 (база = master ef70cd0; сниппеты вычитаны из кода; аудит #51 от 2026-11-03 — подтверждено ЖИВЫМ репортом юзера «перф Organ играет только до C3»: organ.json имеет split_point=60 и НОЛЬ включённых партов зоны B)

**Goal:** Сплит реален только при живой правой руке: нет включённых партов зоны
B — все парты играют 0..=127, независимо от сохранённого в перфе split_point.
Заголовки зон показывают честные диапазоны.

**Architecture:** Один чистый предикат `zone_range_pair(split_point, split_active)`
→ единственный источник диапазонов для движка (`apply_split_ranges`) и заголовков
GUI (layers.rs). Юзер-модель (подтверждена): «сплит включён = есть парты B;
выключен/нет B = только A, все ноты».

**Tech Stack:** Rust; pure-функция + юнит-тесты в layers.rs (интеграционный путь
SetPartRange→движок тривиален и покрыт существующими тестами root/range).

**Проверенные чтением кода факты (не пересматривать):**
- `apply_split_ranges` (src/gui/layers.rs:524-539): безусловно режет зону A
  до `sp-1`, зону B — `sp..127` для ВСЕХ партов, не проверяя наличие
  включённых B. ZONE_A/ZONE_B — массивы `[usize; 8]`.
- Вызов из `apply_performance` (src/gui/mod.rs:2886) безусловен; split_point
  fallback — 60 (mod.rs:2795-2802); перф сохраняет `split_point: Some(sp)`
  даже при выключенном сплите (layers.rs:228).
- Офф-ветка тумблера Split (layers.rs:48-58): Disables zone B + A на 0..127 —
  дублирует логику, которую унесёт хелпер (оставляем как есть — она про
  включённость партов, не про диапазоны; #51 не про неё).
- Заголовки: A — layers.rs:119-123 (`0..{sp-1}` / `0..127`), B — :146
  (`{sp}..127`), секция B видима `if split_on`; полуоткрытая нотация `..`
  читается двусмысленно.
- Движок-гейт: `raw < lo || raw > hi` (synth/mod.rs:1783) — диапазоны
  включающие по обоим краям.
- ZONE_A/ZONE_B — `std::ops::Range<usize>` (layers.rs:24-25), НЕ массивы.
  Паттерн предиката: `self.parts[ZONE_B].iter().any(|p| p.enabled)`
  (layers.rs:34 — так считается split_on). `for i in ZONE_A` — ок (Range =
  Iterator<Item=usize>).

---

## Test Cases (from the requirement)

Требование (юзер): «сплит включён — добавляются парты B; выключен/нет B —
только A играет все ноты». Живой кейс: perf Organ (8 партов A, split 60,
B пусто) должен играть все 128 нот.

- TC1: `zone_range_pair(60, true)` → `([0,59],[60,127])` — живой сплит.
- TC2: `zone_range_pair(60, false)` → `([0,127],[0,127])` — нет B = нет
  сплита (регрессия «Organ играет до C3»).
- TC3: `zone_range_pair(0, true)` → `([0,0],[0,127])` — вырожденный сплит 0
  (документируем известный overlap ноты 0).
- TC4: `zone_range_pair(127, true)` → `([0,126],[127,127])` — сплит на краю.
- TC5 (интеграция, чистая): `apply_split_ranges` применяет пару из хелпера
  (глию тонкая — проверяется чтением; юнит-пин — хелпер).

## Negative Matrix

- Юзер удалил последний B-парт через ×: split_on = производное
  (`self.parts[ZONE_B].iter().any(...)`, layers.rs:34) — в том же кадре
  становится false, секция B скрывается. ОДНАКО remove_part
  (layers.rs:501-519) НЕ зовёт apply_split_ranges — движок оставался бы
  порезанным до следующего триггера. Закрыто Step 2b: remove_part для
  B-парта зовёт apply_split_ranges (теперь безопасно — предикат сам
  разворачивает полный диапазон).
- Включение B-парта позже: `reset_and_push_part` + `apply_split_ranges`
  (layers.rs:80-82) — хелпер пересчитает с split_active=true автоматически.
- sp=0/127 из битого перфа — `saturating_sub` + TC3/TC4 (существующее поведение).
- split_point=None в перфе — существующий fallback 60 (mod.rs:2795) до
  предиката не касается: нет B → пары всё равно 0..=127.
- Дубли вызова apply_split_ranges (5 мест) — все получают предикат
  автоматически (правка в самой функции).
- Слайдеры min/max партов в строке (не существуют) — N/A, диапазоны не
  редактируются руками с #46.

---

### Task 1: `zone_range_pair` — чистая функция + тесты (TDD)

**Files:**
- Modify: `src/gui/layers.rs` (free fn рядом с apply_split_ranges; тест-модуль
  `#[cfg(test)] mod split_tests` в конце файла — создать, если нет)

- [ ] **Step 1: падающий тест**

```rust
#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn tc1_active_split_pair() {
        assert_eq!(zone_range_pair(60, true), ([0, 59], [60, 127]));
    }

    #[test]
    fn tc2_no_zone_b_means_no_split() {
        // Регрессия «Organ играет до C3»: перф с split_point=60 и без
        // включённых B-партов обязан давать полный диапазон
        assert_eq!(zone_range_pair(60, false), ([0, 127], [0, 127]));
    }

    #[test]
    fn tc3_split_at_zero() {
        assert_eq!(zone_range_pair(0, true), ([0, 0], [0, 127]));
    }

    #[test]
    fn tc4_split_at_top() {
        assert_eq!(zone_range_pair(127, true), ([0, 126], [127, 127]));
    }
}
```

- [ ] **Step 2: прогнать — красный**

Run: `cargo test --lib tc1_active 2>&1 | tail -4`
Expected: FAIL — `zone_range_pair` не существует.

- [ ] **Step 3: реализация** — в src/gui/layers.rs на уровне модуля (вне impl
  App, рядом с apply_split_ranges):

```rust
/// Честные диапазоны зон по РЕАЛЬНОСТИ сплита (#51): сплит активен только
/// при живой правой руке — иначе все парты 0..=127, как бы перф ни хранил
/// split_point. Единственный источник диапазонов для движка и заголовков.
#[allow(dead_code)] // снимается в Task 2
pub(super) fn zone_range_pair(split_point: u8, split_active: bool) -> ([u8; 2], [u8; 2]) {
    if split_active {
        ([0, split_point.saturating_sub(1)], [split_point, 127])
    } else {
        ([0, 127], [0, 127])
    }
}
```

- [ ] **Step 4: зелёный**

Run: `cargo test --lib split_tests 2>&1 | tail -4`
Expected: 4 passed. (На этом коммите функция не используется — добавь
`#[allow(dead_code)]` над ней и сними в Task 2, иначе коммит не проходит
clippy -D warnings.)

- [ ] **Step 5: коммит**

```bash
git add src/gui/layers.rs && git commit -m "feat: zone_range_pair — split reality predicate (#51, red-tested)"
```

### Task 2: apply_split_ranges и заголовки через предикат

**Files:**
- Modify: `src/gui/layers.rs:524-539` (apply_split_ranges), `:119-123` (заголовок
  A), `:146` (заголовок B)

- [ ] **Step 1: apply_split_ranges** — заменить тело:

```rust
    pub(super) fn apply_split_ranges(&mut self) {
        // TECHDEBT #51: сплит реален только при живой правой руке. Перф
        // хранит split_point даже при выключенном сплите и безусловный
        // сплит резал зону A до sp-1 («Organ играет до C3» при выключенном
        // сплите; Reset и рестарт — те же грабли). Юзер-модель: нет B —
        // нет сплита.
        let split_active = self.parts[ZONE_B].iter().any(|p| p.enabled);
        let (a, b) = zone_range_pair(self.split_point, split_active);
        for i in ZONE_A {
            self.parts[i].min_note = a[0];
            self.parts[i].max_note = a[1];
            self.send_part_range(i);
        }
        for i in ZONE_B {
            self.parts[i].min_note = b[0];
            self.parts[i].max_note = b[1];
            self.send_part_range(i);
        }
    }
```

- [ ] **Step 2: заголовок A** — заменить блок :119-123:

```rust
            let split_active = self.parts[ZONE_B].iter().any(|p| p.enabled);
            let (a_r, _) = zone_range_pair(self.split_point, split_active);
            let a_header = format!("◀ Left hand (A) — {}..={}", a_r[0], a_r[1]);
```

(Старый `if split_on {...} else {...}` удаляется: заголовок всегда честный по
реальности движка; включающая нотация `..=` вместо двусмысленной `..`.)

- [ ] **Step 2b: remove_part перестаёт оставлять порезанный движок** — в
  `remove_part` (layers.rs:501-519), после сброса пушей удалённого B-парта
  (ветка `if part >= PARTS_PER_ZONE` / аналог — реализатор находит ветку
  удаления парта ЗОНЫ B), добавить:

```rust
        // #51: удаление последнего B-парта = сплит исчез; предикат внутри
        // сам развернёт зону A в 0..=127 (раньше движок оставался
        // порезанным до следующего триггера apply_split_ranges)
        self.apply_split_ranges();
```

Точное место: в remove_part (layers.rs:501-519) после push-очистки слота
(`if part >= PARTS_PER_ZONE` — условие зоны B уже там); вызывать только для
B-парта (для A диапазоны не меняются; безусловный вызов тоже безопасен —
просто 16 лишних send_part_range).

- [ ] **Step 3: заголовок B** — в секции B (`if split_on {`) заменить
  формат заголовка:

```rust
                    ui.label(egui::RichText::new(format!(
                        "▶ Right hand (B) — {}..=127", self.split_point)).weak().small());
```

на

```rust
                    let (_, b_r) = zone_range_pair(self.split_point, split_active);
                    ui.label(egui::RichText::new(format!(
                        "▶ Right hand (B) — {}..={}", b_r[0], b_r[1])).weak().small());
```

(split_on — производное от наличия включённых B (layers.rs:34), поэтому
пустая B при split_on невозможна: секция B скрыта вместе с удалением
последнего B-парта. Заголовок всегда sp..=127 при показе.)

- [ ] **Step 3b: снять `#[allow(dead_code)]` с zone_range_pair** (Task 1).

- [ ] **Step 4: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`
Expected: clippy чист, все result-строки ok, FAILED/panicked нет.

- [ ] **Step 5: коммит**

```bash
git add src/gui/layers.rs && git commit -m "fix: split ranges honor reality — no enabled zone-B part means full keyboard (#51, perf Organ played only below C3/C4)"
```

### Task 3: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): загрузить perf Organ → играть выше C4 — ЗВУК ЕСТЬ;
  заголовок «Left hand (A) — 0..=127»; включить Split → появился B-парт,
  резка по сплиту живёт; выключить → снова все ноты
- [ ] TECHDEBT #51: пометить ГОТОВО (зафиксировав живое подтверждение)

## Out of scope

- Офф-ветка тумблера Split (layers.rs:48-58) — дублирующий код, но корректный;
  унификация — рефактор вне бага.
- Слайдеры per-part диапазонов и подсветка границы на клавиатуре — UX-фича,
  не баг (отдельно при желании).
