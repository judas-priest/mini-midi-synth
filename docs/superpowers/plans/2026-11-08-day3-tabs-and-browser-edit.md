# Stage: Вкладки toggle-off + редактор параметров в браузере Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

anchor-checked: 2026-11-08 (база = master 0c577cd; канон — Tavily 2026-11-08:
Analog Lab паттерн «браузер выбирает → редактор параметров рядом», Vital/u-he:
browser + edit page; юзер-спека 2026-11-08: «логичнее в браузере», «как выйти
в нейтральное положение?»)

**Goal:** (1) клик по активной вкладке возвращает в нейтральный вид; (2) полный
редактор параметров DSP-пресета — в модалке браузера (под списком); (3) на
главной вкладке — компактно (секвенсор + шапка + макро-строка, без 1430-строчного
редактора).

**Architecture:** layers.rs — клик по уже-активной вкладке сбрасывает все флаги;
params.rs — `draw_params_editable` → `draw_params_editable_for(ui, part)` (undo
через `push_undo_snapshot_for(part)` — корректный парт, не active_part);
browser.rs — после списка секция «Параметры» в ScrollArea для inst_browser_part
(только DSP-парт: sf2-парт показывает статусную строку как в main); mod.rs —
главная ветка без draw_params_editable, hint-строка вместо него.

**Tech Stack:** Rust/egui; behavior: UI-редизайн (осознанный, юзер-спека);
goldens не затрагиваются (рендер аудио); ui_layout_contract — панели
регистрируются как раньше.

**Проверенные факты (HEAD 0c577cd):**
- layers.rs:100-117: клик по Drums/MIDI Seq/FX Chain ставит флаг безусловно —
  выхода в нейтральный вид нет.
- params.rs:247 `draw_params_editable(&mut self, ui)` — `let part =
  self.active_part;` внутри; undo через `self.push_undo_snapshot()` (active-based)
  в 4 местах (osc_type :289 + matrix ×3 + остальные секции).
- browser.rs: ModalWindow 540px; список; Done в футере shell; post-closure
  блок после .show.
- mod.rs:1502: `self.draw_params_editable(ui);` в else-ветке (DSP-парт);
  is_sf2-ветка показывает статусную строку.

---

## Test Cases

- TC1: клик Drums → контент вкладки; повторный клик Drums → нейтральный вид
  (шапка пресета + параметры). — код-ревью + экран.
- TC2: браузер (DSP-парт) — под списком редактор параметров этого парта
  (шапка «Farfisa (A2)» + макро + секции); скроллится в модалке.
- TC3: браузер (SF2-парт) — секция параметров скрыта (статусная строка).
- TC4: правки параметров В браузере пишутся в edited_params ПРАВИЛЬНОГО парта
  (undo-пин: push_undo_snapshot_for(part)); после Done правки живы.
- TC5: главная вкладка — секвенсор + шапка + макро-строка + hint; полный
  редактор не дублируется.
- TC6: гейты (clippy/test/goldens/rt_safety/ui_layout) зелёные; goldens
  байт-стабильны.

## Negative Matrix

- Браузер открыт с партом ≥ 2 → редактор работает (undo snapshot for(part)).
- inst_browser_part None → секция пропускается (guard уже есть в draw-fn).
- sf2-парт в браузере → hint «SF2: параметры DSP не применяются».
- Правка в браузере при другом active_part → undo снапшот правого парта
  (push_undo_snapshot_for), не active_part.
- Макро-строка на главной — draw_macros вызывается без редактора (компакт).
- ~~timeout/clock~~ — n/a.

---

### Task 1: layers.rs — toggle-off вкладок

**Files:** Modify `src/gui/layers.rs:100-112`.
- [ ] Step 1: каждый клик: если вкладка уже активна — сброс всех флагов:
```rust
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").on_hover_text("Drum sequencer").clicked() {
                if self.show_drums && !self.show_midi_seq {
                    // повторный клик по активной вкладке = нейтральный вид
                    self.show_drums = false;
                } else {
                    self.show_drums = true; self.show_looper = false;
                    self.show_midi_seq = false; self.show_fx_chain = false;
                    self.seq_target_atom.store(0, Ordering::Relaxed);
                }
            }
```
  (аналогично MIDI Seq и FX Chain — без seq_target_atom строки).
- [ ] Step 2: гейт + коммит `fix: click active view tab returns to neutral layout (tabs toggle-off)`.

### Task 2: params.rs — draw_params_editable_for(ui, part)

**Files:** Modify `src/gui/params.rs`.
- [ ] Step 1: `draw_params_editable(&mut self, ui)` → тело в новую
  `draw_params_editable_for(&mut self, ui, part: usize)` (убрать
  `let part = self.active_part;`); ВСЕ `self.push_undo_snapshot()` внутри →
  `self.push_undo_snapshot_for(part);` (grep по телу: osc_type :289 и др.).
- [ ] Step 2: обёртка:
```rust
    pub(super) fn draw_params_editable(&mut self, ui: &mut egui::Ui) {
        self.draw_params_editable_for(ui, self.active_part);
    }
```
- [ ] Step 3: гейт + коммит `refactor: draw_params_editable_for(part) — undo targets the edited part`.

### Task 3: браузер — секция параметров

**Files:** Modify `src/gui/browser.rs`.
- [ ] Step 1: после списка (перед post-closure блоком), внутри win-контента:
```rust
            ui.separator();
            if !self.parts[part].sf2_mode {
                ui.strong("Параметры пресета");
                egui::ScrollArea::vertical()
                    .id_salt("browser_params_scroll")
                    .max_height(220.0)
                    .show(ui, |ui| {
                        self.draw_params_editable_for(ui, part);
                    });
            } else {
                ui.label(egui::RichText::new(
                    "SF2: у семплов нет DSP-параметров").weak());
            }
```
  (part — локал браузера = inst_browser_part.unwrap_or_default; секция
  скроллится, окно может остаться 540 — контент с прокруткой).
- [ ] Step 2: гейт (ui_layout_contract входит) + коммит
  `feat: instrument browser — preset params editor below the list (DSP parts)`.

### Task 4: главная вкладка — компактный вид

**Files:** Modify `src/gui/mod.rs` (else-ветка CentralPanel).
- [ ] Step 1: убрать `self.draw_params_editable(ui);`; после шапки добавить:
```rust
                        self.draw_macros(ui);
                        ui.add_space(theme::SP_SM);
                        ui.label(egui::RichText::new(
                            "Полные параметры пресета — кнопка Browse на строке парта").weak());
```
  (draw_macros pub(crate)? — проверить видимость из mod.rs: он в gui/mod.rs
  impl App — mod.rs и есть gui/mod.rs — ок).
- [ ] Step 2: гейт + коммит `feat: main view compact — macros strip + params in browser (S4-gui polish)`.

### Task 5: Верификация стадии

- [ ] Step 1: полный гейт (5 команд).
- [ ] Step 2: PR → squash → rebuild → launch.
- [ ] Step 3: «ПРОВЕРИТЬ ВЕЧЕРОМ»: вкладки toggle-off; браузер показывает
  параметры под списком; правки из браузера применяются и переживают Done;
  главная — компактная.

## Out of scope
- SF2-парт: страница настроек семплов (кроме статуса) — фича по фидбеку.
