# Golden hermeticity: preset_hashes отрывается от живых каталогов Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD d351ce9; re-checked на 8bed920 — все якоря
действительны, смещения нет)

**Goal:** tests/golden_fx.rs перестаёт зависеть от живых пользовательских
каталогов пресетов (~/.local/share, ~/.config) — любой эдит юзер-пресетов
больше не краснит golden без реальной регрессии (инцидент QW3 #106: 40-строчный
regen чуть не замаскировал настоящий регресс).

**Architecture:** `load_all_patches()` расщепляется на core-функцию
`load_all_patches_from(ext_dir: Option<&Path>, user_dir: Option<&Path>)`
(factory + явные каталоги) + существующую обёртку с реальными каталогами
(поведение рантайма 1:1). Все 4 вызова load_all_patches в tests/golden_fx.rs
переходят на core-функцию с фикстурой tests/golden/fixtures/external (2 JSON:
`golden_fixture_saw.json` с name "Golden Fixture Saw" и `chorus_pad.json` с
name "Chorus Pad" — точный дубль заводского «Chorus Pad», даёт collision-суффикс
"#2" под golden-контролем; имена файлов = canonical stems по
sanitize_preset_filename, иначе migrate_preset_filename переименует их прямо
в коммитнутом каталоге при прогоне тестов). preset_hashes.txt перегенерируется
один раз: полный набор = 283 factory + 2 фикстуры + "#2" ≈ 286 строк
(осознанное сужение с ~900 живых), это тест-инфра change, НЕ sound golden.

**Tech Stack:** Rust; регенерация через существующий MINI_MIDI_GOLDEN_UPDATE=1;
это ТЕСТ-ИНФРА change, не sound golden — регенерация канонична.

**Проверенные факты (HEAD d351ce9):**
- src/preset.rs:562-571: `load_factory_patches()` — без файловой системы,
  machine-independent (комментарий это и заявляет).
- src/preset.rs:574-587: `load_all_patches()` = factory +
  `load_presets_from_dir(&external_preset_dir(), ..)` + user_preset_dir().
- src/preset.rs:501-505: `user_preset_dir()` = app_config_dir()/presets;
  549-551: `external_preset_dir()` = app_data_dir()/presets — реализатор
  сохраняет это различие в core-вызове.
- tests/golden_fx.rs:4 вызова `preset::load_all_patches()`: 240
  (engine_preset_render_bit_deterministic), 263 (golden_preset_hashes_match),
  301 (golden_engine_fx_chain_slots), 336 (третий тест — имя прочитать на
  месте).
- tests/golden_fx.rs:269-278: collision-суффикс `#2` для дублей имён — фикстура
  должна сохранить покрытие этой ветки.
- tests/golden_fx.rs:47: `golden_path(name)` — база для путей goldens;
  фикстурный каталог строить от `env!("CARGO_MANIFEST_DIR")`.
- Прецедент регенерации с осознанным diff: QW3 #106 (preset_hashes, 40 строк).

---

## Test Cases (from the requirement)

- TC1: `load_all_patches_from(None, None)` == `load_factory_patches()` —
  точное равенство длины и порядка имён.
- TC2: `load_all_patches_from(Some(fixture), None)` (как в хелпере
  hermetic_patches) — factory (283) + 2 фикстуры; «Chorus Pad»-дубль получает
  суффикс #2; имена стабильны между прогонами.
- TC3: golden-тесты зелёные на грязной машине-эмуляции: мусорный пресет в
  fakehome + `env -u XDG_CONFIG_HOME -u XDG_DATA_HOME HOME=/tmp/fakehome
  cargo test --test golden_fx` → зелёный БЕЗ регенерации. ПОЗИТИВНЫЙ
  КОНТРОЛЬ (до Task 2): тот же fakehome-прогон на СТАРОМ коде → RED (доказывает,
  что утечка существовала и тест её видит). dirs-6.0: config_dir =
  XDG_CONFIG_HOME else $HOME/.config; data_dir = XDG_DATA_HOME else
  $HOME/.local/share — поэтому -u XDG_* обязателен.
- TC4: рантайм-поведение не изменилось: `load_all_patches()` по-прежнему
  читает реальные каталоги (проверка — существующие контракт-тесты preset
  collisions зелёные).

## Negative Matrix

- Фикстурный каталог отсутствует (свежий чекаут без fixtures? — нет,
  fixtures коммитятся) → load_presets_from_dir на отсутствующем пути =
  пусто, заводские остаются — тест зелёный (деградация покрытия, не красный).
- Дубль имени фикстуры с заводским — суффикс #2 (TC2 покрывает).
- External dir = user dir (одинаковый путь дважды) — пресеты прочитаются
  дважды → двойной суффикс #2/#3 — детерминированно; в тесте не используем.
- ~~~timeout/clock~~ — n/a. ~~~конкурентность~~ — cargo test изолирован.
- JSON фикстуры битый → load_presets_from_dir пропускает (существующая
  семантика) — не красный.

---

### Task 1: preset.rs — core-функция + обёртка

**Files:** Modify `src/preset.rs:574-587`.

- [ ] Step 1: failing-тест (в существующий mod tests preset.rs или tests/
  контракт-файл — по месту соседних preset-тестов):
```rust
    #[test]
    fn load_all_patches_from_none_equals_factory() {
        let a = preset::load_all_patches_from(None, None);
        let b = preset::load_factory_patches();
        assert_eq!(a.len(), b.len());
        assert!(a.iter().map(|p| &p.name).eq(b.iter().map(|p| &p.name)));
    }
```
  (если тесты в src/preset.rs mod tests — путь `load_all_patches_from` без
  `preset::`.)
- [ ] Step 2: рефактор. ВАЖНО (review C1): `load_presets_from_dir` принимает
  `&PathBuf` (preset.rs:509) — расширить её сигнатуру до `&Path`
  (behavior-preserving; внутренние вызовы `&PathBuf` коерсятся автоматически),
  и добавить `use std::path::Path` (сейчас импортирован только PathBuf):
```rust
/// Core: factory + явно указанные каталоги (None = пропустить). Герметичный
/// доступ для тестов: тесты не должны зависеть от живых пользовательских
/// каталогов (инцидент QW3 #106 — preset_hashes краснел от эдитов юзера).
pub fn load_all_patches_from(ext_dir: Option<&Path>, user_dir: Option<&Path>) -> Vec<Patch> {
    let mut presets = load_factory_patches();
    if let Some(d) = ext_dir {
        load_presets_from_dir(d, &mut presets);
    }
    if let Some(d) = user_dir {
        load_presets_from_dir(d, &mut presets);
    }
    presets
}

/// Load all available presets: factory + external + user.
pub fn load_all_patches() -> Vec<Patch> {
    let mut presets = load_factory_patches();
    load_presets_from_dir(&external_preset_dir(), &mut presets);
    if let Some(dir) = user_preset_dir() {
        load_presets_from_dir(&dir, &mut presets);
    }
    presets
}
```
  (`use std::path::Path` — проверить существующие imports.) ВАЖНО: реализатор
  читает 495-510 и сохраняет РАЗЛИЧИЕ external vs user каталогов как в
  текущем коде.
- [ ] Step 3: `cargo test load_all_patches_from` PASS; полный `cargo test`
  (контракт-тесты preset collisions зелёные = рантайм не изменился).
- [ ] Step 4: коммит `refactor: load_all_patches_from — hermetic core for tests (behavior-preserving)`.

### Task 2: golden_fx.rs — фикстура + перевод 4 вызовов

**Files:** Create `tests/golden/fixtures/external/golden_fixture_saw.json` +
`chorus_pad.json`; Modify `tests/golden_fx.rs` (4 вызова: 240, 263, 301, 336
— имена тестов прочитать на месте; устаревший комментарий о «Clarinet»
коллизии 265-268 переписать под фикстурный «Chorus Pad»-дубль).

- [ ] Step 0 (ПОЗИТИВНЫЙ КОНТРОЛЬ на СТАРОМ коде, до любых правок —
  доказывает утечку; тот же прогон после Task 2 станет зелёным и смысла уже
  не докажет):
```bash
mkdir -p /tmp/fakehome/.local/share/mini_midi_synth/presets
cp presets/surge_*.json /tmp/fakehome/.local/share/mini_midi_synth/presets/ 2>/dev/null || true
env -u XDG_CONFIG_HOME -u XDG_DATA_HOME HOME=/tmp/fakehome cargo test --test golden_fx golden_preset_hashes_match
```
  → ожидаемо RED. Записать факт в отчёт (dirs-6.0 уважает XDG_* поверх HOME,
  поэтому -u обязателен).

- [ ] Step 1: фикстуры — 2 валидных JSON (структуру списать с заводского из
  FACTORY_PRESETS; оба с `category: "Golden"`):
  - `golden_fixture_saw.json` — name `"Golden Fixture Saw"`;
  - `chorus_pad.json` — name `"Chorus Pad"` (точный дубль заводского «Chorus
    Pad» — collision-суффикс #2; заводского name «Chorus» не существует,
    review I1).
  ИМЕНА ФАЙЛОВ = canonical stems (sanitize_preset_filename: lowercase,
  non-alnum → `_`), иначе migrate_preset_filename (preset.rs:527) переименует
  файлы в коммитнутом каталоге при первом прогоне (review I4).
- [ ] Step 2: в golden_fx.rs — хелпер рядом с golden_path:
```rust
fn hermetic_patches() -> Vec<preset::Patch> {
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden/fixtures/external");
    preset::load_all_patches_from(Some(fixture.as_path()), None)
}
```
  (`Some(fixture.as_path())` — Option<&PathBuf> НЕ коерсится в Option<&Path>,
  review C1.) Заменить 4 вызова `preset::load_all_patches()` →
  `hermetic_patches()`. Тест 240: фильтр по keys остаётся рабочим на
  заводских; фолбэк `subset.is_empty()` не сработает.
- [ ] Step 3: `cargo test --test golden_fx` → preset_hashes RED (набор
  сузился с ~900 до ~286) — это ОЖИДАЕМЫЙ red этого шага.
- [ ] Step 4: регенерация: `MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx`;
  diff tests/golden/preset_hashes.txt: полный набор = 283 factory + 2
  фикстуры + «Chorus Pad#2» ≈ 286 строк (review I2 — НЕ ~40!); В ДИФФЕ НЕ
  ДОЛЖНО быть factory-хэшей с ИЗМЕНЁННЫМИ значениями (factory хэши либо
  исчезают из диффа, либо идентичны старым — если какой-то factory hash
  изменился = реальная проблема, STOP).
- [ ] Step 5: gates: `cargo clippy --all-targets -- -D warnings`; `cargo test`;
  `cargo test --test golden_fx` зелёный.
- [ ] Step 6: коммит `test: hermetic preset goldens — fixture dir instead of live user dirs (machine-independent)`.

### Task 3: Верификация герметичности

- [ ] Step 1 (повтор прогока из Task 2 Step 0, уже на НОВОМ коде): тот же
  fakehome-прогон → зелёный БЕЗ регенерации. Существующие тесты preset
  collisions (save_patch_in, tempdir-based) не задеты.
- [ ] Step 2: полный гейт: clippy / cargo test / rt_safety / `git status tests/golden` чистый.
- [ ] Step 3: PR-описание: «регенерация preset_hashes = тест-инфра change
  (набор сужен до 283 factory + fixture ≈ 286 строк), НЕ sound change».

## Out of scope
- Аналогичная герметизация других файловых goldens (drum kits, performances)
  — у них нет живых каталогов в тестах на текущий момент; расширять по мере
  появления.
- Перенос preset-каталогов на env-переменные в production — не нужно,
  core-функции достаточно.
