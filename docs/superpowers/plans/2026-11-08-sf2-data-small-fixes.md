# SF2/data мелочи батч: F2+F4+F5+F12-F15 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD d351ce9; все якоря верифицированы
data-layer верификатором против 663b03d, QW3 #106 synth-файлы не трогал —
сверить по именам при исполнении, номера могли слегка съехать)

**Goal:** 7 LOW-находок data-аудита: starvation retry (F2), документация skip_samples (F4), pitch_bend_range внутрь
load_keys_soundfont (F5), лог stale param_key (F12), обновление range из
реестра (F13), лог unknown keybinds (F14), "?"-round-trip keybinds (F15).

**Architecture:** точечные правки в src/synth/sampler.rs, src/synth/mod.rs,
src/cc_map.rs, src/gui/keybinds.rs. F4 — только документирование (design
accepted). Каждый фикс независим, порядок свободный.

**Tech Stack:** Rust; юнит-тесты где тестируемо; goldens не затрагиваются
(DSP-рендер не меняется; F5 — SF2-события, не рендер-путь factory пресетов).

**Проверенные факты (663b03d → имена; номера примерные):**
- F2: sampler.rs комментарий 210-213 «16th part stays DSP until a channel
  frees up»; единственная аллокация — set_part_mode (391), освобождение (393)
  не ре-аллоцирует ждущие части.

- F4: sampler.rs 480-487 note_on → skip_samples (159-183) на ОБЩЕМ keys
  synth; clamp 0-50ms на 476; рендер-и-дроп до 50 ms для ВСЕХ звучащих SF2
  частей. Design accepted — комментировать, не менять.
- F5: sampler.rs 293-306 load_keys_soundfont ре-ассертит release scale +
  sustain, но НЕ pitch_bend_range; коллеры сами до-патчат (mod.rs:1861,
  sampler.rs:366-368).
- F12: cc_map.rs:172-181 resolve_key fail → бинд молча выброшен, дефолтный
  воскресает; лога нет.
- F13: cc_map.rs:173 обновляет только scope; 174-179 сохраняют сохранённые
  min_val/max_val/logarithmic навсегда.
- F14: gui/keybinds.rs:22-26 filter_map молча дропает неизвестные
  key/action-строки.
- F15: gui/keybinds.rs:123-125 key_to_str fallback "?" → key_from_str("?")
  = None → бинд молча теряется при перезагрузке; to_config 53-57.

---

## Test Cases (from the requirement)

- TC-F2: 15 частей SF2- enabled; 16-я включается (каналов нет) → остаётся
  DSP; одна выключается → 16-я получает освободившийся канал (retry) +
  program этой части.

- TC-F4: код-ревью комментария (рендер-семантика не меняется).
- TC-F5: load_keys_soundfont после загрузки применяет pitch_bend_range
  сам; вызовы-коллеры больше не дублируют (grep: 0 внешних до-патчей).
- TC-F12/F14: config со stale ключом → log::warn зафиксирован (тестом не
  проверяем — лог; код-ревью).
- TC-F13: сохранённый бинд со старым range (registry обновился) →
  from_config возвращает registry range.
- TC-F15: keybind с ключом вне KEY_TABLE → to_config не пишет "?" (пропуск
  + warn), round-trip не создаёт мусорную запись.

## Negative Matrix

- F2: часть НЕ sf2-enabled без канала — не участвует в retry (гвард по
  part_sf2[i]); alloc_channel вернул None (все заняты) — остаётся DSP.

- F4: sample_offset=0 — skip не вызывается (существующая ветка).
- F5: pb-range равен дефолту — ре-ассерт идемпотентен.
- F13: параметр исчез из registry → resolve fail → ветка F12 (warn + дефолт).
- F15: key_to_str None не должен ломать существующие валидные бинды.
- ~~~timeout/clock~~ — n/a; ~~~конкурентность~~ — n/a (однопоточные пути).

---

### Task 1: F2 — retry SF2-канала при освобождении

**Files:** Modify `src/synth/sampler.rs` (set_part_mode, disable-ветка
~399-406 post-QW3).

- [ ] Step 1: failing-тест в mod tests sampler.rs: 15 частей sf2-enabled
  (через set_part_mode), 16-я включена (осталась DSP, канал None), часть 0
  выключается → assert канал достался 16-й (part_channel[..].is_some()) и
  part_program корректен (getter part_program()). Конструкцию Sampler списать
  с существующих sampler-тестов (5 в mod tests).
- [ ] Step 2: реализация (ПОРЯДОК КРИТИЧЕН — review C1: если флаг
  part_sf2[part] не сбросить ДО retry, цикл вернёт канал самой выключаемой
  части → утечка канала):
```rust
        } else if !enabled {
            if let Some(ch) = self.part_channel[part].take() {
                if let Some(synth) = &mut self.keys.synth {
                    synth.note_off_all_channel(ch as i32, false);
                }
            }
            // F2: сброс флага ДО retry-скана — иначе i==part удовлетворяет
            // гварду и канал утечёт обратно в DSP-часть (review C1).
            self.part_sf2[part] = enabled;
            // F2: освободившийся канал отдаём ждущей SF2-части («16th stays
            // DSP until a channel frees up» — но free никогда не
            // ре-аллоцировал; audit data #F2).
            for i in 0..self.part_sf2.len() {
                if self.part_sf2[i] && i != part && self.part_channel[i].is_none() {
                    self.set_part_mode(i, true);
                    break;
                }
            }
        }
        self.part_sf2[part] = enabled;
```
  (set_part_mode(i, true) сам аллоцирует + шлёт program после QW3-7;
  рекурсия глубиной 1 — enable-ветка без retry, безопасно, review подтверждён.
  Индексный цикл, не iterator — иначе immutable borrow self.part_sf2 живёт
  через &mut self вызов, E0502, review round-2 F-C.
  ЗАМЕЧАНИЕ (pre-existing класс, не расширяем): alloc_channel игнорирует
  seq_channel_used — retry при живом MIDI-seq может дать 0xC0 на seq-канал;
  тот же риск у первой аллокации. Комментарий рядом.
  ФИНАЛЬНАЯ ФОРМА: завершающая строка `self.part_sf2[part] = enabled;` (406)
  ОСТАЁТСЯ после if/else — она обязательна для enable-пути (enable-ветка флаг
  не ставит); на disable-пути её повторный store = безвредный no-op
  (review round-2 F-B).)
- [ ] Step 3: gates: clippy -D warnings; `cargo test` (+ sampler suite);
  rt_safety (set_part_mode на RT-потоке).
- [ ] Step 4: коммит `fix: freed SF2 channel retried to starving part (audit data #F2)`.

### Task 2: F4 — документация skip_samples + F5 — pb-range внутрь loader

**Files:** Modify `src/synth/sampler.rs` (комментарий у skip_samples 159-183
и note_on 476-487; load_keys_soundfont 293-306; удалить до-патчи на
mod.rs:1861 и sampler.rs:366-368).

- [ ] Step 1: комментарий F4 над skip_samples:
```rust
/// ВНИМАНИЕ (design-accepted, audit data #F4): рендер-и-дроп идёт на ОБЩЕМ
/// keys synth — note_on с sample_offset>0 отбрасывает до 50 ms звучания для
/// ВСЕХ одновременно звучащих SF2 частей + до 50 ms лишнего рендера на ноту.
/// Альтернатива (scratch-синт для прематча) — отдельная фича.
```
  (написать по-русски без «丢弃»: «отбрасывает».) И краткая пометка у clamp
  на 476.
- [ ] Step 2: F5 — перенести блок ре-ассерта pitch_bend_range из mod.rs:1861
  и sampler.rs:366-368 ВНУТРЬ load_keys_soundfont (после release scale ~302):
```rust
        // F5: pb-range как часть контракта загрузки (коллеры дублировали —
        // новый caller молча терял диапазон; audit data #F5).
        self.set_pitch_bend_range(...); // фактический вызов/сигнатуру списать с mod.rs:1861
```
  и удалить дубль на обоих коллерах. Реализатор сверяет, что диапазон берётся
  из того же источника (config/парт-состояние), что и у коллеров.
- [ ] Step 3: gates: clippy; cargo test; rt_safety.
- [ ] Step 4: коммит `fix: pitch_bend_range inside load_keys_soundfont; document skip_samples cross-part drop (audit data #F4,#F5)`.

### Task 3: F12+F13 — cc_map: warn stale + refresh ranges

**Files:** Modify `src/cc_map.rs:160-184` (from_config).

- [ ] Step 1: F12 — в ветке else resolve_key fail:
```rust
                } else {
                    // F12: stale param_key (параметр удалён из registry) —
                    // дефолтный бинд воскресал молча (audit data #F12).
                    log::warn!("cc_map: stale param_key {:?} for CC{} — dropped", raw_binding.param_key, cc);
                }
```
- [ ] Step 2: F13 — при успешном resolve использовать range ИЗ REGISTRY
  (review #5: поля ParamMeta — `min`/`max` + logarithmic, cc_map.rs:17-18;
  НЕ min_val/max_val — это поля CcBinding; fallback на raw_binding мёртв —
  resolve==Some влечёт meta==Some, поэтому unwrap напрямую):
```rust
                    let meta = find_param_meta(static_key)
                        .expect("resolve_key==Some влечёт meta==Some (один реестр)");
                    let scope = meta.scope;
                    // F13: min/max/logarithmic обновляются из registry SSOT
                    // (сохранённые значения морозились на момент learn —
                    // смена range в registry не доезжала; audit data #F13).
                    let (min_val, max_val, logarithmic) =
                        (meta.min, meta.max, meta.logarithmic);
```
  и использовать их в CcBinding.
- [ ] Step 3: тест в mod tests cc_map.rs: бинд с искажённым range (min/max
  от балды) на валидный ключ → from_config вернул registry range; stale ключ
  → дефолтный CC остаётся с дефолтным биндом (существующее поведение).
- [ ] Step 4: gates + коммит `fix: cc_map — warn stale keys, refresh ranges from registry (audit data #F12,#F13)`.

### Task 4: F14+F15 — keybinds: warn unknown + round-trip без "?"

**Files:** Modify `src/gui/keybinds.rs` (from_config filter_map 22-27,
to_config 53-57, key_to_str 123-125) + 3 display-коллера
gui/mod.rs ~930/~2033/~2065.

- [ ] Step 1: F14 — filter_map с логом (фактический парс = key_from_str(k) +
  KeyAction::from_str(action), различать что именно невалидно):
```rust
        .filter_map(|(k, action)| {
            let key = match key_from_str(k) { Some(k) => k, None => {
                log::warn!("keybinds: unknown key {k:?} — entry dropped"); return None; } };
            match KeyAction::from_str(action) {
                Some(a) => Some((key, a)),
                None => { log::warn!("keybinds: unknown action {action:?} — entry dropped"); None }
            }
        })
```
  (под фактическую форму 22-26 — реализатор сохраняет типы.)
- [ ] Step 2: F15 — `key_to_str(key) -> Option<&'static str>` (None для ключей
  вне KEY_TABLE; &'static str, НЕ String — таблица статическая). Обновить 3
  display-коллера (gui/mod.rs ~930/~2033/~2065): там "?" fallback УМЕСТЕН —
  `.unwrap_or("?")` на месте. to_config: None → пропуск записи +
  `log::warn!("keybinds: key for {action:?} outside KEY_TABLE — not persisted")`.
  (detect_key_press ограничен KEY_TABLE — ветка сегодня недостижима, фикс =
  защита pub API binds_mut.)
- [ ] Step 3: новый `mod tests` в keybinds.rs: to_config с биндом на
  egui::Key::Comma (существует в egui 0.31, в KEY_TABLE отсутствует —
  review #9) через pub binds_mut → запись Comma отсутствует в конфиг-мапе,
  валидные бинды не потеряны.
- [ ] Step 4: gates + коммит `fix: keybinds — warn unknown entries, no "?" round-trip poison (audit data #F14,#F15)`.

### Task 5: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens
  (`git status tests/golden` пустой).
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.

## Out of scope
- F3 (sustain после паники) — УБРАН по ревью round-2 (set_sustain(0) зовётся ДО all_notes_off на всех 4 паник-путях: snapshot-подход no-op; правка семантики = юзер-решение «паника отпускает педаль или сохраняет её»). Вынесено в TECHDEBT.
- F1 (browser program) — сделан в QW3 #106.
- F6-F9 (perf restore logs — сделано в QW3 Task 14; dual persistence —
  юзер-решение; block_size — сделано QW3).
- F10/F-clean пункты — не дефекты.
