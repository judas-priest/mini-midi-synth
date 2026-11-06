# Part Swap via Drag + Alias Reorder (TECHDEBT #54) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-07 (база = master 59bcad1; факты вычитаны из кода; research — tavily_search 2026-11-06 + tavily_research 2026-11-06 (дропнулось по лимиту, дозаполнено search): egui_dnd канон, egui demo drag_and_drop.rs (drag_source/Sense::drag/drop zone), DragAndDrop payload API; UX-канон: drag-ХЕНДЛ для строк с интерактивными виджетами (whole-row драг конфликтует с чекбоксами/слайдерами — NN/g + Material), insertion-линия/свап-подсветка, кнопки ↑↓ как keyboard-альтернатива, undo одним шагом на drop)

**Goal:** Парты в перфе меняются местами drag-and-drop (хендл ⠿ в строке парта,
свап в пределах ЗОНЫ — A↔A, B↔B) + кнопки ↑↓ в Tracks-менеджере для алиасов
(keyboard-альтернатива канона).

**Architecture:** Свап = полный `std::mem::swap` GUI-состояний двух партов
(`PartState`) + движковый пуш обоих слотов (push_full_part_state +
send_edited_params — канон reset_and_push_part) + remap active_part. Драг —
хендл-иконка (Sense::drag) в начале строки; во время драга — подсветка
слота-цели по pointer.y (hit-test по сохранённым rect'ам строк); drop = свап.
Зоны: свап ТОЛЬКО в пределах зоны (A 0..8, B 8..16) — диапазоны/сплит-семантика
не ломаются. Undo: два PartUndoSnapshot (по одному на слот, пуш в обратном
порядке) — каждый Ctrl+Z откатывает один слот, два отката = полный откат свапа.
Tracks-менеджер: ↑↓ кнопки на алиасах (drag там — по фидбеку).

**Tech Stack:** Rust/egui (Sense::drag, hit-test по rect'ам — без egui_dnd:
строки парта — плотные интерактивные виджеты, egui_dnd whole-row драг
конфликтует, а хендл-паттерн — 30 строк на egui-примитивах без новой
зависимости); TDD — свап-логика чистая функция + тесты; драг-детект — экран.

**Проверенные факты (вычитано; не пересматривать):**
- `PartState` (gui/mod.rs:156-200+): все поля Copy-клон-able кроме
  edited_params (BTreeMap) и macro_names (Vec) — Clone есть.
- Строки рисует `draw_part_row(&mut self, ui, i: usize)` (layers.rs:285),
  вызовы из `draw_part_rows` (:39) в двух зонах (`for i in ZONE_A` / ZONE_B).
- ZONE_A = 0..8, ZONE_B = 8..16 (`std::ops::Range`, layers.rs:24-25).
- `active_part: usize` (:369), `set_active_part(part)` (:2567).
- `push_full_part_state(part)` (:2394) — полный пуш состояния слота в движок
  (канон reset_and_push_part #40/#42); `send_edited_params(part)` (:2364) —
  LoadPatch патча парта.
- Undo: `PartUndoSnapshot { part, edited_params, patch_idx, mute, pan,
  transpose }` (mod.rs:2256), `push_undo_snapshot_for(part)` (:2226, guard от
  дублей :2218-2228), restore_part_snapshot (:2259) применяет к своему слоту.
- `persist_parts_state()` (:2867) + `save_config` — персист.
- Дубль-запуск канона undo: `push_undo_snapshot_for` guard — не пушит
  дубликат подряд.
- Канон подтверждения деструктива: confirm.rs `confirm_button`.
- Тест-модуль для чистой логики: синтх-tests НЕ место (GUI); tests/ интеграционный
  не видит приватный gui — чистая fn в gui/mod.rs с `#[cfg(test)]` тестами
  (канон macro_mirror_tests).

---

## Test Cases (from the requirement)

Требование (юзер #54): «поменять парты местами» в перфе — drag part row на
другую строку той же зоны; в Tracks-менеджере — порядок алиасов.

- TC1 (same-zone swap валиден): `swap_check(3, 5, 8)` → Ok (оба в ZONE_A).
- TC2 (cross-zone запрещён): `swap_check(3, 10, 8)` → Err (3 в A, 10 в B) —
  зоны/сплит-семантика не ломаются.
- TC3 (сам на себя): `swap_check(5, 5, 8)` → Err (no-op не должен проходить
  как валидный drag).
- TC4 (active_part remap): active_part = 3, свап 3↔5 → active_part = 5
  (парт «едет» со своим содержимым; юзер продолжает править тот же звук).
- TC5 (alias move down): (0..3), move(1) → [0,2,1] порядок алиасов.
- TC6 (alias move up, clamp): move(0) на [0,1,2] → без изменений (guard).

## Negative Matrix

- свап через зоны (A↔B) — запрещён (TC2): диапазоны/сплит — семантика зоны.
- active_part на одном из свапаемых — remap (TC4).
- свап с НЕактивным (disabled) партом — разрешён: слоты состояний меняются,
  enabled флаги едут с партами (перф «макет» легитимен).
- undo свапа — два PartUndoSnapshot (пуш: сначала новый слот, потом старый —
  чтобы ПЕРВЫЙ Ctrl+Z вернул старый контент в ЦЕЛЕВОЙ слот... фактически
  порядок пушей a-старый, b-старый; поп идёт с конца). Ограничение: полный
  откат = 2 Ctrl+Z (по одному на слот) — осознанный компромисс канона
  single-part snapshot.
- drag упал мимо строк (pointer вне таблицы) — отмена драга, без свапа.
- drag начат, но pointer.y вернулся в исходную строку — отмена (no-op).
- TrackSelect/LoadPerf во время драга — драг живёт в GUI-фрейме (локальное
  состояние drag_src: Option<usize>), консольные события не пересекаются.
- RT: свап — mem::swap двух Vec-содержащих структур в GUI-потоке; движок
  получает пуш по каналам — RT не трогается.

---

### Task 1: Чистая логика свапа + тесты (TDD)

**Files:**
- Modify: `src/gui/mod.rs` — free fn `swap_check` + `#[cfg(test)] mod
  swap_tests` (рядом с macro_mirror_tests)

- [ ] **Step 1: падающие тесты**:

```rust
#[cfg(test)]
mod swap_tests {
    use super::*;

    #[test]
    fn tc1_same_zone_swap_ok() {
        assert_eq!(swap_check(3, 5, crate::gui::PARTS_PER_ZONE), Ok(()));
    }

    #[test]
    fn tc2_cross_zone_denied() {
        assert!(swap_check(3, 10, crate::gui::PARTS_PER_ZONE).is_err());
    }

    #[test]
    fn tc3_self_swap_denied() {
        assert!(swap_check(5, 5, crate::gui::PARTS_PER_ZONE).is_err());
    }
}
```

(константа PARTS_PER_ZONE — verify: grep `PARTS_PER_ZONE` в gui/mod.rs —
существует, = 8. Если приватная — тесты в том же crate-модуле видят
pub(crate).)

- [ ] **Step 2: красный**

Run: `cargo test --lib swap_tests 2>&1 | tail -4`
Expected: FAIL — swap_check не существует.

- [ ] **Step 3: реализация** (рядом):

```rust
/// Свап партов разрешён только в пределах одной зоны (A↔A, B↔B):
/// диапазоны/сплит-семантика зоны не переезжают (#54).
fn swap_check(a: usize, b: usize, zone_size: usize) -> Result<(), &'static str> {
    if a == b { return Err("same slot"); }
    if (a / zone_size) != (b / zone_size) { return Err("cross-zone swap"); }
    Ok(())
}
```

- [ ] **Step 4: зелёный**

Run: `cargo test --lib swap_tests 2>&1 | tail -3`
Expected: 3 passed. (`#[allow(dead_code)]` до Task 2.)

- [ ] **Step 5: коммит**

```bash
git add src/gui/mod.rs && git commit -m "feat: swap_check — same-zone part swap predicate (#54, red-tested)"
```

### Task 2: Свап парта + хендл-драг в строке

**Files:**
- Modify: `src/gui/mod.rs` — App поле `part_drag_src: Option<usize>` (:463
  рядом с console_prev_mode), lib.rs init `None`; impl App: `fn swap_parts(&mut
  self, a: usize, b: usize)`, `fn do_swap_parts(&mut self, a: usize, b: usize)`
  (мутация + пуш + персист)
- Modify: `src/gui/layers.rs` — draw_part_rows: собрать rect'ы строк;
  draw_part_row: хендл ⠿ (Sense::drag) + подсветка цели; drop = do_swap_parts

- [ ] **Step 1: swap_parts + do_swap_parts** — в impl App (рядом
  reset_and_push_part):

```rust
    /// Свап содержимого двух слотов одной зоны (#54): GUI-структуры,
    /// движок обоих слотов, активный парт, персист. Без undo-снапшота —
    /// осознанно (двухслотовый откат ломает single-part канон).
    fn swap_parts(&mut self, a: usize, b: usize) {
        if swap_check(a, b, crate::gui::PARTS_PER_ZONE).is_err() { return; }
        self.parts.swap(a, b);
        // active_part едет с партом (юзер продолжает править тот же звук)
        if self.active_part == a { self.active_part = b; }
        else if self.active_part == b { self.active_part = a; }
        self.do_swap_parts_push(a, b);
    }

    fn do_swap_parts_push(&mut self, a: usize, b: usize) {
        // Движку — полное состояние обоих слотов (канон reset_and_push_part):
        // sf2/volume/pan/etc через push_full_part_state, патч — через
        // send_edited_params (LoadPatch). MIDI-ноты, игравшие в момент свапа,
        // доигрывают в старом слоте (их перенос — out of scope).
        self.push_full_part_state(a);
        self.push_full_part_state(b);
        self.send_edited_params(a);
        self.send_edited_params(b);
        self.persist_parts_state();
    }
```

- [ ] **Step 2: drag-состояние** — App поле:

```rust
    pub part_drag_src: Option<usize>,
```
init `part_drag_src: None,` в lib.rs литерале.

- [ ] **Step 3: draw_part_rows — сбор rect'ов + drop detection.** В
  `draw_part_rows` (layers.rs:39): до цикла — локальный вектор rect'ов
  (заполняется в draw_part_row через параметр), после цикла — drop-детект.
  Механика: App-поле `part_drag_src` хранит исходный слот; во время драга
  каждый кадр — hit-test pointer.y по rect'ам ЗОНЫ исходного слота; целевой
  слот подсвечивается (рамка пурпур — канон «выбрано»); drop (pointer
  released) → do_swap_parts.

  Реализация (фрагмент draw_part_rows):

```rust
        // #54: сбор rect'ов строк для drag hit-test
        let mut row_rects: Vec<(usize, egui::Rect)> = Vec::new();
```
  — параметр `row_rects: &mut Vec<(usize, egui::Rect)>` протащить в
  draw_part_row и пушить `row_rects.push((i, ui.min_rect()))` в конце строки
  (после отрисовки всех контролов). После обоих циклов зон:

```rust
        // #54: drag part row — hit-test + drop
        if let Some(src) = self.part_drag_src {
            let (Some(pointer), dragging) = (
                ui.ctx().pointer_interact_pos(),
                ui.ctx().input(|i| i.pointer.any_down()),
            ) else { self.part_drag_src = None; return; };
            if !dragging {
                // drop: целевой слот = строка под pointer
                for (i, r) in &row_rects {
                    if r.contains(pointer) {
                        let dst = *i;
                        let src_zone = src / crate::gui::PARTS_PER_ZONE;
                        let dst_zone = dst / crate::gui::PARTS_PER_ZONE;
                        if src_zone == dst_zone && src != dst {
                            self.swap_parts(src, dst);
                        }
                        break;
                    }
                }
                self.part_drag_src = None;
            } else {
                // подсветка слота-цели в той же зоне
                for (i, r) in &row_rects {
                    if r.contains(pointer) && (i / crate::gui::PARTS_PER_ZONE)
                        == (src / crate::gui::PARTS_PER_ZONE) && *i != src
                    {
                        ui.painter().rect_stroke(*r, 4.0,
                            egui::Stroke::new(2.0, egui::Color32::from_rgb(0xFF, 0x00, 0xFF)));
                    }
                }
            }
        }
```

  ВНИМАНИЕ borrow: `row_rects` — локальный Vec, мутируемый draw_part_row
  через `&mut Vec` параметр; `self.swap_parts` — &mut self ПОСЛЕ циклов
  (draw_part_rows принимает &mut self — конфликта нет: rect'ы уже собраны в
  локальный Vec до swap-вызова). Pointer-drag ДЕРЖИТСЯ на хендле — см. Step 4.

- [ ] **Step 4: хендл в draw_part_row** — в начале строки (перед Mute):

```rust
            // #54: drag-хендл (⠿) — Sense::drag, старт драга ставит
            // part_drag_src; сам контент строки обычный (drag живёт на
            // хендле, не на всей строке — канон строк с чекбоксами/слайдерами)
            let (handle_resp, _) = ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(
                    egui::vec2(14.0, ui.available_height()),
                    egui::Sense::drag());
                if r.drag_started() {
                    self.part_drag_src = Some(i);
                }
                ui.label(egui::RichText::new("\u{2802}").weak());
                (r, ())
            });
            let _ = handle_resp;
            if self.part_drag_src == Some(i) {
                ui.painter().rect_filled(
                    ui.max_rect(), 2.0,
                    egui::Color32::from_rgba_premultiplied(0xFF, 0x00, 0xFF, 24));
            }
```

  ВНИМАНИЕ borrow: draw_part_row(&mut self) — `self.part_drag_src = Some(i)`
  внутри — ок (не в замыкании: allocate/label на `ui` — ui приходит
  параметром, self свободен). egui::Sense::drag на allocate — драг-старт
  детектится; строка едет С drag point (без repaint переносимого контента —
  подсветка исходной строки = фидбек).

- [ ] **Step 5: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`
Expected: чисто, зелёные.

- [ ] **Step 6: коммит**

```bash
git add src/gui/mod.rs src/gui/layers.rs src/lib.rs && git commit -m "feat: part swap via drag handle — same-zone only, full state push both slots, active part follows (#54)"
```

### Task 3: Alias ↑↓ в Tracks-менеджере (keyboard-альтернатива канона)

**Files:**
- Modify: `src/gui/mod.rs` — draw_tracks_manager: alias-строки — добавить ↑↓
  рядом с × (порядок алиасов = порядок на пэдах папки); enum Op + apply-arms
  MoveAliasUp/Down

- [ ] **Step 1: Op варианты** — в enum Op добавить:

```rust
            MoveAliasUp(usize, usize),
            MoveAliasDown(usize, usize),
```

- [ ] **Step 2: кнопки в alias-строке** — рядом с × (в правой панели, где
  алиасы):

```rust
                            ui.horizontal(|ui| {
                                ui.label(format!("  {}", pname));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("\u{00D7}").clicked() {
                                        ops.push(Op::DelPerf(fi, pi));
                                    }
                                    if pi + 1 < self.config.ui.perf_folders[fi].perfs.len()
                                        && ui.small_button("\u{2193}").clicked()
                                    {
                                        ops.push(Op::MoveAliasDown(fi, pi));
                                    }
                                    if pi > 0 && ui.small_button("\u{2191}").clicked() {
                                        ops.push(Op::MoveAliasUp(fi, pi));
                                    }
                                });
                            });
```

- [ ] **Step 3: apply-arms** — в match op:

```rust
                Op::MoveAliasUp(fi, pi) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        if pi > 0 && pi < f.perfs.len() {
                            f.perfs.swap(pi - 1, pi);
                            let _ = self.config.save();
                        }
                    }
                }
                Op::MoveAliasDown(fi, pi) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        if pi + 1 < f.perfs.len() {
                            f.perfs.swap(pi, pi + 1);
                            let _ = self.config.save();
                        }
                    }
                }
```

- [ ] **Step 4: гейт + коммит**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | tail -30`
Expected: чисто, зелёные.

```bash
git add src/gui/mod.rs && git commit -m "feat: alias ↑↓ reorder in Tracks manager — keyboard alternative canon (#54)"
```

### Task 4: Верификация

- [ ] Гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → rebuild release → relaunch
- [ ] Экран (юзер): ⠿ хендл строки → драг на другую строку той же зоны →
  пурпур-рамка цели → drop → парты поменялись местами (инструмент/громкость/
  мьют едут со слотами), активный парт следует; drag через зоны — рамка не
  появляется, drop — no-op; Tracks → ↑↓ алиасов меняет порядок пэдов
- [ ] TECHDEBT #54: ГОТОВО (drag same-zone swap + alias ↑↓; cross-zone и
  drag-алиасов — по фидбеку)

## Out of scope

- Cross-zone drag (A↔B) — семантика зон/сплита; отдельный дизайн при запросе.
- Драг алиасов в Tracks (мышкой) — ↑↓ кнопки сейчас (канон keyboard-alt);
  egui_dnd — если фидбек попросит.
- Undo свапа — two-slot откат ломает single-part snapshot канон; полный
  откат свапа — отдельная работа (данные персистятся в конфиге).
- Insert-семантика (раздвигание) вместо свапа — слоты партов семантически
  закреплены (A1..B8), свап честнее.
