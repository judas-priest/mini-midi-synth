# Layers Redesign: All-Parts Rows with Two Zones Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Layers-вкладка показывает ВСЕ части как строки (две зоны: левая/правая), каждая строка = все поля части (инструмент, движок, мьют/рут, диапазон, pan/tr/vel/volume, remove). Visual spec: docs/mockup_layers2.html.

**Architecture:** Чистый GUI-рефакторинг src/gui/layers.rs — заменить «табы + параметры одной части» на «строки всех частей». Все поля/ControlEvent уже существуют — меняется только компоновка. Нулевые изменения в движке/звуке.

**Tech Stack:** egui 0.31, существующие ControlEvent.

---

## Test Cases (from the requirement)

1. Все включённые части видны как строки на одном экране (зона A сверху, зона B снизу)
2. В каждой строке: имя, движок (DSP/SF2 кнопка), инструмент (выпадашка), Mute, Root, диапазон, Pan, Tr, Vel, Volume, Remove
3. Split checkbox + split point — над зонами
4. Perf Save/Load — внизу страницы
5. Кнопка «+ Part» в каждой зоне
6. Вёрстка не ломается при малом окне (scroll)

## Negative Matrix

- Пустая зона B (Split off) — зона B скрыта, зона A на всю ширину
- Все 8 партов зоны A заняты — кнопка «+ Part» скрыта
- Часть без инструмента — показывается «---», Remove недоступен для первого парта зоны
- Малое окно — ScrollArea вертикальный
- sf2_mode=true — показывается SF2 program select, иначе DSP patch select
- Удаление первого парта зоны — кнопка скрыта (existing guard)

---

## Task 1: Строки частей вместо табов

**Files:**
- Modify: `src/gui/layers.rs` (draw_part_tabs → draw_part_rows; draw_part_tab удалён)

- [ ] **Step 1: Заменить табы на строки.** Прочитать текущие `draw_part_tabs` (layers.rs:24) и `draw_part_tab` (layers.rs:326). Заменить `draw_part_tabs` целиком:

```rust
    pub(super) fn draw_part_rows(&mut self, ui: &mut egui::Ui) {
        // ── Zone A ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("◀ Левая рука (A)").weak().small());
        });
        for i in ZONE_A {
            if !self.parts[i].enabled { continue; }
            self.draw_part_row(ui, i);
        }
        let a_count = ZONE_A.filter(|&i| self.parts[i].enabled).count();
        if a_count < PARTS_PER_ZONE {
            if ui.small_button("+ Добавить парт").on_hover_text("Add part to Zone A").clicked() {
                self.add_part_to_zone(false);
            }
        }

        ui.add_space(theme::SP_XS);

        // ── Zone B (visible only when split enabled) ──────────────────────
        let split_on = self.parts[ZONE_B].iter().any(|l| l.enabled);
        if split_on {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("▶ Правая рука (B)").weak().small());
            });
            for i in ZONE_B {
                if !self.parts[i].enabled { continue; }
                self.draw_part_row(ui, i);
            }
            let b_count = ZONE_B.filter(|&i| self.parts[i].enabled).count();
            if b_count < PARTS_PER_ZONE {
                if ui.small_button("+ Добавить парт").on_hover_text("Add part to Zone B").clicked() {
                    self.add_part_to_zone(true);
                }
            }
        }

        // ── Split toggle + point ─────────────────────────────────────────
        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            let split_on = self.parts[ZONE_B].iter().any(|l| l.enabled);
            let mut split = split_on;
            if ui.checkbox(&mut split, "Split").on_hover_text("Enable keyboard split").changed() {
                if split {
                    if let Some(i) = ZONE_B.into_iter().find(|&i| !self.parts[i].enabled) {
                        self.parts[i].enabled = true;
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: true });
                        self.send_edited_params(i);
                        self.send_part_volume(i);
                    }
                    self.apply_split_ranges();
                } else {
                    for i in ZONE_B {
                        if self.parts[i].enabled {
                            self.parts[i].enabled = false;
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
                        }
                    }
                    self.apply_split_ranges();
                }
            }
            if split_on {
                ui.label("Split point:");
                let mut sp = self.split_point as i32;
                if ui.add(egui::Slider::new(&mut sp, 0..=127).suffix(format!(" ({})", note_name(sp))).smallest())
                    .on_hover_text("Split point (zone B starts here)").changed() {
                    self.set_split_point(sp as u8);
                }
            }
        });
    }
```

(Адаптируй: `note_name`, `set_split_point`, `split_point` — найди реальные имена в layers.rs; если их нет — создай мини-хелпер.)

- [ ] **Step 2: draw_part_row — все поля в одной строке.** Новый метод (вместо draw_part_tab):

```rust
    fn draw_part_row(&mut self, ui: &mut egui::Ui, i: usize) {
        let is_active = i == self.active_part;
        let zone = if i < PARTS_PER_ZONE { "A" } else { "B" };
        let num = (i % PARTS_PER_ZONE) + 1;
        let muted = self.parts[i].mute;

        ui.horizontal(|ui| {
            if is_active { ui.style_mut().visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::LIGHT_BLUE); }

            // Part label
            ui.label(egui::RichText::new(format!("{zone}{num}")).strong());

            // Engine toggle (DSP/SF2)
            let sf2 = self.parts[i].sf2_mode;
            let eng_label = if sf2 { "SF2" } else { "DSP" };
            if ui.small_button(eng_label).on_hover_text(
                if sf2 { "Switch to DSP engine" } else { "Switch to SF2 engine" }
            ).clicked() {
                self.parts[i].sf2_mode = !sf2;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: !sf2 });
            }

            // Instrument select
            if sf2 {
                // SF2 program select
                let mut prog = self.parts[i].sf2_program;
                let inst = egui::ComboBox::from_id_salt(format!("inst_sf2_{i}"))
                    .width(160.0)
                    .selected_text(format!("SF2 prog {}", prog))
                    .show_ui(ui, |ui| {
                        for p in 0..128 {
                            ui.selectable_value(&mut prog, p as u8, format!("Prog {}", p + 1));
                        }
                    });
                if inst.changed() {
                    self.parts[i].sf2_program = prog;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program { part: i, program: prog, bank: 0 });
                }
            } else {
                // DSP patch select
                let cur = self.patches.get(self.parts[i].patch_idx)
                    .map(|p| p.name.as_str()).unwrap_or("---").to_string();
                egui::ComboBox::from_id_salt(format!("inst_dsp_{i}"))
                    .width(160.0)
                    .selected_text(&cur)
                    .show_ui(ui, |ui| {
                        for (pi, p) in self.patches.iter().enumerate() {
                            ui.selectable_value(&mut self.parts[i].patch_idx, pi, &p.name);
                        }
                    });
                if self.parts[i].patch_idx != /* previous */ {
                    let _ = self.ctrl_tx.push(ControlEvent::SetPatch { part: i, patch_idx: self.parts[i].patch_idx });
                }
            }

            // Mute / Root
            let mut m = self.parts[i].mute;
            if ui.checkbox(&mut m, "M").on_hover_text("Mute").changed() {
                self.parts[i].mute = m;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: m });
                self.persist_parts_state();
            }
            let mut r = self.parts[i].root_only;
            if ui.checkbox(&mut r, "R").on_hover_text("Root-only (bass split)").changed() {
                self.parts[i].root_only = r;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartRootOnly { part: i, on: r });
                self.persist_parts_state();
            }

            // Key range
            ui.label(format!("{}–{}", self.parts[i].min_note, self.parts[i].max_note));

            // Pan
            ui.label("Pan:");
            let mut pan = self.parts[i].pan;
            if ui.add(egui::Slider::new(&mut pan, -1.0..=1.0).fixed_decimals(1)).changed() {
                self.parts[i].pan = pan;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan });
            }

            // Tr
            ui.label("Tr:");
            let mut tr = self.parts[i].transpose as i32;
            if ui.add(egui::Slider::new(&mut tr, -24..=24).suffix("st")).changed() {
                self.parts[i].transpose = tr as i8;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: tr as i8 });
            }

            // Vel
            ui.label("Vel:");
            let mut vmin = self.parts[i].vel_min as i32;
            let mut vmax = self.parts[i].vel_max as i32;
            let c1 = ui.add(egui::DragValue::new(&mut vmin).range(1..=127).speed(1)).changed();
            let c2 = ui.add(egui::DragValue::new(&mut vmax).range(1..=127).speed(1)).changed();
            if c1 || c2 {
                self.parts[i].vel_min = (vmin as u8).max(1);
                self.parts[i].vel_max = (vmax as u8).max(self.parts[i].vel_min);
                let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange {
                    part: i, vel_min: self.parts[i].vel_min, vel_max: self.parts[i].vel_max,
                });
            }

            // Volume
            let mut vol = self.parts[i].volume;
            if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).fixed_decimals(2)).changed() {
                self.parts[i].volume = vol;
                self.send_part_volume(i);
            }

            // Remove (not for zone-first part)
            let is_first = i.is_multiple_of(PARTS_PER_ZONE);
            if !is_first && ui.small_button("✕").on_hover_text("Remove this part").clicked() {
                self.remove_part(i);
            }
        });
    }
```

(ВАЖНО: `self.patches` —_immutable borrow, `self.parts` — mutable. Если borrow checker ругается — скопировать patch_name в локальную переменную до мьютекса. Проверить реальное поле: `patch_idx` vs `patch_name` в PartState — grep. Некоторые поля уже могут называться иначе — адаптировать.)

- [ ] **Step 3: Удалить draw_part_tab** (метод целиком — больше не нужен) и заменить вызов `draw_part_tabs` на `draw_part_rows` в update flow (grep `draw_part_tabs` в gui/mod.rs — найти вызов и заменить).

- [ ] **Step 4: Гейты**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```

- [ ] **Step 5: Commit**

```bash
git add src/gui/layers.rs
git commit -m "feat: Layers redesign — all-parts rows with two zones (mockup_layers2.html)"
```

### Task 2: Instrument select wiring (SF2 program + DSP patch inline)

Это уже включено в Task 1 Step 2. Отдельного таска нет — если borrow checker ругается на `self.patches`/`self.parts` в одном блоке, скопировать `patch_name` в локальную переменную до мьютекса.

### Task 3: Perf Save/Load внизу + Split row

Уже в Task 1 Step 1 (Split row) — Perf save/load остаётся на месте (не трогать, он внизу layers.rs и не конфликтует).

### Task 4: Полные гейты + TECHDEBT #31.5 done

- [ ] **Step 1:**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety && cargo deny check
```

- [ ] **Step 2: MANUAL CHECKLIST (юзер):**
1. Layers: все части строками, две зоны (A сверху / B снизу), Split между ними
2. Инструмент — выпадашка в строке части (SF2 program или DSP patch по движку)
3. Кнопка DSP/SF2 в начале каждой строки — переключение движка
4. Mute/Root — кнопки в строке
5. Pan/Tr/Vel/Volume — слайдеры в строке
6. «+ Добавить парт» — добавляет в соответствующую зону
7. «✕» — удаляет (кроме первого парта зоны)
8. Split on/off — скрывает/показывает зону B
9. Perf Save/Load — внизу страницы как было

- [ ] **Step 3: TECHDEBT #31.5** — добавить «Layers redesign РЕАЛИЗОВАН (2026-10-30)» + commit.

- [ ] **Step 4: Commit**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: Layers redesign realized"
```
