# Layers Followups: Perf Row Above Parts + Remove Duplicate Mode Toggle

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Убрать два UX-дефекта после Layers-redesign: панель Perf Save/Load переносится НАД строками партов; дубль-переключатель «Mode: Synth | SF2» в правой панели удаляется (движок теперь выбирается кнопкой DSP/SF2 в строке парта).

**Architecture:** Чистый GUI-рефакторинг. Задача 1 — вырезать блок «Performance save/load» (layers.rs:180-278) в отдельный метод `draw_perf_row` и вызвать его сразу после split-строки, до ScrollArea партов. Задача 2 — удалить горизонталь с «Mode:» (gui/mod.rs:1363-1380); `is_sf2` остаётся для статуса. Нулевые изменения в движке/событиях.

**Tech Stack:** egui 0.31, существующие `preset::save_performance`/`load_performance`.

---

## Test Cases (from the requirement)

Requirement (юзер, 2026-10-30): «Perf section should be ABOVE part rows»; «Mode: Synth SF2 — дублируется, движок уже в строке парта».

1. **TC1:** Perf-строка (name, Save, Reset, Load combo) отрисована ВЫШЕ ScrollArea партов, ниже split-строки — визуально первая строка после Split/табов.
2. **TC2:** Сохранение/загрузка перформанса работают из нового положения (код не менялся — только перенос).
3. **TC3:** Правая панель (looper/pitch-seq view) для SF2-парта НЕ показывает «Mode: Synth | SF2» — переключение движка только кнопкой DSP/SF2 в строке парта.
4. **TC4:** Правая панель для SF2-парта по-прежнему показывает статус-строку «SF2: … | prog» (не params editor); для DSP-парта — params editor как было.
5. **TC5:** Переключение DSP/SF2 из строки парта по-прежнему шлёт `AllNotesOff` + `SetPartSf2Mode` (layers.rs:314-317 — не тронут).

## Negative Matrix

- Perf-строка при выключенном Split — видна всегда (она не зависит от split_on). Покрыто TC1.
- Окно маленькое — Perf-строка одна horizontal, скролл не нужен. N/A (нет скроллируемого контента).
- Загрузка перформанса с пустым `perf_list` — существующий guard («No performances»), не меняется. N/A.
- Двойной клик Save подряд — существующее поведение (перезапись файла перформанса с тем же именем — это разрешённая перезапись **перформанса юзером по его воле**, не фабричного пресета). N/A.
- Конкурентный доступ — GUI однопоточен. N/A.
- `config.sf2.layer_a_sf2/layer_b_sf2` — эти поля сидируют `sf2_mode` частей 0/1 при старте (src/lib.rs:713-714, headless 422-423). Единственное место записи жил в удаляемом дубле (mod.rs:1375-1377, с багом маппинга `else → layer_b_sf2`). Удаление дубля БЕЗ замены убило бы персист движка — поэтому Task 2 Step 2 переносит запись в part-row toggle с правильным маппингом (part 0 → layer_a, part 1 → layer_b, остальные части без полей в конфиге — не персистятся, как и до-redesign для них).

---

### Task 1: Perf-строка над партами

**Files:**
- Modify: `src/gui/layers.rs` (draw_part_rows:28-279)

- [ ] **Step 1: Вырезать блок в метод.** Текущий блок — строки 180-278 layers.rs: строка 180 — комментарий `// ── Performance save/load ──`, 181 — `ui.add_space(theme::SP_XS);`, 182 — `ui.horizontal(|ui| {`, 278 — закрывающая `});`; дальше сразу закрывающая скобка `draw_part_rows` (279). Вырезать 180-278 целиком и оформить как новый метод:

```rust
    /// Performance save/load row (shown above part rows).
    fn draw_perf_row(&mut self, ui: &mut egui::Ui) {
        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            // сюда — строки 183-277 старого блока (внутренности horizontal) без изменений
        });
    }
```

Внимание: НЕ вставлять старую строку 182 (`ui.horizontal(|ui| {`) внутрь — иначе двойная вложенность. Ничего не переименовывать: `self.perf_name`, `self.perf_list`, `self.load_performance_by_name`, `preset::save_performance` и т.д.

- [ ] **Step 2: Вызвать после split-строки.** В `draw_part_rows` сразу после закрытия первой `ui.horizontal(...)` (split + view tabs, сейчас строка 97) вставить:

```rust
        // ── Performance save/load (above part rows) ──────────────────────
        self.draw_perf_row(ui);
```

- [ ] **Step 3: Проверить, что дубликата внизу нет.** Grep `Performance save/load` в layers.rs — ровно ДВА вхождения: doc-комментарий нового метода + комментарий call-site из Step 2. Третьего вхождения в теле `draw_part_rows` быть не должно.

- [ ] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 5: Commit**

```bash
git add src/gui/layers.rs
git commit -m "refactor: perf save/load row above part rows"
```

### Task 2: Удалить дубль Mode toggle

**Files:**
- Modify: `src/gui/mod.rs` (ветка `else` главного right-panel, строки 1355-1394)

- [ ] **Step 1: Удалить горизонталь с «Mode:».** Вырезать строки 1363-1380 (`// Synth / SF2 toggle` + весь `if self.sf2_keys_soundfont.is_some() { ui.horizontal(...) }`). Оставить `let is_sf2 = self.parts[part].sf2_mode;` (используется дальше) и последующий блок статуса/params editor без изменений:

```rust
                let part = self.active_part;
                let is_sf2 = self.parts[part].sf2_mode;
                if is_sf2 && self.sf2_keys_soundfont.is_some() {
                    // статус-строка + скрытый params editor — как было
```

- [ ] **Step 2: Перенести персист движка в part-row toggle.** Удалённый блок был единственным местом записи `config.sf2.layer_a_sf2/layer_b_sf2` — эти поля сидируют `sf2_mode` частей 0/1 при старте (src/lib.rs:713-714, headless 422-423). Без замены движок перестал бы переживать рестарт. В `src/gui/layers.rs`, в обработчике engine toggle (draw_part_row, сейчас строки 308-317), дополнить после `SetPartSf2Mode`:

```rust
                // Persist engine choice for parts 0/1 (seeds sf2_mode on startup)
                if i == 0 {
                    self.config.sf2.layer_a_sf2 = !sf2;
                    let _ = self.config.save();
                } else if i == 1 {
                    self.config.sf2.layer_b_sf2 = !sf2;
                    let _ = self.config.save();
                }
```

(Маппинг part 0 → layer_a, part 1 → layer_b — исправляет баг старого кода, где любая часть кроме 0 писала в layer_b.) Поля конфига не удалять — обратно-совместимость config.json.

- [ ] **Step 3: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 4: Commit**

```bash
git add src/gui/mod.rs src/gui/layers.rs
git commit -m "refactor: drop duplicate Mode Synth/SF2 toggle, persist engine in part row"
```

### Task 3: Финальная верификация

- [ ] **Step 1:** Полные гейты ещё раз после обоих коммитов.

- [ ] **Step 2: Ручной чеклист (юзер):**
1. Layers: Perf (name/Save/Reset/Load) — первая строка под Split, парты ниже
2. Save перформанса из нового места работает, Load поднимает сохранённое
3. Правая панель для SF2-парта: нет строки «Mode: Synth | SF2» (в params editor DSP-парта лейблы «Mode:» у осцилляторов/фильтров остаются — это другое, не трогаем)
4. Переключил движок кнопкой DSP/SF2 у парта A1/A2 → перезапустил приложение → движок восстановился (config.json `sf2.layer_a_sf2`/`layer_b_sf2`)
5. Для DSP-парта params editor как было
