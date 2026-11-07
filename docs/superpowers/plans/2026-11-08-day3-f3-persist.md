# Stage: F3 — персист-граница (dirty + debounce) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.

status: executed 2026-11-08 (29 сайтов → 1 точка записи; ST8 отложен — отдельная сессия)

anchor-checked: 2026-11-08 (база = master d3813a3; канон — Tavily 2026-11-08:
«периодический батч-save как middle ground» — erdosmiller; аудит F3: 61 точка
записи, каждый чекбокс = полный JSON на диск)

**Goal:** Один пункт записи конфига: 61 точка → mark-dirty; flush раз в 2 с в
конце update() + форс-flush на выходе (on_exit). Диск перестаёт писать на
каждый чекбокс.

**Architecture:** App поле `config_dirty: bool` + `fn mark_config_dirty(&mut
self)` + `fn flush_config(&mut self, force: bool)`. Все 4 обёртки
(save_config/save_global_to_config/persist_parts_state + прямые `.config.save()`)
→ mark_config_dirty (внутренние поля конфига обновляются КАК РАНЬШЕ — меняется
только момент записи на диск). update() в самом конце: `flush_config(false)`
(пишет если dirty и прошло ≥2 с с последней записи; last_config_save уже
существует — переиспользовать). on_exit: `flush_config(true)`. ПЕРФ-файлы
(preset.rs save_performance_file) НЕ трогаются — там осознанная мгновенная
запись.

**Tech Stack:** Rust; гейты стандартные. НЕТ тестов на App-уровень — вечерняя
проверка юзером (изменить громкость → подождать 3 с → убить приложение →
перезапустить → настройка на месте).

**Проверенные факты:**
- last_config_save (App поле, mod.rs) — используется только троттлингом
  save_global_to_config; тип Instant.
- config.save() — config.rs:383 (полный serde JSON).
- on_exit существует (impl eframe::App on_exit :847-849).

---

## Test Cases

- TC1: гейты (clippy/test/goldens/rt_safety/ui_layout) зелёные; goldens
  байт-стабильны (персист вне рендера).
- TC2: поведенческий контракт: в течение ≤2 с после изменения — файл может
  быть старым; на выходе — ВСЕГДА свежий (on_exit force). Проверка юзером
  вечером.
- TC3: grep-пин: в gui/ не остаётся прямых `.config.save()` кроме
  flush_config (единственная точка записи).

## Negative Matrix

- Краш в 2-секундном окне → потеря последних ≤2 с изменений настроек —
  осознанный трейд-офф (канон батч-save); на выходе форс.
- Несколько dirty за 2 с → один write (цель).
- flush при незакрытом диске/ошибке → let _ = (как было), следующая dirty
  повторит попытку через 2 с.
- Ре-энтрантность: flush_config зовётся раз в конце update — ре-энтранта нет.
- perf-файлы — не через эту границу (мгновенно, как было).
- headless — не трогается (у headless свой путь; ST8 отдельно).

---

### Task 1: граница

**Files:** Modify `src/gui/mod.rs`, `src/gui/settings.rs` (его вызовы
save_global_to_config/save_config остаются — они станут dirty-маркерами),
`src/gui/layers.rs`, `src/gui/drums.rs` (вызовы persist_parts_state и
save_global_to_config — остаются, станут маркерами).

- [x] Step 1: App поля: `config_dirty: bool` (init false в App::new);
  `fn mark_config_dirty(&mut self) { self.config_dirty = true; }`;
  `fn flush_config(&mut self, force: bool)`:
```rust
    /// Единственная точка записи конфига на диск (F3): dirty-маркер +
    /// дебаунс 2 с. force=true — на выходе приложения.
    fn flush_config(&mut self, force: bool) {
        if !self.config_dirty { return; }
        let due = self.last_config_save
            .map(|t| t.elapsed().as_secs_f32() >= 2.0)
            .unwrap_or(true);
        if force || due {
            let _ = self.config.save();
            self.last_config_save = Some(std::time::Instant::now());
            self.config_dirty = false;
        }
    }
```
  (тип last_config_save сверить: если Option<Instant> — ок; если не Option —
  адаптировать.)
- [x] Step 2: конвертация ВСЕХ точек (grep `.config.save()\|self.save_config()\|save_global_to_config()\|persist_parts_state()` в src/gui/): каждая
  → `self.mark_config_dirty();` ВНУТРИ обёрток: save_config/save_global_to_config/
  persist_parts_state тела заменяют дисковую запись на mark_config_dirty
  (остальная логика обёрток — копирование полей — остаётся). Прямые
  `let _ = self.config.save();` в gui-файлах → `self.mark_config_dirty();`.
  settings.rs вызовы save_global_to_config/save_config — не меняются (обёртки
  стали маркерами). ВАЖНО: save_drum_remap/save_drum_pad_vel (drums-персист
  в config?) — проверить, что идут через config.save — тоже маркер.
  ИСКЛЮЧЕНИЯ (остаются мгновенными): save_performance_file (preset.rs) —
  не в gui.
- [x] Step 3: update() конец (после CentralPanel): `self.flush_config(false);`
  on_exit: `self.flush_config(true);`.
- [x] Step 4: grep-пин TC3 вручную + гейт полный.
- [x] Step 5: коммит `refactor: config persistence boundary — dirty flag + 2s debounce + on_exit force (F3)`.

### Task 2: Верификация стадии

- [x] Step 1: полный гейт.
- [x] Step 2: PR → squash → rebuild → launch-check.
- [x] Step 3: roadmap + TECHDEBT + «УТРОМ: изменение громкости → 3 с → kill →
  restart → сохранилось».

## Out of scope
- ST8 headless consolidate — отдельная сессия (behavior change headless).
- perf-файлы мгновенной записи — осознанно.
