# Instrument Browser in Part Row + Remove Presets Side Panel

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Убрать правую панель пресетов; выбор инструмента — кнопка в строке парта, открывающая модал с поиском и категориями (Vital-style). Init/Rnd/Undo/Redo переезжают в шапку params-редактора. Вариант 1, согласованный с юзером (2026-10-30).

**Architecture:** Рендер списка (поиск + дерево категорий DSP / GM-категории SF2) переносится из `SidePanel::right("preset_panel")` (gui/mod.rs:1051-1289) в модал `egui::Modal` (паттерн уже есть в layers.rs — диалог remove_part). Кнопки Init/Rnd/Undo/Redo (mod.rs:1092-1112) переезжают в else-ветку правой панели (params editor, mod.rs ~1355-1390). Персист `layer_a/b_program` (mod.rs:1198-1200) сохраняется внутри модала — это единственное место записи. Нулевые изменения в движке.

**Tech Stack:** egui 0.31 (`egui::Modal`, `egui::ScrollArea`, `TextEdit`), существующие поля App: `preset_search`, `collapsed_categories`.

---

## Test Cases (from the requirement)

Requirement (юзер): «правую панель с пресетами убрать, раз выбор в строке парта; вариант 1 — кнопка в строке открывает поиск+категории, Init/Rnd/Undo/Redo — в params editor».

1. **TC1:** Правой панели пресетов больше нет — парты занимают всю ширину окна.
2. **TC2:** Кнопка инструмента в строке DSP-парта показывает имя текущего патча; клик открывает модал: поле поиска сверху, ниже категории (сворачиваемые), клик по патчу применяет его к ЭТОМУ парту и закрывает модал.
3. **TC3:** То же для SF2-парта: кнопка «prog N: Name», модал с 16 GM-категориями и поиском.
4. **TC4:** Поиск фильтрует по имени патча/инструмента И по категории (логика перенесена без изменений).
5. **TC5:** Выбор SF2-инструмента в модале персистит `config.sf2.layer_a_program` (part 0) / `layer_b_program` (part 1) — иначе потеря программы при рестарте для партов 0/1 (критично: модал становится единственным местом записи, mod.rs:1198-1200 удаляется вместе с панелью).
6. **TC6:** Выбор DSP-патча в модале: AllNotesOff + patch_idx + load_edited_params + send_edited_params + save_config — как было в панели (mod.rs:1281-1287).
7. **TC7:** Init/Rnd/Undo/Redo — в шапке params-редактора для DSP-парта; Init сбрасывает параметры активной части с undo-снапшотом, Rnd рандомизирует, кнопки ↶/↷ работают и дизейблятся при пустых стеках; Ctrl+Z не сломан.
8. **TC8:** Для SF2-парта params-редактор по-прежнему скрыт (статус-строка), Init/Rnd не показываются.
9. **TC9:** Старое состояние категорий (`collapsed_categories`) и поиск (`preset_search`) работают в модале.

## Negative Matrix

- Открыли модал для парта A1, потом кликнули строку A2 (модал открыт, модалка поверх) — фон кликабелен? Нет: `egui::Modal` блокирует ввод под собой (owning layer), клик вне модала = `should_close()` → закрытие без выбора. Покрыто реализацией Modal.
- Модал открыт, часть сменила движок извне (LED-консоль/миди/хоткей) — pointer-ввод модал блокирует, но MIDI/hotkeys живут; `is_sf2`/`current_program` пересчитываются каждый кадр внутри замыкания, модал самокорректируется. N/A.
- Пустой поиск → полный список. `preset_search` сбрасывается при каждом открытии модала (Step 4: `self.preset_search.clear()` в обоих обработчиках кнопок) — устаревший фильтр не переносится между открытиями.
- part без SF2 soundfont (sf2_mode вкл, шрифт не загружен) → кнопка-заглушка «---» без открытия модала (существующее поведение layers.rs:350-353 сохраняется).
- Двойной клик по кнопке инструмента → повторный `inst_browser_part = Some(i)` — безвредно (тот же модал, поиск уже сброшен только при переходе None→Some).
- Коллапс категории в модале → `save_collapsed_categories()` как было (state общий с удалённой панелью, ничего не теряем).
- 884 патча в ScrollArea модала — рендер каждый кадр, как было в панели (та же стоимость). N/A.

---

### Task 1: Модал-браузер + кнопка в строке парта

**Files:**
- Modify: `src/gui/mod.rs` (App fields ~346-350; вызов окон ~1291-1306)
- Modify: `src/gui/layers.rs` (draw_part_row, instrument select ~320-377)

- [ ] **Step 1: Поле App + открытие модала.** В `App` (рядом с `preset_search`) добавить:

```rust
    /// Some(part) = instrument browser modal is open for this part
    pub inst_browser_part: Option<usize>,
```

Инициализация в `App::new`-литерале: `inst_browser_part: None,`.

- [ ] **Step 2: Метод-модал в mod.rs.** В `impl App` добавить (рендер списка — дословный перенос внутренностей панели mod.rs:1137-1288, `part` теперь из `inst_browser_part`). СТРУКТУРА КРИТИЧНА: всё, что читает `modal` (в т.ч. `should_close()`), — строго ПОСЛЕ `.show(...)`; локальные аккумуляторы `picked_*`/`toggled_cat`/`close` объявлены ДО `.show` и обрабатываются ПОСЛЕ него:

```rust
    /// Instrument browser modal: search + category tree, applies to `inst_browser_part`.
    fn draw_instrument_browser_modal(&mut self, ctx: &egui::Context) {
        let Some(part) = self.inst_browser_part else { return };
        let is_sf2 = self.parts[part].sf2_mode && self.sf2_keys_soundfont.is_some();
        let layer_label = LAYER_NAMES.get(part).unwrap_or(&"?");

        let mut close = false;
        let mut picked_patch: Option<usize> = None;
        let mut picked_program: Option<u8> = None;
        let mut toggled_cat: Option<String> = None;

        let modal = egui::Modal::new(egui::Id::new("inst_browser")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.strong(if is_sf2 {
                format!("GM Instrument — Part {layer_label}")
            } else {
                format!("Patch — Part {layer_label}")
            });
            ui.add_space(theme::SP_XS);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let w = ui.available_width();
                let r = ui.add(egui::TextEdit::singleline(&mut self.preset_search)
                    .hint_text("\u{1F50D} Search...")
                    .desired_width((w - 22.0).max(60.0)));
                if !self.preset_search.is_empty()
                    && ui.small_button("\u{2715}").clicked()
                {
                    self.preset_search.clear();
                    r.request_focus();
                }
            });
            ui.separator();

            let query = self.preset_search.to_lowercase();
            let searching = !query.is_empty();

            egui::ScrollArea::vertical().auto_shrink(false)
                .max_height(420.0)
                .show(ui, |ui| {
                    if is_sf2 {
                        // ── GM categories (перенос mod.rs:1141-1205: клик пишет
                        //    picked_program, коллапс — toggled_cat) ──
                        use crate::synth::sampler::GM_PROGRAM_NAMES;
                        let current_program = self.parts[part].sf2_program;
                        let gm_categories: &[(&str, usize, usize)] = &[
                            ("Piano", 0, 8), ("Chromatic Perc", 8, 16),
                            ("Organ", 16, 24), ("Guitar", 24, 32),
                            ("Bass", 32, 40), ("Strings", 40, 48),
                            ("Ensemble", 48, 56), ("Brass", 56, 64),
                            ("Reed", 64, 72), ("Pipe", 72, 80),
                            ("Synth Lead", 80, 88), ("Synth Pad", 88, 96),
                            ("Synth FX", 96, 104), ("Ethnic", 104, 112),
                            ("Percussive", 112, 120), ("Sound FX", 120, 128),
                        ];
                        for &(cat_name, start, end) in gm_categories {
                            let matches: Vec<usize> = if searching {
                                (start..end)
                                    .filter(|&i| GM_PROGRAM_NAMES[i].to_lowercase().contains(&query)
                                        || cat_name.to_lowercase().contains(&query))
                                    .collect()
                            } else {
                                (start..end).collect()
                            };
                            if searching && matches.is_empty() { continue; }
                            let is_collapsed =
                                !searching && self.collapsed_categories.contains(cat_name);
                            let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
                            if !searching {
                                if ui.selectable_label(false, format!("{arrow} {cat_name}")).clicked() {
                                    toggled_cat = Some(cat_name.to_string());
                                }
                            } else {
                                ui.label(egui::RichText::new(cat_name).strong().small());
                            }
                            if !is_collapsed {
                                for i in matches {
                                    let name = GM_PROGRAM_NAMES[i];
                                    let selected = current_program == i as u8;
                                    if ui.selectable_label(selected, format!("  {name}")).clicked() && !selected {
                                        picked_program = Some(i as u8);
                                    }
                                }
                            }
                        }
                    } else {
                        // ── DSP patch tree (перенос mod.rs:1216-1271: клик пишет
                        //    picked_patch) ──
                        let current_preset_idx = self.parts[part].patch_idx;
                        let categories: Vec<(String, usize)> = {
                            let mut cats = Vec::new();
                            let mut last = String::new();
                            for (i, p) in self.patches.iter().enumerate() {
                                if p.category != last {
                                    cats.push((p.category.clone(), i));
                                    last = p.category.clone();
                                }
                            }
                            cats
                        };
                        let mut cat_idx = 0;
                        for (i, patch) in self.patches.iter().enumerate() {
                            if cat_idx < categories.len() && categories[cat_idx].1 == i {
                                cat_idx += 1;
                            }
                            if searching {
                                let name_match = patch.name.to_lowercase().contains(&query);
                                let cat_match = patch.category.to_lowercase().contains(&query);
                                if !name_match && !cat_match { continue; }
                            }
                            if !searching {
                                if cat_idx > 0 && categories[cat_idx - 1].1 == i {
                                    let cat = &categories[cat_idx - 1].0;
                                    let is_collapsed = self.collapsed_categories.contains(cat);
                                    let arrow = if is_collapsed { "\u{25B6}" } else { "\u{25BC}" };
                                    if ui.selectable_label(false, format!("{arrow} {cat}")).clicked() {
                                        toggled_cat = Some(cat.clone());
                                    }
                                }
                                if self.collapsed_categories.contains(&patch.category) {
                                    continue;
                                }
                            }
                            let selected = i == current_preset_idx;
                            if ui.selectable_label(selected, format!("  {}", patch.name)).clicked() && !selected {
                                picked_patch = Some(i);
                            }
                        }
                    }
                });
        });

        // ── Post-closure handling (after .show, reads locals + modal) ────
        if let Some(cat) = toggled_cat {
            if self.collapsed_categories.contains(&cat) {
                self.collapsed_categories.remove(&cat);
            } else {
                self.collapsed_categories.insert(cat);
            }
            self.save_collapsed_categories();
        }
        if let Some(prog) = picked_program {
            self.parts[part].sf2_program = prog;
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                part, program: prog, bank: 0 });
            // Persist for parts 0/1 (seeds program on startup) — перенос mod.rs:1198-1200
            if part == 0 { self.config.sf2.layer_a_program = prog; }
            else { self.config.sf2.layer_b_program = prog; }
            let _ = self.config.save();
            close = true;
        }
        if let Some(idx) = picked_patch {
            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
            self.parts[part].patch_idx = idx;
            self.load_edited_params(part);
            self.send_edited_params(part);
            self.save_config();
            close = true;
        }
        if modal.should_close() {
            close = true;
        }
        if close {
            self.inst_browser_part = None;
        }
    }
```

Примечание: `egui::Modal::show(ctx, impl FnOnce(&mut Ui))` — FnOnce, мутирующие захваты локалей легальны; `modal.should_close()` существует (`ModalResponse`). Проверено ревьюером по egui-0.31.1 modal.rs.

- [ ] **Step 3: Вызов модала.** В `update()` после блока окон (mod.rs, после `if self.show_keybinds_window { ... }`, ~строка 1306):

```rust
        // Instrument browser modal (from part row)
        self.draw_instrument_browser_modal(ctx);
```

- [ ] **Step 4: Кнопки в строке парта (layers.rs).** В `draw_part_row`:
  - SF2-ветка (сейчас ComboBox `inst_sf2_{i}`, ~320-337): заменить ComboBox на кнопку:

```rust
                let prog = self.parts[i].sf2_program;
                let cur_name = GM_PROGRAM_NAMES.get(prog as usize).copied().unwrap_or("?");
                if ui.add_sized([170.0, 0.0], egui::Button::new(format!("prog {prog}: {cur_name}")))
                    .on_hover_text("Open instrument browser (search + categories)").clicked()
                {
                    self.preset_search.clear();
                    self.inst_browser_part = Some(i);
                }
```

  - DSP-ветка (сейчас ComboBox `inst_dsp_{i}`, ~354-377): заменить на кнопку (хелперы `patch_names`/`cur` оставить):

```rust
                if ui.add_sized([170.0, 0.0], egui::Button::new(&cur))
                    .on_hover_text("Open patch browser (search + categories)").clicked()
                {
                    self.preset_search.clear();
                    self.inst_browser_part = Some(i);
                }
```

  (Блок `if new_idx != cur_idx {...}` от ComboBox удалить целиком — выбор теперь через модал. Локальный `use crate::synth::sampler::GM_PROGRAM_NAMES;` в SF2-ветке layers.rs сохранить — новый код его использует.)

- [ ] **Step 5: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 6: Commit**

```bash
git add src/gui/mod.rs src/gui/layers.rs
git commit -m "feat: instrument browser modal from part row (search + categories)"
```

### Task 2: Init/Rnd/Undo/Redo в params editor + удаление правой панели

**Files:**
- Modify: `src/gui/mod.rs` (else-ветка right panel ~1355-1390; удаление SidePanel 1051-1289)

- [ ] **Step 1: Шапка params-редактора.** В else-ветке главного централ-панела (где `let is_sf2 = ...` и дальше `if is_sf2 ... else { ScrollArea draw_params_editable }`) перед ScrollArea вставить шапку с именем патча и кнопками (перенос логики mod.rs:1092-1112 + имя из правой панели):

```rust
                } else {
                    // Header: patch name + Init/Rnd/Undo/Redo
                    ui.horizontal(|ui| {
                        let idx = self.parts[part].patch_idx;
                        let name = self.patches.get(idx).map(|p| p.name.as_str()).unwrap_or("---");
                        ui.strong(name);
                        ui.separator();
                        if ui.small_button("Init").on_hover_text("Reset to default patch").clicked() {
                            self.push_undo_snapshot();
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].edited_params = crate::synth::PatchParams::default().to_map();
                            self.send_edited_params(part);
                        }
                        if ui.small_button("Rnd").on_hover_text("Randomize patch").clicked() {
                            self.push_undo_snapshot();
                            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                            self.parts[part].edited_params = crate::synth::PatchParams::random_map();
                            self.send_edited_params(part);
                        }
                        ui.separator();
                        if ui.add_enabled(!self.undo_stack.is_empty(), egui::Button::new("\u{21B6}").small()).on_hover_text("Undo (Ctrl+Z)").clicked() {
                            self.undo();
                        }
                        if ui.add_enabled(!self.redo_stack.is_empty(), egui::Button::new("\u{21B7}").small()).on_hover_text("Redo (Ctrl+Shift+Z)").clicked() {
                            self.redo();
                        }
                    });
                    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                        self.draw_params_editable(ui);
                    });
                }
```

  (Ветка `if is_sf2 && self.sf2_keys_soundfont.is_some()` со статус-строкой — без изменений, Init/Rnd для SF2 не показываем, как было.)

- [ ] **Step 2: Удалить SidePanel::right("preset_panel")** целиком (mod.rs:1051-1289, от комментария `// Right: patch list ...` до закрывающей `});` перед `// Settings window`).

- [ ] **Step 3: Мёртвое и зеркало теста.** Grep `LAYER_NAMES` в mod.rs — если использовался только панелью и новым модалом, оставить (модал его использует). Grep `preset_search` — должен остаться (модал). Если clippy ругается на неиспользуемое — удалить конкретное. Затем **tests/ui_layout_contract.rs**: тест зеркалит `SidePanel::right("preset_panel")` (там ~строка 52) и тестирует «phantom layout» после удаления панели — обновить зеркало: убрать right-panel из регистрации в `frame()` (helper вернёт кортеж на элемент меньше — поправить деструктуризацию в обоих тестах), удалить/поправить `gm_panel_resize_reflows_central` и правую часть `panels_carve_no_overlap`. Заголовок теста явно требует синхронизировать зеркало с mod.rs.

Примечание о сознательной потере: кнопки ◀▶ (prev/next preset, mod.rs:1065-1091) удаляются вместе с панелью БЕЗ замены — вариант 1 согласован с юзером, перебор патчей остаётся возможен через модал. Если позже понадобится — вернём как кнопки рядом с кнопкой патча в строке парта.

- [ ] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 5: Commit**

```bash
git add src/gui/mod.rs tests/ui_layout_contract.rs
git commit -m "feat: drop presets side panel; Init/Rnd/Undo/Redo in params header"
```

### Task 3: Финальная верификация

- [ ] **Step 1:** Полные гейты обоих коммитов уже зелёные — прогнать `cargo build --release`.

- [ ] **Step 2: Ручной чеклист (юзер):**
1. Правой панели нет, парты на всю ширину
2. Кнопка патча в DSP-строке → модал с поиском и категориями; клик применяет и закрывает
3. Кнопка prog в SF2-строке → модал с GM-категориями
4. Поиск по имени и категории работает, ✕ сбрасывает
5. Esc/клик мимо — модал закрылся без выбора
6. Init/Rnd/↶/↷ — в шапке params-редактора, работают для DSP-парта
7. После рестарта: SF2-программа партов A1/A2 и выбранный DSP-патч восстановились
