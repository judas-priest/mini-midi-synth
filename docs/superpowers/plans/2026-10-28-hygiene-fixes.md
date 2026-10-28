# Hygiene fixes (#2 eprintln, #3 preset collisions, #4 looper reconnect, #9 dead API) Implementation Plan

anchor-checked: 2026-10-28 (независимое ревью на master @ 329faac; находки вшиты)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Четыре находки Medium/Low из docs/deep-review-2026-10-28-audit-remediation.md: #2 eprintln в MIDI-потоках, #3 санитизация имён пресетов (коллизии/legacy-файлы), #4 reconnect теряет контент лупера, #9 мёртвый API `note_off_for`.

**Architecture:** #2 — `log::debug!` + init desktop-логгера (env_logger); #3 — точное сравнение `name` из существующего JSON (upsert только своего файла) через тестабельные `*_in(dir)`-варианты save-функций + миграция-переименование legacy-файлов при загрузке каталога; #4 — `LooperDisplay` получает `loop_length_ticks` (истина от движка, не реконструкция из bpm/bars), GUI снимает snapshot до drop старого бэкенда, zero-alloc `MidiLooper::restore` через `ControlEvent::LooperRestore` (Box-payload возвращается в trash); #9 — удалить мёртвый код (восстановимо из git, см. TECHDEBT #7).

**Tech Stack:** Rust, `log` + `env_logger` (desktop), serde, rtrb/trash-паттерн, tempfile (dev-dep).

**Обязательные проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требования: (a) повторное сохранение пресета с тем же именем обновляет тот же файл; (b) имена, различающиеся только пунктуацией/регистром, не перезаписывают друг друга — второе сохранение получает суффикс; (c) JSON в целевом файле с другим именем/без `name` не перезаписывается; (d) legacy-файл, чьё имя не совпадает с деривацией от `name` в его JSON, переименовывается при загрузке каталога; (e) лупер: restore переносит события и длину цикла в свежий движок; restore с cap на MAX_LOOP_EVENTS; пустой restore — no-op. Красная фаза: (b), (c), (d) — red до фикса; (a), (e)-cap/no-op — guard-тесты (зелёные до и после).

## Negative Matrix

- missing field — JSON в каталоге без поля `name`: десериализация fails → файл считается чужим → save получает суффикс (тест (c)).
- null — `name: null` → десериализация fails → тот же путь, что missing field (тест (c)).
- zero/empty — пустой каталог: save пишет без суффикса (тест (a)); пустой snapshot лупера: restore — no-op (тест (e2)).
- clock skew — неприменимо.
- duplicate/concurrent — двойной save подряд одного имени: второй — upsert того же пути (тест (a) guard).
- external-call timeout — неприменимо (fs::rename локальный).

---

### Task 1: Regression tests — presets (fresh subagent, plan-only context)

**Files:**
- Create: `tests/preset_collision.rs`
- Modify: `Cargo.toml` (dev-dependencies: tempfile)

Существование API: `preset::Patch` без `Default` — собирать тестовые патчи через `preset::load_factory_patches()[0].clone()` и править `name` (герметично: factory presets компилированы в бинарник, FS не читается). `save_patch` в текущем коде пишет только в реальный user-каталог (preset.rs:560-567) — тесты НЕ должны его звать; они тестируют новые `_in(dir)`-варианты из Task 2. До Task 2 тесты не компилируются — красная фаза здесь ограничена прогоном после Task 2 Step 1? Нет: TDD-порядок — Task 1 добавляет тесты, Task 2 делает их компилируемыми и зелёными; interim-прогон `cargo test --test preset_collision` ожидаемо падает компиляцией (это и есть красная фаза для несуществующего API).

- [ ] **Step 1: Add tempfile to dev-dependencies**

В `Cargo.toml`, секция `[dev-dependencies]` (создать, если нет):

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Write the tests**

```rust
//! Preset filename collision/legacy-migration contract (review finding #3).

use mini_midi_synth::preset;

fn test_patch(name: &str) -> preset::Patch {
    let mut p = preset::load_factory_patches()[0].clone();
    p.name = name.into();
    p
}

/// (a) guard: same name upserts the same file
#[test]
fn same_name_save_updates_same_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let p = test_patch("bass lead");
    let path1 = preset::save_patch_in(&p, Some(dir.clone())).unwrap();
    let path2 = preset::save_patch_in(&p, Some(dir.clone())).unwrap();
    assert_eq!(path1, path2, "same name must upsert the same file");
}

/// (b) punctuation/case-only distinct names must not silently overwrite
#[test]
fn punctuation_only_distinct_names_do_not_collide() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let a = test_patch("bass lead");
    let b = test_patch("bass-lead!");
    let pa = preset::save_patch_in(&a, Some(dir.clone())).unwrap();
    let pb = preset::save_patch_in(&b, Some(dir.clone())).unwrap();
    assert_ne!(pa, pb, "distinct names silently overwrote each other");
    let saved: preset::Patch =
        serde_json::from_str(&std::fs::read_to_string(&pa).unwrap()).unwrap();
    assert_eq!(saved.name, "bass lead", "first preset was overwritten");
}

/// (c) foreign/malformed JSON at the target path gets a suffix
#[test]
fn foreign_json_at_target_path_gets_suffix() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    std::fs::write(dir.join("bass_lead.json"), r#"{"not_a_patch": 1}"#).unwrap();
    let p = test_patch("bass lead");
    let path = preset::save_patch_in(&p, Some(dir.clone())).unwrap();
    assert_ne!(
        path.file_name().unwrap().to_str().unwrap(),
        "bass_lead.json",
        "save overwrote a foreign file"
    );
}

/// (d) legacy filename is migrated on directory load
#[test]
fn legacy_filename_migrated_on_load() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let body = serde_json::to_string(&test_patch("bass lead (user)")).unwrap();
    std::fs::write(dir.join("bass_lead_(user).json"), body).unwrap();
    let mut presets = Vec::new();
    preset::load_presets_from_dir(&dir, &mut presets);
    assert_eq!(presets.len(), 1);
    assert!(dir.join("bass_lead_user.json").exists(), "legacy file not renamed");
    assert!(!dir.join("bass_lead_(user).json").exists(), "legacy file left in place");
}
```

- [ ] **Step 3: Verify red**

Run: `cargo test --test preset_collision`
Expected: не компилируется — `save_patch_in`/`load_presets_from_dir` не существуют (красная фаза для нового API; логика коллизий будет проверена в Task 2).

- [ ] **Step 4: Commit (red)**

```bash
git add tests/preset_collision.rs Cargo.toml Cargo.lock
git commit -m "test: preset collision + legacy migration contracts (red)"
```

### Task 2: #3 — collision-aware save + legacy migration

**Files:**
- Modify: `src/preset.rs` — `save_json` (:41-48), новый `resolve_save_path`, новый `migrate_preset_filename`, `load_presets_from_dir` (:480-514 → pub), `save_patch` (:560-567 → `_in`-вариант), `save_performance` (:755), `save_drum_kit` (:587)

- [ ] **Step 1: Shared collision resolver + save_json**

Точное сравнение имён (НЕ sanitized-stem: «bass lead» и «bass-lead!» дают один stem, но это разные пресеты). После `sanitize_preset_filename` добавить:

```rust
/// Resolve the save path: upsert only a file owned by THIS preset (its stored
/// `name` equals ours exactly). Any other occupant — punctuation/case collision
/// or foreign JSON — forces a numbered suffix instead of a silent overwrite.
fn resolve_save_path(dir: &std::path::Path, name: &str) -> PathBuf {
    #[derive(Deserialize)]
    struct NameOnly { name: String }
    let stem = sanitize_preset_filename(name);
    let path = dir.join(stem.clone() + ".json");
    if !path.exists() { return path; }
    let same_owner = fs::read_to_string(&path).ok()
        .and_then(|c| serde_json::from_str::<NameOnly>(&c).ok())
        .map(|n| n.name == name)
        .unwrap_or(false);
    if same_owner { return path; }
    let mut n = 2;
    loop {
        let p = dir.join(format!("{stem}_{n}.json"));
        if !p.exists() { return p; }
        n += 1;
    }
}
```

(Не generic: достаточно прочитать из существующего файла только поле `name` — все сохраняемые типы сериализуют его без `skip_serializing_if` (Patch:83, DrumKit:575, Performance:741). Битый/чужой JSON → `NameOnly` не десериализуется → не owner → суффикс.)

Заменить `save_json`:

```rust
fn save_json<T: Serialize>(
    obj: &T,
    dir: Option<PathBuf>,
    name: &str,
) -> Result<PathBuf> {
    let dir = dir.context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let path = resolve_save_path(&dir, name);
    let json = serde_json::to_string_pretty(obj)?;
    crate::config::write_atomic(&path, &json)?;
    Ok(path)
}
```

Вызовы остаются совместимы по сигнатуре (`save_drum_kit`, `save_performance`) — правки не требуют.

- [ ] **Step 2: save_patch — testable `_in` variant + collision check**

Заменить `save_patch` (preset.rs:558-567):

```rust
/// Save a patch to an explicit directory (user dir by default).
pub fn save_patch_in(preset: &Patch, dir: Option<PathBuf>) -> Result<PathBuf> {
    let dir = dir.context("Could not determine config directory")?;
    fs::create_dir_all(&dir)?;
    let path = resolve_save_path(&dir, &preset.name);
    let json = serde_json::to_string_pretty(preset)?;
    crate::config::write_atomic(&path, &json)?;
    Ok(path)
}

/// Save a patch to the user directory.
pub fn save_patch(preset: &Patch) -> Result<PathBuf> {
    save_patch_in(preset, user_preset_dir())
}
```

(`#[allow(dead_code)]` снять — функция вызывается из settings.rs:470.)

- [ ] **Step 3: Legacy filename migration**

После `resolve_save_path` добавить (pub — вызывается из `load_presets_from_dir` и тестов):

```rust
/// Best-effort rename of a preset file whose stem no longer matches the
/// sanitized name stored in its JSON (legacy filenames from the old sanitizer
/// that kept `- + . ( )`). Repeat-saves then update the same file.
/// NOTE: applies to any scanned directory, including the external
/// (~/.local/share) one — hand-renamed files get canonicalized too.
pub fn migrate_preset_filename(path: &std::path::Path, name: &str) {
    let want = sanitize_preset_filename(name);
    let have = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
    if want == have { return; }
    // If the canonical stem is taken by another file, leave this one alone —
    // suffixing here (`want_2`, `want_3`, ...) would re-suffix on every load:
    // a file saved into a suffixed path by resolve_save_path must NOT migrate,
    // otherwise the suffix grows forever.
    if path.with_file_name(format!("{want}.json")).exists() { return; }
    let _ = fs::rename(path, path.with_file_name(format!("{want}.json")));
}
```

В `load_presets_from_dir` (preset.rs:480): сделать `pub` (`pub fn load_presets_from_dir(dir: &PathBuf, presets: &mut Vec<Patch>)`) и после успешного парса (после блока category, перед `.wt`-загрузкой, preset.rs:496) вставить:

```rust
                        // One-time legacy filename migration (see fn doc).
                        migrate_preset_filename(&path, &p.name);
```

Нюанс: миграция переименует и файлы, переименованные пользователем вручную нестандартно — осознанный trade-off (после первого запуска условие самопогашается). В `list_json` (:57-77, performances/kit) миграцию НЕ добавлять — после rename в листинге пришлось бы перечитывать актуальное имя файла, а главный субъект находки #3 — пресеты и повторное сохранение; коллизии kit/perf уже закрыты `resolve_save_path` (YAGNI). `list_json` остаётся без изменений.

- [ ] **Step 4: Run preset tests + roundtrip**

Run: `cargo test --test preset_collision && cargo test --test preset_roundtrip`
Expected: все зелёные; (b)/(c)/(d) зелёные (были red), (a) guard зелёный; roundtrip не сломан.

- [ ] **Step 5: Gates + commit**

```bash
cargo clippy --all-targets -- -D warnings && cargo test
git add src/preset.rs tests/preset_collision.rs
git commit -m "fix: preset saves never silently overwrite; migrate legacy filenames"
```

### Task 3: #9 — remove dead `note_off_for` API

**Files:**
- Modify: `src/synth/voice.rs:513-517` (метод), `src/synth/envelope.rs:162-176` (метод)

- [ ] **Step 1: Delete both methods**

Удалить `Voice::note_off_for` (voice.rs:515-517, с doc-комментарием) и `Envelope::note_off_for` (envelope.rs:162-176). Ноль вызовов по репо (проверено аудитом; перепроверить: `grep -rn "note_off_for" src/ tests/` — останутся только определения).

- [ ] **Step 2: Gates**

```bash
cargo clippy --all-targets -- -D warnings && cargo test
```
Expected: зелёные.

- [ ] **Step 3: Commit**

```bash
git add src/synth/voice.rs src/synth/envelope.rs
git commit -m "refactor: remove dead note_off_for API (revert remnant of 59ed413)"
```

### Task 4: #2 — eprintln → log::debug! + desktop logger init

**Files:**
- Modify: `src/midi.rs:117-124`, `src/lib.rs:352` (`desktop_main`), `Cargo.toml` (dependencies)

- [ ] **Step 1: Replace eprintln**

```rust
    match &event {
        MidiEvent::ControlChange { cc, value, .. } => {
            log::debug!("[midi-in] CC{} = {}", cc, value);
        }
        MidiEvent::NoteOn { channel, note, velocity } => {
            log::debug!("[midi-in] note ch{} n{} v{}", channel, note, velocity);
        }
        _ => {}
    }
```

- [ ] **Step 2: Init desktop logger**

В `Cargo.toml` `[dependencies]`:

```toml
env_logger = { version = "0.11", default-features = false, features = ["humantime"] }
```

(Попадёт и в Android-сборку без инициализации — мёртвый dep, несколько КБ; target-specific gating усложнит Cargo.toml — осознанный компромисс.) В `desktop_main` (src/lib.rs:352) первой строкой:

```rust
#[cfg(not(target_os = "android"))]
pub fn desktop_main() -> Result<()> {
    let _ = env_logger::try_init(); // RUST_LOG-gated; Android uses android_logger
    #[cfg(target_os = "linux")]
    suppress_alsa_errors();
    // ... остальное без изменений
```

`run_headless` вызывается из `desktop_main` (init уже случился); Android-путь (`android_main`, lib.rs:~895 `android_logger::init_once`) не трогать.

- [ ] **Step 3: Gates**

```bash
cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety
```
Expected: зелёные (midi.rs в RT-зоне не используется — `parse_and_push` вызывается из midir/JNI потоков; `parse_midi_message` в аудиопотоке не тронут).

- [ ] **Step 4: Commit**

```bash
git add src/midi.rs src/lib.rs Cargo.toml Cargo.lock
git commit -m "fix: replace per-message eprintln with log::debug; init desktop logger"
```

### Task 5: #4 — looper survives reconnect

**Files:**
- Modify: `src/synth/looper.rs` (LooperDisplay.loop_length_ticks, LooperEventSnapshot, restore, unit-тесты), `src/synth/mod.rs` (`ControlEvent::LooperRestore` + handle_control), `src/gui/settings.rs:186-282` (`reconnect_audio`)

- [ ] **Step 1: looper.rs — loop length in display + snapshot type + restore**

В `LooperDisplay` (looper.rs:42-45) добавить поле (инициализация `loop_length_ticks: 0` в `LooperDisplay::new`, looper.rs:47-55):

```rust
pub struct LooperDisplay {
    pub events: Vec<LooperDisplayEvent>,
    pub current_layer: u8,
    pub loop_length_ticks: u32, // source of truth for GUI snapshot; 0 = none
}
```

В `update_display` (looper.rs:490-512) перед закрывающей скобкой try_lock-блока:

```rust
            d.loop_length_ticks = self.loop_length_ticks;
```

Рядом с `LooperDisplayEvent` (looper.rs:33):

```rust
/// Serialisable event snapshot for looper restore across audio reconnects.
#[derive(Clone, Copy)]
pub struct LooperEventSnapshot {
    pub tick: u32,    // sample offset from loop start
    pub note: u8,
    pub velocity: u8, // 0 = note-off
    pub layer_id: u8,
    pub part_id: u8,
}
```

В `impl MidiLooper` (после `clear`, looper.rs:298-311 — сверить сигнатуру `clear` для списка сбрасываемых полей):

```rust
    /// Restore recorded content from a GUI-side snapshot (audio thread).
    /// Zero-alloc: copies up to MAX_LOOP_EVENTS from the borrowed slice.
    pub fn restore(&mut self, events: &[LooperEventSnapshot], loop_length_ticks: u32) {
        self.len = 0;
        self.playback_tick = 0;
        self.playback_cursor = 0;
        self.needs_note_offs = false;
        self.loop_length_ticks = loop_length_ticks;
        self.state = LooperState::Idle;
        for ev in events {
            if self.len as usize >= MAX_LOOP_EVENTS { break; }
            self.events[self.len as usize] = LoopEvent {
                tick: ev.tick,
                note: ev.note,
                velocity: ev.velocity,
                layer_id: ev.layer_id,
                part_id: ev.part_id,
            };
            self.len += 1;
        }
        // Layer bookkeeping: restored events may reference layers of the old
        // engine; undo() targets current_layer, so advance to the max seen.
        self.current_layer = events.iter().map(|e| e.layer_id).max().unwrap_or(0);
        // Fresh engine: mute/solo/active-notes are at defaults; events keep
        // their old layer_ids, which stay valid because ids are copied 1:1.
        self.atoms.state.store(LooperState::Idle.to_u8(), Ordering::Relaxed);
        self.update_atoms();
        self.update_display();
    }
```

Если `len` приватный — добавить pub-аксессор (безусловно):

```rust
    pub fn len(&self) -> u16 { self.len }
```

Unit-тесты в конец looper.rs (тут же, после реализации — типы уже существуют, компиляция не ломается):

```rust
#[cfg(test)]
mod restore_tests {
    use super::*;

    fn snap(tick: u32, note: u8, vel: u8) -> LooperEventSnapshot {
        LooperEventSnapshot { tick, note, velocity: vel, layer_id: 0, part_id: 0 }
    }

    #[test]
    fn restore_carries_content_and_length() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[snap(0, 60, 127), snap(24_000, 60, 0)], 48_000);
        assert_eq!(l.len(), 2);
        assert_eq!(l.loop_length_ticks, 48_000);
        assert_eq!(l.state, LooperState::Idle);
    }

    #[test]
    fn restore_sets_current_layer_to_max() {
        let mut l = MidiLooper::new(48_000.0);
        let events = vec![
            LooperEventSnapshot { tick: 0, note: 60, velocity: 127, layer_id: 2, part_id: 0 },
            LooperEventSnapshot { tick: 100, note: 62, velocity: 127, layer_id: 0, part_id: 0 },
        ];
        l.restore(&events, 48_000);
        assert_eq!(l.current_layer, 2);
    }

    #[test]
    fn empty_restore_is_noop() {
        let mut l = MidiLooper::new(48_000.0);
        l.restore(&[], 0);
        assert_eq!(l.len(), 0);
        assert_eq!(l.loop_length_ticks, 0);
    }

    #[test]
    fn restore_caps_at_max_events() {
        let mut l = MidiLooper::new(48_000.0);
        let events: Vec<LooperEventSnapshot> = (0..MAX_LOOP_EVENTS + 100)
            .map(|i| snap(i as u32, 60, 127))
            .collect();
        l.restore(&events, u32::MAX / 2);
        assert_eq!(l.len() as usize, MAX_LOOP_EVENTS);
    }
}
```

- [ ] **Step 2: mod.rs — ControlEvent::LooperRestore**

В enum `ControlEvent` (рядом с `LooperSetQuantize`, grep `LooperSetQuantize`):

```rust
    LooperRestore {
        events: Box<[crate::synth::looper::LooperEventSnapshot]>,
        loop_length_ticks: u32,
    },
```

В `handle_control` рядом с остальными Looper-вариантами (grep `ControlEvent::LooperSetBpm`):

```rust
            ControlEvent::LooperRestore { events, loop_length_ticks } => {
                self.looper.restore(&events, loop_length_ticks);
                // Heap-owning payload: hand it back for GUI-thread disposal.
                return Some(ControlEvent::LooperRestore { events, loop_length_ticks });
            }
```

Heap-owning вариант обязан вернуться через `Some(...)` до комментария «Every variant that owns heap memory returns early above» (mod.rs:2310-2311).

- [ ] **Step 3: settings.rs — snapshot before drop, restore after reconnect**

В `reconnect_audio` (settings.rs:186) ПЕРЕД `self._audio_handle = None;` (settings.rs:190) — движок живёт внутри бэкенда и умрёт при drop, снимаем содержимое из display (источник длины — новое поле `loop_length_ticks`, не реконструкция из bpm/bars: движок фиксирует длину на момент записи, looper.rs:225, а bpm/bars могли поменять после):

```rust
        // Snapshot looper content from the display shadow BEFORE the old
        // engine (which owns the recorder) is dropped with the dead stream.
        let looper_snap: Vec<crate::synth::looper::LooperEventSnapshot>;
        let looper_len_ticks: u32;
        {
            let mut snap = Vec::new();
            let mut len_ticks = 0u32;
            if let Ok(d) = self.looper_display.lock() {
                len_ticks = d.loop_length_ticks;
                snap = d.events.iter().map(|e| crate::synth::looper::LooperEventSnapshot {
                    tick: (e.position * d.loop_length_ticks as f32) as u32,
                    note: e.note,
                    velocity: e.velocity,
                    layer_id: e.layer_id,
                    part_id: e.part_id,
                }).collect();
            }
            looper_snap = snap;
            looper_len_ticks = len_ticks;
        }
```

(`LooperDisplay` не Clone — читаем под lock'ом, не клонируем.) После `self.send_drum_state();` (settings.rs:278):

```rust
        if !looper_snap.is_empty() && looper_len_ticks > 0 {
            let n = looper_snap.len();
            let _ = self.ctrl_tx.push(crate::synth::ControlEvent::LooperRestore {
                events: looper_snap.into_boxed_slice(),
                loop_length_ticks: looper_len_ticks,
            });
            log::info!("[reconnect] looper restored: {n} events");
        }
```

Лупер остаётся в Idle после reconnect — контент сохранён, старт воспроизведения за пользователем (существующие кнопки/пэды).

- [ ] **Step 4: Run gates + looper restore unit tests**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```
Expected: зелёные, включая `restore_tests`. RT-инвариант: `restore` — копирование в фиксированный массив + pre-sized display-Vec, аллокаций нет; Box-payload уходит в trash (`Trash::Control`, audio.rs:178-200).

- [ ] **Step 5: Commit**

```bash
git add src/synth/looper.rs src/synth/mod.rs src/gui/settings.rs
git commit -m "feat: looper content survives audio reconnect (display snapshot + restore)"
```
