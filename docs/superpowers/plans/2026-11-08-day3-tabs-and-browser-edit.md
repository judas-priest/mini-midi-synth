# Stage: Вкладки toggle-off + редактор параметров в браузере Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

status: executed 2026-11-08 (финальный ревью APPROVE; id-collision — false alarm: egui ids иерархические)

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
- [x] Step 1: каждый клик: если вкладка уже активна — сброс всех флагов:
```rust
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").on_hover_text("Drum sequencer").clicked() {
                if self.show_drums && !self.show_midi_seq {
                    // повторный клик по активной вкладке = нейтральный вид
                    self.show_drums = false; self.show_looper = false;
                    self.show_midi_seq = false; self.show_fx_chain = false;
                } else {
                    self.show_drums = true; self.show_looper = false;
                    self.show_midi_seq = false; self.show_fx_chain = false;
                    self.seq_target_atom.store(0, Ordering::Relaxed);
                }
            }
```
  (аналогично MIDI Seq и FX Chain — без seq_target_atom строки).
- [x] Step 2: гейт + коммит `fix: click active view tab returns to neutral layout (tabs toggle-off)`.

### Task 2: params.rs — draw_params_editable_for(ui, part)

**Files:** Modify `src/gui/params.rs`, `src/gui/macros.rs`.
- [x] Step 1: `draw_params_editable(&mut self, ui)` → тело в новую
  `pub(super) fn draw_params_editable_for(&mut self, ui, part: usize)`
  (убрать `let part = self.active_part;`). В теле **58** сайтов
  `self.push_undo_snapshot()` → `self.push_undo_snapshot_for(part);`
  (механическая замена по телу функции :247-1663 — ревью перечислило все
  строки). ВСЕ `part`-зависимые чтения уже self.* — тело verbatim.
- [x] Step 2: **macros.rs — параметризовать парт** (ревью HIGH):
  `draw_macros(&mut self, ui, part: usize)` + `draw_xy_pad(&mut self, ui,
  part: usize)` — внутренние `self.active_part` → `part` (включая
  push_undo_snapshot_for и SetMacro { part }); обёртка для главного экрана:
  `pub(super) fn draw_macros(&mut self, ui) { self.draw_macros(ui,
  self.active_part); }` — СТАРЫЕ вызовы не меняются.
- [x] Step 3: обёртка в params.rs:
```rust
    pub(super) fn draw_params_editable(&mut self, ui: &mut egui::Ui) {
        self.draw_params_editable_for(ui, self.active_part);
    }
```
- [x] Step 4: гейт + коммит `refactor: draw_params_editable_for(part) + draw_macros(part) — undo targets the edited part`.

### Task 3: браузер — секция параметров

**Files:** Modify `src/gui/browser.rs`.
- [x] Step 1: РЕВЬЮ-ФАКТЫ: окно fixed_width(400) + default_height(540), НЕТ
  внутреннего скролла (egui Window::neither), контент шире окна клипается,
  окно растёт по контенту до off-screen. РЕШЕНИЕ: (а) fixed_width 400→520
  (слайдеры SLIDER_SECONDARY ~180px влезают; список +56px — ок); (б) секция
  параметров — в ВНУТРЕННИЙ ScrollArea с max_height 200 (ограничивает рост
  окна); (в) insertion point — после закрытия списка (:356), до `});` (:360).
- [x] Step 2: после списка (внутри win-контента, `part` в scope — let-else :7):
```rust
            ui.separator();
            if !self.parts[part].sf2_mode {
                ui.strong("Параметры пресета");
                egui::ScrollArea::vertical()
                    .id_salt("browser_params_scroll")
                    .max_height(200.0)
                    .show(ui, |ui| {
                        self.draw_params_editable_for(ui, part);
                    });
            } else {
                ui.label(egui::RichText::new(
                    "SF2: у семплов нет DSP-параметров").weak());
            }
```
- [x] Step 3: гейт (ui_layout_contract входит) + коммит
  `feat: instrument browser — preset params editor below the list (DSP parts)`.

### Task 4: главная вкладка — компактный вид

**Files:** Modify `src/gui/mod.rs` (else-ветка CentralPanel).
- [x] Step 1: убрать `self.draw_params_editable(ui);`; после шапки добавить:
```rust
                        self.draw_macros(ui); // обёртка: active_part
                        ui.add_space(theme::SP_SM);
                        ui.label(egui::RichText::new(
                            "Полные параметры пресета — кнопка с именем патча на строке парта").weak());
```
  (draw_macros — 4×2 сетка + XY-пад ~220px; компактная «строка» = сетка + пад.)
- [x] Step 2: гейт + коммит `feat: main view compact — macros strip + params in browser (S4-gui polish)`.

### Task 5: Верификация стадии

- [x] Step 1: полный гейт (5 команд).
- [x] Step 2: PR → squash → rebuild → launch.
- [x] Step 3: «ПРОВЕРИТЬ ВЕЧЕРОМ»: вкладки toggle-off; браузер показывает
  параметры под списком; правки из браузера применяются и переживают Done;
  главная — компактная.

## Out of scope
- SF2-парт: страница настроек семплов (кроме статуса) — фича по фидбеку.
