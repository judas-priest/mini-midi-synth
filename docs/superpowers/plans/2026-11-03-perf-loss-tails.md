# Perf-Loss Tails (TECHDEBT #49) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-03 (база = master c671957 = origin/master; все «было»-сниппеты вычитаны из текущего кода в этом сеансе; аудит #49 от 2026-11-03 в docs/TECHDEBT.md §49)

**Scope (из код-аудита #49):** хвосты 1, 2, 3, 5. Хвост 4 (SF2 bank) — Out of scope
(потери нет: bank-UI не существует). Хвост 6 (key ranges) — by design, не баг.

**Goal:** Устранить 4 тихие потери состояния перфов: рассинхрон GUI-зеркала
macro_vals (главный симптом «после reconnect уезжает»), молчаливый fallback на
патч 0 при промахе patch_name, рестарт возвращает до-undo патч, потеря имён
макросов и XY-биндингов.

**Architecture:** Источник правды для macro_vals — `edited_params` (ключи
`macro_{0..7}`); GUI-зеркало `parts[i].macro_vals` синхронизируется из params
чистой функцией `macro_vals_from_params` в двух местах (load перфа, undo
парта). patch_name-промах — warn на load и save. Undo парта 0 — last_preset
через существующий `save_config()`. Персист имён/XY — три новых
`#[serde(default)]`-поля в `PartConfig` (обратно совместимо: старые перфы
парсятся, поля не пишутся когда дефолтные).

**Tech Stack:** Rust, egui 0.31.1, serde. Работа в отдельном worktree, интеграция
через PR → squash-merge (workflow: GitHub — точка сериализации, локальный merge
в master запрещён).

**Проверенные чтением кода факты (не пересматривать):**
- `Part` (GUI): `macro_vals: [f32; 8]` (gui/mod.rs:164), `macro_names: Vec<String>`
  (:166, дефолт «Macro 1..8» :182), `xy_macro_x: usize` (:168, дефолт 0),
  `xy_macro_y: usize` (:170, дефолт 1 :184).
- Движок получает макросы из `edited_params`: клавиша `macro_{i}` пишется при
  движении слайдера/XY (gui/macros.rs:54-58, 222, 229);
  `send_edited_params` → `load_patch_from_edited` → `PatchParams::from_map`
  парсит `macro_0..7` (patch_params.rs:365-368) → `LoadPatch` применяет
  `macro_vals` (synth/mod.rs:2497). Значит синхронизация зеркала из
  edited_params чинит и load, и undo.
- `apply_performance` (gui/mod.rs:2770-2841): в цикле
  `self.parts[i].edited_params = part.param_overrides.clone();` (:2799) —
  зеркала macro_vals НЕ касается. `push_full_part_state` (:2325) пушит
  `SetMacro` из зеркала — при audio-reconnect перезаписывает движок stale-миром.
- `restore_part_snapshot` (gui/mod.rs:2259-2275): восстанавливает
  edited_params/patch_idx/mute/pan/transpose, зовёт `send_edited_params`,
  `persist_parts_state`, `set_active_part`; зеркало macro_vals НЕ трогает;
  `config.ui.last_preset` НЕ откатывает.
- `save_config` (gui/mod.rs:2613-2618): last_preset = имя патча `parts[0]`.
- `save_global_to_config` (:1999-2015) / `persist_parts_state` (:2725-2736):
  last_preset не пишут.
- patch_name lookup: gui/mod.rs:2793-2796
  `position(|p| p.name == part.patch_name).unwrap_or(0)` — без лога.
- Perf save (gui/layers.rs:196-213):
  `self.patches.get(l.patch_idx).map(|p| p.name.clone()).unwrap_or_default()`
  — пустое имя сохраняется молча; литерал PartConfig :199-212.
- `PartConfig` (preset.rs:742-780): `#[derive(Clone, Serialize, Deserialize)]`,
  поля с `#[serde(default)]` — паттерн для новых полей есть. МЕСТА
  КОНСТРУИРОВАНИЯ ЛИТЕРАЛОМ (PartConfig без Default, компилятор заставит
  дополнить ВСЕ): preset.rs:849 (init_performance), gui/layers.rs:199
  (perf save), tests/perf_load_seq.rs:8 (хелпер part()), tests/sustain_audit.rs:428
  (тест performance_part_config_roundtrip).
- Стартап: lib.rs:322-327 last_preset → patch_idx `unwrap_or(0)`;
  startup-restore перфа: gui/mod.rs:1974 (last_performance).
- Существующие тесты перфов: tests/perf_load_seq.rs (serde roundtrip-паттерны,
  `missing_optional_fields_get_defaults` — образец для legacy-теста).
- Старые перфы на диске (~/.config/mini_midi_synth/performances/*.json) НЕ
  содержат новых ключей — `#[serde(default)]` обязателен.

---

## Test Cases (from the requirement)

Требование: перф грузится/undo-ится/рестартится — ничего не теряет тихо.

- TC1 (хвост 1): params `{"macro_0":0.25, "macro_3":0.9}` →
  `macro_vals_from_params` = `[0.25,0,0,0.9,0,0,0,0]`; пустые params → все 0.0;
  лишние/чужие ключи игнорируются.
- TC2 (хвост 5, serde): PartConfig с macro_names=["Kick","Snare"], xy=(2,3) →
  JSON-кругтрип сохраняет все три поля.
- TC3 (хвост 5, legacy): JSON без macro_names/xy_macro_x/xy_macro_y →
  десериализуется: names=[], x=0, y=1 (y-дефолт единица — как PartState::new_empty).
- TC4 (хвост 2): экран — perf с переименованным/удалённым патчем: warn в логе,
  приложение не падает, парт играет патч 0 (осознанно); save парта с
  битым patch_idx — warn.
- TC5 (хвост 1, экран): загрузить перф с сохранёнными макросами — слайдеры
  макросов показывают сохранённые значения; подвигать макро-слайдер/XY (drag)
  → Ctrl+Z — откат к значению до жеста (undo-снапшот на drag_started,
  Task 2 Step 4; без него undo — no-op и пункт бессмыслен).
- TC6 (хвост 3, экран, регрессионный): A → B (патч парта 0) → Ctrl+Z →
  ПРОВЕРИТЬ конфиг: `grep last_preset ~/.config/mini_midi_synth/config.json`
  → "A" (до фикса было "B"). Затем ПЕРЕИМЕНОВАТЬ файл активного перфа в
  performances/ (иначе рестарт восстанавливает патч ИЗ ПЕРФА, минуя
  last_preset — фикс ненаблюдаем) → рестарт → парт 0 = A.

## Negative Matrix

- пустой `macro_names` в PartConfig — skip_serializing_if, restore
  регенерирует дефолты «Macro 1..8» (как PartState::new_empty), не пустые строки.
- macro_names короче 8 из рукописного JSON — безопасно: все чтения через
  `.get()` (macros.rs:25, 39, 81).
- xy-индекс ≥8 в битом JSON — clamp `.min(7)` при restore (иначе panic при
  прямом индексе `macro_labels[xy_macro_x]`, macros.rs:94/109).
- перф длиннее 16 партов — существующий guard `if i >= self.parts.len() { break; }`.
- undo НЕ парта 0 — last_preset не трогается (он зеркалит только parts[0] by design).
- last_preset указывает на несуществующий патч — существующий `unwrap_or(0)`
  на старте (вне scope; только warn из TC4 покрывает perf-путь).
- config.save() промахнулся (диск) — существующий `let _ =` паттерн, не меняем.
- дубликат patch_name в списке — `position` берёт первый, существующее поведение.
- macro_{i} = 0.0 vs отсутствие ключа — оба дают mirror 0.0 (неразличимы и
  неразличивать не нужно: движок получает одно и то же).

---

### Task 1: `macro_vals_from_params` — чистая функция + тесты (TDD)

**Files:**
- Modify: `src/gui/mod.rs` (free fn рядом с App, тесты в `#[cfg(test)]`-модуле
  внизу файла — рядом с `part_undo_tests`)

- [ ] **Step 1: падающий тест** — в `src/gui/mod.rs` добавить новый
  `#[cfg(test)] mod macro_mirror_tests { ... }` на уровне модуля (рядом с
  существующим `mod part_undo_tests` в конце файла):

```rust
#[cfg(test)]
mod macro_mirror_tests {
    use super::*;

    #[test]
    fn tc1_macro_mirror_from_params() {
        let mut params = std::collections::BTreeMap::new();
        params.insert("macro_0".to_string(), 0.25);
        params.insert("macro_3".to_string(), 0.9);
        params.insert("unrelated".to_string(), 5.0);
        assert_eq!(
            macro_vals_from_params(&params),
            [0.25, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0, 0.0]
        );
        assert_eq!(
            macro_vals_from_params(&std::collections::BTreeMap::new()),
            [0.0f32; 8]
        );
    }
}
```

- [ ] **Step 2: прогнать, убедиться в падении**

Run: `cargo test --lib tc1_macro_mirror 2>&1 | tail -5`
Expected: FAIL — `macro_vals_from_params` не существует.

- [ ] **Step 3: реализация** — в `src/gui/mod.rs` на уровне модуля (вне impl
  App, например сразу после импортов):

```rust
/// GUI-зеркало macro_vals из edited_params (источник правды — params,
/// зеркало синхронизируется при load перфа и undo; #49: без этого
/// push_full_part_state при audio-reconnect перезаписывал движок stale-миром).
pub(crate) fn macro_vals_from_params(
    params: &std::collections::BTreeMap<String, f32>,
) -> [f32; 8] {
    let mut out = [0.0f32; 8];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = params.get(&format!("macro_{i}")).copied().unwrap_or(0.0);
    }
    out
}
```

- [ ] **Step 4: прогнать — зелёный**

Run: `cargo test --lib tc1_macro_mirror 2>&1 | tail -3`
Expected: `test result: ok` (на этом коммите функция ещё не используется вне
тестов — не-clippy-clean промежуточное состояние допустимо; Task 2
подключит её и закроет clippy-гейт)

- [ ] **Step 5: коммит**

```bash
git add src/gui/mod.rs && git commit -m "feat: macro_vals_from_params — pure mirror derivation for #49"
```

### Task 2: синхронизация зеркала в load и undo + undo-снапшот макро-драга + last_preset при undo (хвосты 1+3)

**Files:** Modify: `src/gui/mod.rs` — apply_performance (~:2799),
restore_part_snapshot (~:2255-2271); `src/gui/macros.rs` — слайдер (~:48),
XY-пад (~:213)

- [ ] **Step 1: apply_performance** — после строки
  `self.parts[i].edited_params = part.param_overrides.clone();` вставить:

```rust
            // #49: зеркало макросов следует за params, иначе GUI показывает
            // stale, а push_full_part_state (audio-reconnect) перезапишет
            // движок старыми значениями через SetMacro
            self.parts[i].macro_vals = macro_vals_from_params(&self.parts[i].edited_params);
```

- [ ] **Step 2: restore_part_snapshot** — ИТОГОВОЕ СОСТОЯНИЕ блока показано
  ниже; единственная ДОБАВЛЯЕМАЯ строка — `p.macro_vals = ...` (остальное —
  существующий код :2256-2264, НЕ дублировать):

```rust
        let PartUndoSnapshot { part, edited_params, patch_idx, mute, pan, transpose } = s;
        {
            let p = &mut self.parts[part];
            p.edited_params = edited_params;
            p.patch_idx = patch_idx;
            p.mute = mute;
            p.pan = pan;
            p.transpose = transpose;
            // #49: undo откатывает и macro_{i} в edited_params — зеркало
            // обязано последовать, иначе последующий push перезапишет откат
            p.macro_vals = macro_vals_from_params(&p.edited_params);
        }
```

- [ ] **Step 3: last_preset при undo парта 0** — в конце
  `restore_part_snapshot`, после `self.persist_parts_state();` и ДО
  `self.set_active_part(part);` вставить:

```rust
        // #49: last_preset зеркалит parts[0]; undo патча парта 0 иначе
        // оставляет конфиг на до-undo патче — рестарт вернёт откаченное
        if part == 0 {
            self.save_config();
        }
```

- [ ] **Step 4: undo-снапшот на макро-драг** — сейчас макро-панель не пушит
  undo-снимок ВООБЩЕ (в macros.rs нет push_undo_snapshot*): TC5 «после undo —
  значения до правки» невыполним (стек пуст → undo no-op). Канон кодбазы —
  снимок на drag_started (params.rs:299/349/601...); no-duplicate guard
  уже есть (mod.rs:2218-2228). В `src/gui/macros.rs`:

  Слайдер (:48-53) — сохранить response и добавить пуш; было:

```rust
                        let mut v = val;
                        if ui.add(
                            egui::Slider::new(&mut v, 0.0..=1.0)
                                .show_value(true)
                                .text("")
                        ).on_hover_text("Macro modulation value").changed() {
```

  стало:

```rust
                        let mut v = val;
                        let resp = ui.add(
                            egui::Slider::new(&mut v, 0.0..=1.0)
                                .show_value(true)
                                .text("")
                        ).on_hover_text("Macro modulation value");
                        // undo-канон кодбазы: один шаг на жест, снимок на старте
                        if resp.drag_started() {
                            self.push_undo_snapshot_for(part);
                        }
                        if resp.changed() {
```

  XY-пад (:213-215) — перед `if response.dragged() || response.clicked() {`
  вставить:

```rust
        // undo-снимок на старте жеста (один шаг на драг; guard от дублей в
        // part_undo_push)
        if response.drag_started() {
            self.push_undo_snapshot_for(part);
        }
        if response.dragged() || response.clicked() {
```

- [ ] **Step 5: clippy + тесты**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --lib 2>&1 | tail -3`
Expected: чисто, тесты зелёные.

- [ ] **Step 6: коммит**

```bash
git add src/gui/mod.rs src/gui/macros.rs && git commit -m "fix: macro mirror sync on perf load/undo, undo snapshot on macro drag, last_preset rollback on undo of part 0 (#49)"
```

### Task 3: warn при patch_name-промахе (load и save) — хвост 2

**Files:** Modify: `src/gui/mod.rs` (~:2793), `src/gui/layers.rs` (~:196-199)

- [ ] **Step 1: load-сторона** — заменить

```rust
            let patch_idx = self.patches.iter()
                .position(|p| p.name == part.patch_name)
                .unwrap_or(0);
```

на

```rust
            let patch_idx = self.patches.iter()
                .position(|p| p.name == part.patch_name);
            if patch_idx.is_none() && !part.patch_name.is_empty() {
                // #49: тихий fallback на патч 0 = «партия играет чужое» без
                // единого следа в логе; поведение сохраняем (патч 0), но
                // диагностируем
                log::warn!(
                    "perf '{}': part {}: patch '{}' not found — falling back to patch 0",
                    perf.name, i, part.patch_name
                );
            }
            let patch_idx = patch_idx.unwrap_or(0);
```

- [ ] **Step 2: save-сторона** — в `draw_perf_row` (gui/layers.rs) цепочка
  `.iter().map(|l| { ... })` НЕ имеет индекса — заменить на `.enumerate()` и
  добавить warn. Было:

```rust
                let parts: Vec<PartConfig> = self.parts.iter().map(|l| {
                    let patch_name = self.patches.get(l.patch_idx)
                        .map(|p| p.name.clone()).unwrap_or_default();
```

стало:

```rust
                let parts: Vec<PartConfig> = self.parts.iter().enumerate().map(|(i, l)| {
                    let patch_name = self.patches.get(l.patch_idx)
                        .map(|p| p.name.clone()).unwrap_or_default();
                    if patch_name.is_empty() {
                        // #49: пустое имя (unwrap_or_default) никогда не
                        // сматчится при load — сохранение невосстановимо.
                        // Load-сторона warn-ует только на НЕпустые промахи
                        // (guard !is_empty — legacy/disabled парты без спама),
                        // поэтому здесь сообщение самодостаточное
                        log::warn!(
                            "perf save '{}': part {} patch_idx {} out of range — saved with empty patch_name",
                            self.perf_name, i, l.patch_idx
                        );
                    }
```

- [ ] **Step 3: clippy + тесты**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: clippy чисто, счётчик ok-строк > 0, ни одной строки `failed` без ` 0 failed`.

- [ ] **Step 4: коммит**

```bash
git add src/gui/mod.rs src/gui/layers.rs && git commit -m "fix: warn on patch_name miss in perf load and save (#49) — silent wrong-patch fallback is now diagnosable"
```

### Task 4: персист macro_names + XY-биндингов (хвост 5, serde TDD)

**Files:**
- Modify: `src/preset.rs` — PartConfig три поля + дефолт-хелпер; литерал :849
- Modify: `src/gui/layers.rs` — литерал PartConfig в draw_perf_row
- Modify: `src/gui/mod.rs` — restore в apply_performance
- Test: `tests/perf_load_seq.rs` — TC2/TC3

- [ ] **Step 1: падающие тесты** — в `tests/perf_load_seq.rs`. Файл уже
  импортирует `PartConfig` (`use mini_midi_synth::preset::{self, PartConfig, Performance};`)
  и имеет локальный хелпер `fn part(patch_name, enabled) -> PartConfig` —
  КОМПИЛЯТОР заставит дополнить его тремя новыми полями (PartConfig без
  Default): добавь в литерал хелпера
  `macro_names: Vec::new(), xy_macro_x: 0, xy_macro_y: 1,`.
  Затем рядом с существующими тестами добавить (см. также Step 3 — там
  четвёртое место конструирования, tests/sustain_audit.rs:428):

```rust
#[test]
fn macro_names_and_xy_roundtrip() {
    let mut p = part("Init", true);
    p.macro_names = vec!["Kick".into(), "Snare".into()];
    p.xy_macro_x = 2;
    p.xy_macro_y = 3;
    let json = serde_json::to_string(&p).unwrap();
    let back: PartConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.macro_names, vec!["Kick".to_string(), "Snare".to_string()]);
    assert_eq!(back.xy_macro_x, 2);
    assert_eq!(back.xy_macro_y, 3);
}

#[test]
fn legacy_part_without_macro_fields_gets_defaults() {
    // часть валидного PartConfig без новых ключей (как старые перфы)
    let legacy = r#"{"patch_name":"Init","enabled":true,"volume":0.8,"key_low":0,"key_high":127}"#;
    let p: PartConfig = serde_json::from_str(legacy).unwrap();
    assert!(p.macro_names.is_empty());
    assert_eq!(p.xy_macro_x, 0);
    assert_eq!(p.xy_macro_y, 1); // y-дефолт = 1, как PartState::new_empty (ось Y = Macro 2)
}
```

- [ ] **Step 2: прогнать, убедиться в падении** (E0560/unknown field)

Run: `cargo test --test perf_load_seq 2>&1 | tail -5`
Expected: FAIL — полей нет.

- [ ] **Step 3: поля в PartConfig** (preset.rs, после `sustain_enabled`):

```rust
    /// GUI-side macro labels for this part (global App-level in older code).
    /// Empty = client falls back to "Macro 1..8" defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub macro_names: Vec<String>,
    /// XY-pad X macro index (0-7). Default 0 = Macro 1.
    #[serde(default, skip_serializing_if = "usize_is_zero")]
    pub xy_macro_x: usize,
    /// XY-pad Y macro index (0-7). Default 1 = Macro 2 (как PartState::new_empty).
    #[serde(default = "default_xy_y", skip_serializing_if = "usize_is_one")]
    pub xy_macro_y: usize,
```

рядом с `fn default_true()` добавить:

```rust
fn usize_is_zero(v: &usize) -> bool { *v == 0 }
fn usize_is_one(v: &usize) -> bool { *v == 1 }
fn default_xy_y() -> usize { 1 }
```

Дополнить ВСЕ оставшиеся литералы PartConfig (компилятор перечислит, но для
полноты):
- `init_performance()` (preset.rs:849, после `sustain_enabled: true,`):
  `macro_names: Vec::new(), xy_macro_x: 0, xy_macro_y: 1,`
- **`tests/sustain_audit.rs:428`** (литерал `let orig = PartConfig { ... }` в
  тесте `performance_part_config_roundtrip`) — те же три поля.

serde_json в тестах доступен (Cargo.toml:35; perf_load_seq.rs:30 уже юзает).

- [ ] **Step 4: save** — литерал PartConfig в `draw_perf_row` (gui/layers.rs)
  дополнить:

```rust
                        macro_names: l.macro_names.clone(),
                        xy_macro_x: l.xy_macro_x,
                        xy_macro_y: l.xy_macro_y,
```

- [ ] **Step 5: restore** — в apply_performance (gui/mod.rs), рядом с
  зеркалом макросов из Task 2:

```rust
            // #49: имена макросов и XY-биндинги — GUI-стейт парта.
            // Пустой список (Init/legacy-перф) = дефолты «Macro 1..8»,
            // НЕ унаследованные имена от ПРЕДЫДУЩЕГО перфа
            self.parts[i].macro_names = if part.macro_names.is_empty() {
                (1..=8).map(|k| format!("Macro {k}")).collect()
            } else {
                part.macro_names.clone()
            };
            // clamp: битый JSON с индексом ≥8 не должен паниковать при
            // индексации macro_vals/макро-лейблов
            self.parts[i].xy_macro_x = part.xy_macro_x.min(7);
            self.parts[i].xy_macro_y = part.xy_macro_y.min(7);
```

- [ ] **Step 6: прогнать — зелёный, полный гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --test perf_load_seq 2>&1 | tail -3`
Expected: `test result: ok`, оба новых теста зелёные.

- [ ] **Step 7: коммит**

```bash
git add src/preset.rs src/gui/layers.rs src/gui/mod.rs tests/perf_load_seq.rs tests/sustain_audit.rs
git commit -m "feat: persist per-part macro names and XY bindings in performances (#49) — serde-default, backward compatible"
```

### Task 5: Верификация

- [ ] Полный гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] `cargo run --release` — экран TC4/TC5/TC6:
  - загрузить перф с макросами → слайдеры макросов показывают сохранённое;
  - макро-драг → Ctrl+Z → откат (TC5);
  - patch_name-промах (правкой JSON вписать несуществующее имя) → warn в
    stderr, приложение живо (TC4);
  - TC6 по шагам из секции Test Cases (grep last_preset + переименовать
    перф-файл перед рестартом);
  - сохранить перф с переименованным макросом → перезапуск → имя на месте.
- [ ] PR → squash merge → перезапуск release из main checkout
- [ ] TECHDEBT #49: пометить хвосты 1/2/3/5 ГОТОВО (4 — out of scope, 6 — by
  design), план status: executed

## Out of scope

- SF2 bank в перфах (хвост 4) — нет bank-UI, потерь нет; делать вместе с bank-UI.
- Undo имён макросов/XY (PartUndoSnapshot не несёт macro_names/xy) — осознанное
  упрощение: Ctrl+Z откатывает значения макросов, но не их переименования.
- Известная морщина вне скоупа: apply_performance:2785 зовёт save_config() ДО
  цикла — в конфиг уходит last_preset до-лоад патча. Не баг (успешный перф-рестор
  перекрывает last_preset на старте), но персист производного состояния после
  изменений был бы чище — при случае.
- Дебаунс config-записей: undo парта 0 вызывает save_config() синхронно на
  каждый Ctrl+Z (в т.ч. mute/pan/transpose-undo). Существующий global_dirty +
  flush 2s (mod.rs:691) save_config обходит. Терпимо (клик патча уже пишет так
  же); унификация дебаунса — отдельная работа.
- key_low/high в перфах (хвост 6) — by design (#46), не баг.
- Toast-уведомления при patch_name-промахе — log::warn достаточно (тост-инфра
  в GUI для этого пути не заведена).
