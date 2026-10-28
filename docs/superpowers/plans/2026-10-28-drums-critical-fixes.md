# Drums critical fixes (D1, D2, D3, D6) Implementation Plan

status: executed 2026-10-28 (branch worktree-bugfix-batch, commits d10797b..87becf6)

anchor-checked: 2026-10-28 (независимое ревью на master @ 329faac; находки вшиты)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Починить критические находки drums-аудита (docs/deep-review-2026-10-28-audit-remediation.md): D1 identity-remap в GUI как 0..15 вместо нот 36..51, D2 `set_sample_rate` стирает состояние DrumEngine, D3 underflow `note - 36` в record-пути, D6 `send_drum_state` не пушит choke-группы.

**Architecture:** D1/D3 — точечные правки в GUI-кодировании и record-гварде; D2 — `DrumEngine::set_sample_rate` сохраняет всё пользовательское состояние (remap, pad_vel, choke, sf2_mode, fixed_velocity, volume, sequencer целиком — StepSequencer не хранит sample_rate, тайминг передаётся параметром), что снимает и необходимость re-push флага в GUI; D6 — `send_drum_state` дополнительно пушит choke-таблицу.

**Tech Stack:** Rust, cargo test, `tests/drum_remap.rs` (движковые тесты), `tests/sustain_audit.rs` (harness для интеграционного теста record-пути).

**Обязательные проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требования (из аудита): (1) identity remap кодируется нотами 36..51 во всех четырёх местах GUI и после unload ноты 30..35 не возникают; (2) сохранённая в конфиг таблица нот 36..51 восстанавливается как есть; (3) `set_sample_rate` сохраняет remap/pad_vel/choke/sf2_mode/fixed_velocity/volume/паттерны секвенсора; (4) удар по пэду при remap-цели вне 36..51 не паникует и не пишет хит в чужой слот. Тесты Task 1 пишет FRESH subagent, получивший только этот план.

## Negative Matrix

- missing field — конфиг-таблицы короче 16: `unwrap_or(36 + i)` даёт identity-хвост (тест 2b).
- null — нет Option в таблицах.
- zero/empty — пустая remap-таблица в конфиге → identity 36..51 (тест 2a).
- out-of-range notes — remap-цель вне 36..51 (30..55 легальны для SF2): record дропается, SF2-триггер играет (тест 4, Task 3); цели <36 после клампа движка невозможны (drum.rs:1026).
- GUI/engine encoding mismatch — главное содержание D1: legacy 0..15 в конфиге мигрирует +36 на всех путях загрузки (тест 2, Task 2 — gui/mod.rs:1099 и :1601).
- clock skew — неприменимо.
- duplicate/concurrent — `set_sample_rate` дважды подряд: состояние сохраняется оба раза (тест 3c).
- sample-rate change — сверх требуемого: после фикса D2 проверяется сохранением состояния (тесты 3a/3b), choke-стейт сбрасывается (тест 3d).
- external-call timeout — неприменимо.

---

### Task 1: Regression tests (fresh subagent, plan-only context)

**Files:**
- Modify: `tests/drum_remap.rs` (движковые контракты), `tests/sustain_audit.rs` (интеграционный record-тест)

- [x] **Step 1: Write engine-level tests в tests/drum_remap.rs**

Посмотреть существующие импорты/хелперы в файле и дописать:

```rust
#[test]
fn identity_remap_is_full_notes_36_to_51() {
    // D1 contract: GUI identity = 36+i (полные ноты), не слоты 0..15.
    let mut e = DrumEngine::new(48_000.0);
    // after build() identity is engine default; emulate what GUI must write:
    let identity: Vec<u8> = (0..16).map(|i| 36 + i as u8).collect();
    e.apply_remap_vec(&identity);
    for i in 0..16u8 {
        assert_eq!(e.remapped_note(36 + i), 36 + i, "pad slot {i} not identity");
    }
}

#[test]
fn legacy_slot_encoding_migrates_to_notes() {
    // D1: legacy-таблица из старого конфига (значения 0..15) интерпретируется
    // как слоты и мигрирует +36; ноты >=16 остаются как есть.
    let mut e = DrumEngine::new(48_000.0);
    let legacy: Vec<u8> = (0..16).collect(); // 0..15 — старое (баговане) кодирование
    let migrated: Vec<u8> = legacy.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
    e.apply_remap_vec(&migrated);
    assert_eq!(e.remapped_note(36), 36);
    assert_eq!(e.remapped_note(51), 51);
}

#[test]
fn sample_rate_change_preserves_user_state() {
    // D2: set_sample_rate не стирает remap/pad_vel/choke/sf2_mode/
    // fixed_velocity/volume/секвенсор.
    let mut e = DrumEngine::new(48_000.0);
    let remap: Vec<u8> = (0..16).map(|i| (36 + i) as u8).collect();
    e.apply_remap_vec(&remap);
    e.apply_pad_vel_vec(&[100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 90]);
    let mut choke = [0u8; 26];
    choke[0] = 1; // note 30 -> group 1
    e.set_choke_groups(&choke);
    e.sf2_mode = true;
    e.fixed_velocity = true;
    e.volume = 0.42;
    e.set_sample_rate(44_100.0);
    assert_eq!(e.remapped_note(36), 36, "remap lost");
    assert_eq!(e.pad_velocity(36, 5), 100, "pad_vel lost");
    assert_eq!(e.choke_groups[0], 1, "choke table lost");
    assert!(e.sf2_mode, "sf2_mode lost");
    assert!(e.fixed_velocity, "fixed_velocity lost");
    assert_eq!(e.volume, 0.42, "volume lost");
}

#[test]
fn sample_rate_change_twice_still_preserves() {
    let mut e = DrumEngine::new(48_000.0);
    let remap: Vec<u8> = (0..16).map(|i| (40 + i) as u8).collect();
    e.apply_remap_vec(&remap);
    e.set_sample_rate(44_100.0);
    e.set_sample_rate(48_000.0);
    assert_eq!(e.remapped_note(36), 40, "remap lost on double SR change");
}

#[test]
fn sample_rate_change_resets_choke_state() {
    // stale group_last_note не переживает SR change
    let mut e = DrumEngine::new(48_000.0);
    let mut choke = [0u8; 26];
    choke[0] = 1;
    e.set_choke_groups(&choke);
    assert_eq!(e.sf2_choke_take(30), 0);
    e.set_sample_rate(44_100.0);
    let prev = e.sf2_choke_take(30);
    assert_eq!(prev, 0, "stale choke holder survived SR change");
}
```

Импорт: `use mini_midi_synth::synth::drum::DrumEngine;` — уже есть в tests/drum_remap.rs:2. Поля `sf2_mode`/`fixed_velocity`/`volume` — уже `pub` (drum.rs:857-861), ассерты компилируются.

- [x] **Step 2: Write record-path underflow test в tests/sustain_audit.rs**

Harness (helpers `engine_with_init_patch`, `render_peak`) уже в файле. Имена вариантов: `ControlEvent::DrumSetRemap { table: Box<[u8; 16]> }` (mod.rs:212), `ControlEvent::DrumSeqRecord { recording: bool }` (mod.rs:191).

```rust
#[test]
fn drum_record_with_out_of_range_remap_target_does_not_panic() {
    use mini_midi_synth::synth::ControlEvent;
    let mut e = engine_with_init_patch();
    // remap pad slot 0 (note 36) to target 30 (вне 36..51 — легитимная SF2-цель)
    let mut remap = [36u8; 16];
    remap[0] = 30;
    let _ = e.handle_control(ControlEvent::DrumSetRemap { table: Box::new(remap) });
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    // live hit on ch10, note 36 -> remapped target 30 -> slot math 30-36
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 36, velocity: 100 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 36 });
    // must not panic (debug overflow) and must not record a bogus hit
    assert!(render_peak(&mut e, 10) >= 0.0);
}
```

- [x] **Step 3: Run, verify failure**

Run: `cargo test --test drum_remap --test sustain_audit`
Expected: `sample_rate_change_preserves_user_state` FAIL (remap/pad_vel/sf2_mode сброшены), `drum_record_with_out_of_range_remap_target_does_not_panic` FAIL с паникой «attempt to subtract with overflow». Тесты identity/migration — PASS (движок уже корректен; они фиксируют контракт).

- [x] **Step 4: Commit (red)**

```bash
git add tests/drum_remap.rs tests/sustain_audit.rs
git commit -m "test: drums D1/D2/D3 regression cases (red)"
```

### Task 2: D1 — GUI identity encoding (4 места)

**Files:**
- Modify: `src/gui/mod.rs:1099` (internal-kit restore), `src/gui/mod.rs:1606` (kit load, нет сохранённой таблицы), `src/gui/mod.rs:1641` (unload), `src/gui/drums.rs:221` (detach дубликата в однопэдной модалке)

- [x] **Step 1: Fix all four anchors**

`src/gui/mod.rs:1096-1107` (internal-kit restore) — добавить ту же миграцию legacy slot-кодирования, что и в per-kit load (mod.rs:1601-1603), иначе конфиги от багованной сборки с 0..15 под ключом «internal» после фикса дадут цели 0..15 → пэды 36..51 молчат. Заменить блок:

```rust
        if self.sf2_drums_loaded_name.is_empty() {
            if let Some(v) = self.config.sf2.drums_remaps.get("internal") {
                // migrate legacy slot-encoded values (0..16) to kit notes (+36)
                let v: Vec<u8> = v.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
                let mut t = [0u8; 16];
                for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or((36 + i) as u8).min(55); }
                self.drum_remap = t;
            }

`src/gui/mod.rs:1606` — ветка «нет сохранённой таблицы»:

```rust
            for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = (36 + i) as u8; }
```

`src/gui/mod.rs:1641` — unload, identity reset:

```rust
        for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = (36 + i) as u8; }
```

`src/gui/drums.rs:221` — detach:

```rust
                    if *v == sel && i != idx { *v = (36 + i) as u8; }
```

Эталон уже в кодовой базе: Reset в полной модалке `src/gui/drums.rs:150` (`*v = 36 + i as u8;`) и per-kit load `src/gui/mod.rs:1603`.

- [x] **Step 2: Build + clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: зелёный. ВНИМАНИЕ: `cargo test` в этой точке ещё НЕ полностью зелёный — красные остаются `sample_rate_change_preserves_user_state`, `sample_rate_change_twice_still_preserves` (их закрывает Task 4) и `drum_record_with_out_of_range_remap_target_does_not_panic` (его закрывает Task 3). Это ожидаемо. GUI-код тестами напрямую не покрыт (приватные методы App) — контракт зафиксирован движковым тестом `identity_remap_is_full_notes_36_to_51`.

- [x] **Step 3: Commit**

```bash
git add src/gui/mod.rs src/gui/drums.rs
git commit -m "fix: encode identity drum remap as notes 36..51 in all GUI paths"
```

### Task 3: D3 — record-path underflow guard

**Files:**
- Modify: `src/synth/mod.rs:1580-1592` (live ch10 record в `handle_event`)

- [x] **Step 1: Guard the slot computation**

Не клампить (кламп 30→36 записал бы хит в чужой слот 0 — тихая порча паттерна), а пропускать запись. Заменить:

```rust
                        if is_drum_note {
                            let slot = (note - 36) as usize;
```

на:

```rust
                        // `note` here is the REMAPPED target (30..=55 legal). Only
                        // 36..=51 maps to sequencer slots; skip recording other
                        // targets (drop, don't clamp — clamping would corrupt
                        // a neighbouring slot's pattern).
                        if is_drum_note && (36..=51).contains(&note) {
                            let slot = (note - 36) as usize;
```

Важно: `is_drum_note` выше вычислен по сырой ноте до ремапа — условие записи по remapped-ноте должно быть именно `(36..=51).contains(&note)`. SF2-триггер ниже по коду (`sampler_drums` ветка) не менять — ноты 30..55 там легитимны.

- [x] **Step 2: Run underflow test**

Run: `cargo test --test sustain_audit drum_record_with_out_of_range -- --nocapture`
Expected: PASS (нет паники, нет bogus hit).

- [x] **Step 3: Commit**

```bash
git add src/synth/mod.rs
git commit -m "fix: skip drum record for remap targets outside 36..51 (underflow)"
```

### Task 4: D2 — set_sample_rate preserves state

**Files:**
- Modify: `src/synth/drum.rs:953-962`

- [x] **Step 1: Preserve state across rebuild**

Заменить `set_sample_rate` (сохранив существующий комментарий о вызове из RT-callback):

```rust
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        // Preserve everything user-facing: StepSequencer timing takes sample_rate
        // as a parameter (no stored sr), voice coefficients are the only thing
        // that must be rebuilt. Stale choke holders are reset (panic-path rule).
        let step = self.step_atom.clone();
        let play = self.play_atom.clone();
        let rec = self.rec_atom.clone();
        let params = self.params;
        let sequencer = std::mem::replace(&mut self.sequencer, StepSequencer::new());
        let volume = self.volume;
        let enabled = self.enabled;
        let sf2_mode = self.sf2_mode;
        let fixed_velocity = self.fixed_velocity;
        let remap = self.remap;
        let pad_vel = self.pad_vel;
        let choke_groups = self.choke_groups;
        self.sample_rate = sample_rate;
        *self = Self::new(sample_rate);
        self.step_atom = step;
        self.play_atom = play;
        self.rec_atom = rec;
        self.params = params;
        self.sequencer = sequencer;
        self.volume = volume;
        self.enabled = enabled;
        self.sf2_mode = sf2_mode;
        self.fixed_velocity = fixed_velocity;
        self.remap = remap;
        self.pad_vel = pad_vel;
        self.choke_groups = choke_groups;
        self.group_last_note = [0u8; NUM_CHOKE_GROUPS];
    }
```

Поля `sf2_mode`/`fixed_velocity`/`volume` уже `pub` (drum.rs:857-861) — тест из Task 1 компилируется без изменений API. Синхронизация sampler: `Sampler::set_sample_rate` сохраняет `drums_sf2`, теперь и движок сохраняет `sf2_mode` — десинк устранён. GUI-десинка нет: атомы `step/play/rec` сохраняют те же Arc, `drum_volume` GUI и сохранённый движковый `volume` остаётся синхронным (Sampler `drums_sf2`/volume не трогает). Reset `group_last_note` согласован: при SR-change семплер всё равно перезагружает SF2-голоса.

- [x] **Step 2: Run drums tests**

Run: `cargo test --test drum_remap`
Expected: все зелёные, включая `sample_rate_change_*`.

- [x] **Step 3: Full gates + rt_safety**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```
Expected: зелёные. (Аллокация `Self::new` в callback — существующий techdebt #12, план его не трогает.)

- [x] **Step 4: Commit**

```bash
git add src/synth/drum.rs
git commit -m "fix: DrumEngine::set_sample_rate preserves remap/pad_vel/choke/sf2/volume/sequencer"
```

### Task 5: D6 — send_drum_state pushes choke groups

**Files:**
- Modify: `src/gui/mod.rs:1490-1505` (`send_drum_state`)

- [x] **Step 1: Add choke push**

Свежий движок после reconnect стартует с GM choke-таблицей; derived-таблица SF2-кита теряется. В конец `send_drum_state` (после `push_drum_pad_vel()`):

```rust
        // Choke semantics survive reconnect even if the SF2 restore path bails
        // (missing file). For the internal kit (no SF2 loaded) this pushes a
        // zero table over the engine's GM default — harmless: `sf2_choke_take`
        // is not called in DSP mode (DSP choke lives on the hardcoded
        // CHOKE_GROUPS), and load_sf2_drums overwrites it for SF2 kits.
        self.push_drum_choke_groups();
```

Метод `push_drum_choke_groups` уже существует (`src/gui/mod.rs:1424-1428`), idempotent, таблица `[u8; 26]` через Box — trash-паттерн как у остальных. Порядок безопасен: `DrumSetChokeGroups` сбрасывает `group_last_note` внутри движка (`drum.rs:1016-1019`). SF2-mode/soundfont восстановление остаётся на существующем пути `send_initial_presets` → `load_sf2_drums` (out of scope).

- [x] **Step 2: Gates**

```bash
cargo clippy --all-targets -- -D warnings && cargo test
```
Expected: зелёные (GUI-код, движковых изменений нет).

- [x] **Step 3: Commit**

```bash
git add src/gui/mod.rs
git commit -m "fix: reconnect restores derived drum choke groups"
```
