# Refactor Quick Wins Batch (QW1-QW11) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

status: executed 2026-11-07 (PR: QW-батч, субагентно; 3 багфикса + 8 рефакторов)

anchor-checked: 2026-11-07 (база = master ed992a0; анкоры из refactor-audit отчёта этого же дня, все Critical/High перепроверены вручную; канон-исследование — Tavily 2026-11-07: SSOT-параметры JUCE APVTS, dead-code sweep, characterization)

**Goal:** Закрыть 11 Quick wins из refactor-audit: 3 реальных бага (CC scope flip, GM-pick затирание layer_b, LFO1 в mod-matrix), мёртвое состояние (cached_*), cfg-дрейф, 109 дублированных дефолтов, 13 мёртвых pub API, пины и дубликаты.

**Architecture:** Каждый QW — отдельный атомарный коммит с гейтом (clippy + tests + goldens). Behavior-changing фиксы (QW1, QW11) помечены: если goldens красные — сознательная регенерация с записью в коммит-сообщении. Рефактор-механика (QW3/QW5/QW10) обязана оставить все golden-хэши НЕИЗМЕННЫМИ — это проверка корректности.

**Tech Stack:** Rust; TDD где есть чистая поверхность (QW1/QW6/QW9/QW11); для GUI-мест (QW2) — код-ревью + гейт. Гейты: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`.

**Проверенные факты (вычитано из кода 2026-11-07; не пересматривать):**
- PARAM_REGISTRY (cc_map.rs:24-61): `delay_mix` и `reverb_mix` (:50,:55) — `scope: ParamScope::Patch`; GLOBAL_PARAM_KEYS (:64-66) содержит оба; default_map биндит CC66/CC54 как Global (:145-156). `CcMap::from_config` (:160-185) берёт scope из `find_param_meta` (реестр), игнорируя сохранённый `raw_binding.scope` — отсюда flip Global→Patch.
- CC-диспетчер (synth/mod.rs:2488-2492) маршрутизирует по `binding.scope`: Patch → `part.set_param`, Global → `global_params.set`.
- Баг браузера (gui/mod.rs:1995-1997): `if part == 0 { layer_a } else { layer_b }` — part ≥2 пишет layer_b. Параллельный блок режима (:1679-1681) корректно гвардит part==0/part==1.
- `cached_mod_sources/cached_lfo_pitch_offset/cached_filter_offset/cached_lfo_amp_base` (synth/mod.rs:613,620-622 decl; 657-675 init; 3592-3595 writes) — НОЛЬ чтений (проверено grep по src/, tests/, gui/).
- lib.rs:365 `drum_undo_atom,` — единственный не-gui-gated литерал среди соседей (:360-368), поле и локал gui-gated.
- `CachedFxParams::from_preset` (synth/mod.rs:1482-1598): 109 `p.map(|p| p.X).unwrap_or(LIT)`; дрифт литералов от `PatchParams::default()` = 0 (проверено тестом 2026-11-07).
- `tc_linear_interp_bit_identical_to_canon` (synth/tests.rs:1475-1499) — только write_pos=0; combulator/floaty используют always-add+fmod wrap, канон — conditional-subtract; раз ловится только на положительных read_pos.
- 13 мёртвых pub API (audit F5): audio.rs:377, graphic_eq.rs:126, compressor.rs:52+54, filter.rs:120, sampler.rs:327+329, drum.rs:1393, lfo.rs:64+259+266, theme.rs:121+125.
- Panic-сообщение (synth/mod.rs:1284) «GlobalParams::set: unknown key» — на самом деле fallback **Part::set_param**; `GlobalParams::set` (:509) молчит во всех сборках. Все продовые отправители шлют только ключи из 7 arm'ов (gui/mod.rs:1211-1251: master_tone/reverb_mix/delay_mix; lib.rs:471-480: config.ui globals).
- Педальный pedal-up блок verbatim ×2: set_sustain synth/mod.rs:1740-1780 и SetPartSustain :2670-2708. Latch-mismatch блок verbatim ×2: ProgramChange :2394-2401 и LoadPatch :2536-2543.
- update_routing_cache (synth/mod.rs:681-694) кэширует lfo2/3/4_routed + scene, НО НЕ lfo1; гейт lfo1 (:3455-3459) не проверяет routed, гейт lfo2 (:3466) проверяет `part.lfo2_routed`.
- Панельный пин (tests/ui_layout_contract.rs:93-127): include_str только mod.rs, magic offsets 21/24, нет assert CentralPanel-последним.

---

## Test Cases (from the requirement)

Требование: устранить баги QW1/QW2/QW11, удалить мёртвый код без изменения поведения, укрепить пины.

- TC1 (QW1, реестр-пин): для каждого `PARAM_META` с `is_global_param(key)` — `scope == ParamScope::Global`.
- TC2 (QW1, round-trip): `CcMap::default_map()` → сериализовать в config → `from_config` → binding CC66 (reverb_mix) и CC54 (delay_mix) имеют `scope == Global`.
- TC3 (QW6): linear-interp пин ловит positive-read ветку канона: канон vs inline-зеркало bit-identical при write_pos ∈ {100,300,511} (сейчас пин только write_pos=0 — всегда wrap-ветка).
- TC4 (QW9): панельный пин читает все src/gui/*.rs, деривит длину иглы, падает если `CentralPanel` НЕ после последней bottom-панели.
- TC5 (QW11): Part с mod-matrix слотом Lfo1 (depth≠0) после `update_routing_cache()` имеет `lfo1_routed == true`; гейт lfo1 тикает LFO (следствие — поведение, экран).
- TC6 (регрессия, все задачи): 884 preset golden + 1642 sweep golden + fx goldens НЕ изменяются задачами QW3/QW5/QW7/QW8/QW10 (behavior-preserving).

## Negative Matrix

- ~~missing field~~ — нет serde-правок.
- CC66 при ОТСУТСТВУЮЩЕЙ записи в конфиге → `from_config` возвращает default_map (ветка else :162) — scope Global из default_map, регрессии нет.
- Конфиг со СТАРОЙ записью scope=Patch для reverb_mix → after QW1 `from_config` форсит Global из реестра (осознанно: реестр — SSOT, сохранённый scope больше не источник правды).
- part ≥2 в браузере GM → после QW2 персист просто НЕ происходит (без затирания layer_b) — осознанное поведение.
- LFO1 слот с depth=0 → `lfo1_routed` = false (условие `depth.abs() > 0.001`), гейт не меняется.
- Дублирующий вызов update_routing_cache — idempotent (pure scan).
- GlobalParams::set с неизвестным ключом → debug_assert (паника в debug) — все продовые отправители проверены (7 arm'ов покрывают gui/lib-сендеры), тесты шлют только arm-ключи.
- Удаление dead API, на которое ссылался бы тест → гейт `cargo test` поймает E0425 на месте.
- Goldens красные после behavior-preserving задачи → НЕ регенерировать: найти причину (задача сломала семантику).
- Параллельный запуск двух QW на одном файле synth/mod.rs — запрещён: задачи исполняются строго последовательно (анКОРЫ строк плывут).

---

### Task 1: QW1 — CC scope flip (TDD)

**Files:**
- Modify: `src/cc_map.rs:50,55` — scope Patch→Global для delay_mix/reverb_mix
- Test: `src/synth/tests.rs` (новые пины в конец)

- [x] **Step 1: падающие тесты** — в конец src/synth/tests.rs:

```rust

// ── QW1: CC scope SSOT (2026-11-07 refactor-audit) ──────────────────

#[test]
fn tc_registry_global_keys_have_global_scope() {
    for m in crate::cc_map::PARAM_REGISTRY {
        if crate::cc_map::is_global_param(m.key) {
            assert_eq!(m.scope, crate::cc_map::ParamScope::Global,
                "registry scope contradicts GLOBAL_PARAM_KEYS for '{}'", m.key);
        }
    }
}

#[test]
fn tc_cc_scope_survives_config_roundtrip() {
    let mut config = crate::config::Config::default();
    // default_map (CC66=reverb_mix, CC54=delay_mix) → raw-формат конфига
    let bindings: std::collections::BTreeMap<u8, crate::cc_map::CcBindingRaw> =
        crate::cc_map::CcMap::default_map().bindings.iter().enumerate()
            .filter_map(|(cc, b)| {
                let b = (*b)?;
                Some((cc as u8, crate::cc_map::CcBindingRaw {
                    param_key: b.param_key.to_string(),
                    min_val: b.min_val, max_val: b.max_val,
                    logarithmic: b.logarithmic, scope: b.scope,
                }))
            }).collect();
    config.cc_map = Some(crate::cc_map::CcMapRaw { bindings });
    let map = crate::cc_map::CcMap::from_config(&config);
    for cc in [54u8, 66] {
        let b = map.bindings[cc as usize].expect("binding present");
        assert_eq!(b.scope, crate::cc_map::ParamScope::Global,
            "CC{cc} {} must stay Global after roundtrip", b.param_key);
    }
    // SSOT-forcing: конфиг со СТАРОЙ записью scope=Patch — реестр правит
    let mut stale = bindings.clone();
    stale.get_mut(&66).unwrap().scope = crate::cc_map::ParamScope::Patch;
    let mut cfg2 = crate::config::Config::default();
    cfg2.cc_map = Some(crate::cc_map::CcMapRaw { bindings: stale });
    let map2 = crate::cc_map::CcMap::from_config(&cfg2);
    assert_eq!(map2.bindings[66].as_ref().unwrap().scope,
        crate::cc_map::ParamScope::Global,
        "registry is SSOT: stale saved scope must be overridden");
}
```

- [x] **Step 2: красный**

Run: `cargo test --lib tc_registry_global tc_cc_scope 2>&1 | tail -6`
Expected: tc_registry_global_keys_have_global_scope FAIL (delay_mix/reverb_mix Patch); tc_cc_scope_survives_config_roundtrip FAIL.

- [x] **Step 3: фикс** — cc_map.rs: в PARAM_REGISTRY заменить scope для двух записей:

```rust
    ParamMeta { key: "delay_mix", label: "Delay Mix", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Global },
```
```rust
    ParamMeta { key: "reverb_mix", label: "Reverb Mix", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Global },
```

Комментарий над ними обновить: `// Global (persist across patch changes)` — оба уже в GLOBAL_PARAM_KEYS.

- [x] **Step 4: зелёный + полный гейт**

Run: `cargo test --lib tc_registry_global tc_cc_scope 2>&1 | tail -3 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: оба теста PASS, clippy чист, все suite'ы зелёные (существующий пин tc_cc_bindings_target_set_param_arms остаётся зелёным).

- [x] **Step 5: коммит**

```bash
git add src/cc_map.rs src/synth/tests.rs && git commit -m "fix: CC scope SSOT — reverb_mix/delay_mix Global in PARAM_REGISTRY, faders survive cc_map roundtrip (QW1)"
```

### Task 2: QW2 — GM-pick затирание layer_b (bugfix)

**Files:**
- Modify: `src/gui/mod.rs:1995-1997`

- [x] **Step 1: фикс** — заменить:

```rust
            // Persist for parts 0/1 (seeds program on startup) — перенос mod.rs:1198-1200
            if part == 0 { self.config.sf2.layer_a_program = prog; }
            else { self.config.sf2.layer_b_program = prog; }
```

на

```rust
            // Persist for parts 0/1 only (startup seeds). part >= 2: no persist —
            // прежний else затирал layer_b_program с любого парта (QW2).
            if part == 0 { self.config.sf2.layer_a_program = prog; }
            else if part == 1 { self.config.sf2.layer_b_program = prog; }
```

- [x] **Step 2: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: чисто, зелёные. (Экранная проверка юзером отдельно: GM-pick на части ≥2 не меняет seed части 1.)

- [x] **Step 3: коммит**

```bash
git add src/gui/mod.rs && git commit -m "fix: GM browser pick on parts >=2 no longer clobbers layer_b_program seed (QW2)"
```

### Task 3: QW3 — удалить write-only cached_* поля

**Files:**
- Modify: `src/synth/mod.rs:613,620-622` (decl), `:657-675` (init), `:3592-3595` (writes), комментарии вокруг

- [x] **Step 1: verify zero reads** (обязательный повтор перед удалением):

Run: `grep -rn "cached_mod_sources\|cached_lfo_pitch_offset\|cached_filter_offset\|cached_lfo_amp_base" src/ tests/ | grep -v "mod.rs:613\|mod.rs:62\|mod.rs:359\|mod.rs:65[0-9]\|mod.rs:67[0-9]"`
Expected: ПУСТО (только decl/init/writes). Если есть чтения — STOP, задача отменяется.

- [x] **Step 2: удалить** — 4 decl-строки (:620-622 + коммент «Cached control-rate base values...»); init: `cached_mod_sources` — 11-строчный struct literal (:657-667) + 3 строки (:673-675), НЕ трогая перемешанные с ними `poly_at_in_matrix`/`lfo2/3/4_routed` init (:669-672); 4 write-строки (:3592-3595 `part.cached_mod_sources = mod_sources;` и соседние).

- [x] **Step 3: гейт — goldens ОБЯЗАНЫ остаться идентичными**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test --test golden_fx 2>&1 | tail -2 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: всё зелёное, goldens не тронуты (поля write-only — рендер байт-в-байт тот же). Красные goldens = найдено скрытое чтение → откат и разбор.

- [x] **Step 4: коммит**

```bash
git add src/synth/mod.rs && git commit -m "refactor: delete write-only Part::cached_* fields (QW3, zero reads, goldens unchanged)"
```

### Task 4: QW4 — cfg-дрейф, gui-less сборка

**Files:**
- Modify: `src/lib.rs:365` (+ все места, которые вывалит check)

- [x] **Step 1: воспроизвести красный**

Run: `cargo check --lib --no-default-features 2>&1 | grep -E "^error" | head -8`
Expected: E0425/E0560 вокруг drum_undo_atom (lib.rs:365) — возможно ещё места того же класса.

- [x] **Step 2: фикс** — lib.rs:365:

```rust
        #[cfg(feature = "gui")]
        drum_undo_atom,
```

и все остальные ошибки того же класса из Step 1 (unguarded литералы gui-полей) — тем же паттерном.

- [x] **Step 3: зелёный**

Run: `cargo check --lib --no-default-features 2>&1 | tail -1 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: check ok, всё зелёное.

- [x] **Step 4: коммит**

```bash
git add src/lib.rs && git commit -m "fix: gui-less build — missing cfg on CommonInit literal fields (QW4)"
```

### Task 5: QW5 — CachedFxParams: убрать 109 дублированных дефолтов

**Files:**
- Modify: `src/synth/mod.rs:1482-1598` (from_preset)

- [x] **Step 1: механический рефактор** — шапку from_preset заменить на:

```rust
    fn from_preset(p: Option<&PatchParams>, global: &GlobalParams) -> Self {
        // Нет включённой части → PatchParams::default() (один источник
        // дефолтов; прежние 109 unwrap_or-литералов дублировали его).
        let default_params = PatchParams::default();
        let p = p.unwrap_or(&default_params);
        Self {
            tape_drive: p.tape_drive,
            tape_saturation: p.tape_saturation,
```
далее все 109 полей — прямой reading `p.X` вместо `p.map(|p| p.X).unwrap_or(LIT)`. ДВА глобальных override в конце (delay_mix/reverb_mix из global_params, :1513-1517) сохранить как есть.

Реализация механикой: python-скрипт по файлу, паттерн `p\.map\(\|p\| p\.([a-z0-9_]+)\)\.unwrap_or\([^)]*\)` → `p.\1`. ОСОБЫЕ СЛУЧАИ (скрипт их не покрывает/меняет — руками):
- `airwindows_mode: p.map(|p| p.airwindows_mode as u32).unwrap_or(0)` — cast ломает паттерн, руками → `p.airwindows_mode as u32`.
- Два global-override (:1513-1517): обёртку `global.X.unwrap_or_else(...)` сохранить; внутренний `p.map(|p| p.X).unwrap_or(LIT)` схлопнется скриптом в `p.X` — это ок (семантика идентична, «как есть» относится только к самой глобальной обёртке).
Итого 111 `p.map` сайтов = 108 plain + 2 override-inner + 1 cast (скрипт покрывает 110, cast — руками).

- [x] **Step 2: гейт — goldens ОБЯЗАНЫ остаться идентичными**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test --test golden_fx 2>&1 | tail -2 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: всё зелёное без регенерации (дрифт литералов от default() = 0, проверено). Красные goldens → откат: где-то литерал ≠ default (найди и разбери).

- [x] **Step 3: коммит**

```bash
git add src/synth/mod.rs && git commit -m "refactor: CachedFxParams::from_preset reads PatchParams defaults directly — 109 duplicated literals removed (QW5, goldens unchanged)"
```

### Task 6: QW6 — пин linear-interp: mid-buffer ветка

**Files:**
- Modify: `src/synth/tests.rs` (tc_linear_interp_bit_identical_to_canon, :1475-1499)

- [x] **Step 1: расширить тест** — после существующего цикла добавить:

```rust
    // Wrap-ветка при положительном read_f (mid-buffer write_pos): ловит
    // расхождение always-add+fmod (combulator/floaty) с канон-conditional
    for wp in [100usize, 300, 511] {
        for delay_raw in [0.0f32, 0.5, 1.0, 7.25, 50.0] {
            let d = delay_raw.clamp(1.0, (n - 2) as f32);
            let read_f = wp as f32 - d;
            let read_f = if read_f < 0.0 { read_f + n as f32 } else { read_f };
            let i0 = read_f as usize % n;
            let i1 = (i0 + 1) % n;
            let frac = read_f.fract();
            let local = buf[i0] * (1.0 - frac) + buf[i1] * frac;
            let canon = dsp_utils::buf_read_linear(&buf, wp, delay_raw);
            assert_eq!(canon.to_bits(), local.to_bits(),
                "linear interp diverges at write_pos={wp} delay={delay_raw}");
        }
    }
```

- [x] **Step 2: зелёный** (тест пинит канон; расхождение fmod-вариантов он ЗАФИКСИРУЕТ когда дойдёт до их слияния в S6)

Run: `cargo test --lib tc_linear_interp 2>&1 | tail -2`
Expected: PASS.

- [x] **Step 3: коммит**

```bash
git add src/synth/tests.rs && git commit -m "test: linear-interp pin covers positive-read wrap branch (QW6, S6 guard)"
```

### Task 7: QW7 — удалить 13 мёртвых pub API

**Files:**
- Modify: `src/audio.rs:377`, `src/synth/graphic_eq.rs:126`, `src/synth/compressor.rs:52,54`, `src/synth/filter.rs:120`, `src/synth/sampler.rs:327,329`, `src/synth/drum.rs:1393`, `src/synth/lfo.rs:64,259,266`, `src/gui/theme.rs:121,125` (+ их `#[allow(dead_code)]`)

- [x] **Step 1: verify zero refs** (для КАЖДОГО имени перед удалением):

Run: `for f in default_output_device_name update_gains set_attack set_release is_k35 keys_loaded drums_loaded slot_name TEMPO_SYNC_NAMES trigger_envelope release_envelope PAD_CYAN PAD_PINK; do echo "== $f"; grep -rn "$f" src/ tests/ --include="*.rs" | grep -v "pub fn $f\|pub const $f" | head -3; done`
Expected: для каждого из 13 — пусто или только комменты/allow-строки. ЛОЖНЫЕ СРАБАТЫВАНИЯ-декой (одноимённые префиксы, НЕ эти функции): `set_attack_shape`/`set_release_time_scale` (voice.rs:496,506; mod.rs:3065; sampler.rs:302,350), `retrigger_envelope` (lfo.rs), `sf2_keys_loaded_name`/`sf2_drums_loaded_name` (sampler.rs). Они остаются. Если есть реальный вызов именно цели — пункт НЕ удалять.

- [x] **Step 2: удалить** — тела функций/констант + их `#[allow(dead_code)]` строки. Внимание: compressor set_attack/set_release — пара; theme PAD_CYAN/PAD_PINK — константы палитры (проверить, что не в макросах).

- [x] **Step 3: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: чисто, зелёные.

- [x] **Step 4: коммит**

```bash
git add -u && git commit -m "refactor: delete 13 dead pub API items (QW7, zero refs verified per item)"
```

### Task 8: QW8 — panic-сообщение + GlobalParams assert

**Files:**
- Modify: `src/synth/mod.rs:1283-1285`, `:509-510`

- [x] **Step 1: правки**

mod.rs:1284 — сообщение на честное:
```rust
                debug_assert!(false, "Part::set_param: unknown key '{key}'");
```

mod.rs:507-510 — fallback GlobalParams::set:
```rust
            _ => {
                debug_assert!(false, "GlobalParams::set: unknown key '{key}'");
            }
```

- [x] **Step 2: гейт** (тесты шлют только arm-ключи; продовые сендеры — 5-7 ключей из arm'ов; если debug_assert сработает в тестах — найден настоящий рассинхрон сендера, разбор а не откат)

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: зелёные.

- [x] **Step 3: коммит**

```bash
git add src/synth/mod.rs && git commit -m "fix: honest debug_assert messages — Part::set_param fallback named correctly, GlobalParams::set fail-loud in debug (QW8)"
```

### Task 9: QW9 — хардening панельного пина

**Files:**
- Modify: `tests/ui_layout_contract.rs:93-127` (panel_registration_order_matches_source_contract)

- [x] **Step 1: переписать скан** — ЗАМЕНИТЬ целиком старый тест `panel_registration_order_matches_source_contract`, старую константу `EXPECTED` и старый `extract_id` на:

```rust
const EXPECTED_PANELS: &[(&str, &str)] = &[
    ("midi_learn_bar", "top"),
    ("header", "top"),
    ("global_controls", "top"),
    ("keyboard_panel", "bottom"),
    ("pad_perf_bar", "bottom"),
    ("drum_seq_panel", "bottom"),
];

/// Все gui-исходники одним буфером: при S4-разрезе регистрация панелей
/// может переехать в дочерний модуль — пин обязан её найти (QW9).
/// include_str! принимает только литералы — потому явный format!, не цикл.
fn panel_sources() -> String {
    // looper.rs НЕ существует в src/gui/ — 9 файлов, все проверены
    format!("{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        include_str!("../src/gui/mod.rs"),
        include_str!("../src/gui/layers.rs"),
        include_str!("../src/gui/drums.rs"),
        include_str!("../src/gui/settings.rs"),
        include_str!("../src/gui/keyboard.rs"),
        include_str!("../src/gui/midi_seq.rs"),
        include_str!("../src/gui/fx_chain.rs"),
        include_str!("../src/gui/macros.rs"),
        include_str!("../src/gui/params.rs"),
    )
}

fn panel_registration_order_matches_source_contract_impl(src: &str) -> Vec<(String, &'static str)> {
    const TOP: &str = "TopBottomPanel::top(\"";
    const BOTTOM: &str = "TopBottomPanel::bottom(\"";
    let mut seq = Vec::new();
    let mut rest = src;
    loop {
        match (rest.find(TOP), rest.find(BOTTOM)) {
            (None, None) => break,
            (Some(i), None) => { seq.push(extract_id(&rest[i + TOP.len()..], "top")); rest = &rest[i + TOP.len()..]; }
            (None, Some(j)) => { seq.push(extract_id(&rest[j + BOTTOM.len()..], "bottom")); rest = &rest[j + BOTTOM.len()..]; }
            (Some(i), Some(j)) => {
                if i < j { seq.push(extract_id(&rest[i + TOP.len()..], "top")); rest = &rest[i + TOP.len()..]; }
                else { seq.push(extract_id(&rest[j + BOTTOM.len()..], "bottom")); rest = &rest[j + BOTTOM.len()..]; }
            }
        }
    }
    seq
}

fn extract_id(s: &str, kind: &'static str) -> (String, &'static str) {
    let id = &s[..s.find('"').expect("unterminated panel id")];
    (id.to_string(), kind)
}

#[test]
fn panel_registration_order_matches_source_contract() {
    let src = panel_sources();
    let expected: Vec<(String, &str)> = EXPECTED_PANELS.iter()
        .map(|(id, k)| (id.to_string(), *k)).collect();
    let seq = panel_registration_order_matches_source_contract_impl(&src);
    assert_eq!(seq, expected,
        "panel registration order changed — egui carves in show() order; \
         update EXPECTED_PANELS only with layout-diff review");
    // CentralPanel строго после последней bottom-панели (карв-канон)
    let central = src.find("CentralPanel::default()").expect("CentralPanel must exist");
    let last_bottom = src.rfind("TopBottomPanel::bottom(\"").expect("bottom panels must exist");
    assert!(central > last_bottom, "CentralPanel must be registered last");
}
```

Примечание: длина иглы берётся из констант TOP/BOTTOM (`.len()` в `i + TOP.len()..`) — прежние magic 21/24 удалены. Скан ищет во всех gui-файлах; если появятся false-positive вызовы в дочерних модулях (панели создаются только в mod.rs::update) — сузить список файлов panel_sources() до реально содержащих TopBottomPanel.

- [x] **Step 2: зелёный**

Run: `cargo test --test ui_layout_contract 2>&1 | tail -3`
Expected: 2 passed (geometry mirror + новый скан). Если скан находит панели в НЕ-mod.rs файлах — сверить с реальной регистрацией (в false-positive случае сузить список файлов до реально содержащих TopBottomPanel).

- [x] **Step 3: коммит**

```bash
git add tests/ui_layout_contract.rs && git commit -m "test: panel-order pin scans all gui files, derived needle lengths, asserts CentralPanel-last (QW9)"
```

### Task 10: QW10 — extract дублей pedal-up и latch-mismatch

**Files:**
- Modify: `src/synth/mod.rs` — impl Part: новый `release_pedal_held` + `release_latch_mismatch`; call sites :1740-1780, :2670-2708, :2394-2401, :2536-2543

- [x] **Step 1: release_pedal_held** — в impl Part (рядом с drain_notes :1103):

```rust
    /// Pedal-up: разжать pedal-held (не key-held) голоса парта. Общий код
    /// set_sustain и SetPartSustain (QW10: verbatim ×2 — дрейф запрещён).
    /// `secs` = static pedal-up release (#7), None = patch release.
    fn release_pedal_held(&mut self, secs: Option<f32>) {
        // Latch voices: released unconditionally (см. latch_held контракт —
        // latched note определяется latch_held, не keys_down).
        for &n in &self.latch_held {
            self.keys_down[n as usize] = false;
        }
        if let Some(secs) = secs {
            // Two passes: needs-битмап держит borrows disjoint.
            let mut needs = [false; 128];
            for voice in &self.voices {
                if voice.active && !voice.is_releasing()
                    && !self.keys_down[voice.src_note as usize]
                {
                    needs[voice.note as usize] = true;
                }
            }
            for (note, &held) in needs.iter().enumerate() {
                if held { self.note_off_for(note as u8, secs); }
            }
        } else {
            for voice in &mut self.voices {
                if voice.active && !voice.is_releasing()
                    && !self.keys_down[voice.src_note as usize]
                {
                    voice.note_off();
                }
            }
        }
        self.sustained_notes = [false; 128];
        // Stale mono/latch entries would resurrect on the next key press.
        self.note_stack.clear();
        self.latch_held.clear();
    }

    /// Latch-mismatch: уход с play_mode==3 (latch) — снять latched голоса.
    /// Общий код ProgramChange и LoadPatch (QW10). Вызывать ДО swap params.
    fn release_latch_mismatch(&mut self, new_play_mode: u8) {
        if self.params.play_mode as u8 == 3 && new_play_mode != 3 {
            for voice in &mut self.voices {
                if voice.active && self.latch_held.contains(&voice.src_note) {
                    voice.note_off();
                }
            }
            self.latch_held.clear();
        }
    }
```

- [x] **Step 2: заменить 4 call-site** — set_sustain (:1740-1780): захватить `let sr = self.sustain_release;` ДО цикла `for (i, part) in self.parts.iter_mut()`, тело `if !on { ... }` → `if !on { part.release_pedal_held(sr); }` (SF2-строка :1739 остаётся). SetPartSustain (:2670-2708): тело `if !on && l.sustain_pedal > 0.5 { ... }` → `{ l.release_pedal_held(self.sustain_release); }` (set_part_sustain :2668 остаётся). ProgramChange (:2394-2401) → `l.release_latch_mismatch(params.play_mode as u8);`. LoadPatch (:2536-2543) → так же.

- [x] **Step 3: гейт — behavior-preserving**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok" && cargo test --features rt-assert --test rt_safety 2>&1 | tail -1`
Expected: всё зелёное (sustain_audit.rs и sustain-тесты пинят семантику).

- [x] **Step 4: коммит**

```bash
git add src/synth/mod.rs && git commit -m "refactor: extract Part::release_pedal_held/release_latch_mismatch — verbatim dup blocks (QW10)"
```

### Task 11: QW11 — LFO1 мёртв в mod-matrix (bugfix, TDD)

**Files:**
- Modify: `src/synth/mod.rs` — Part поле `lfo1_routed` (:617-621 рядом с lfo2/3/4), init (:655-676), update_routing_cache (:681-694), гейт (:3455-3459)
- Test: `src/synth/tests.rs`

- [x] **Step 1: падающий тест** — в конец src/synth/tests.rs:

```rust

// ── QW11: LFO1 routed через mod-matrix обязан тикать ─────────────────

#[test]
fn tc_lfo1_routed_cache() {
    let mut e = SynthEngine::new(48_000.0);
    // Lfo1-слот с depth != 0, все direct depths = 0
    let mut part = &mut e.parts[0];
    part.params.lfo_pitch_depth = 0.0;
    part.params.lfo_filter_depth = 0.0;
    part.params.lfo_amp_depth = 0.0;
    part.mod_matrix.slots[0] = crate::synth::mod_matrix::ModSlot {
        source: crate::synth::mod_matrix::ModSource::Lfo1,
        dest: crate::synth::mod_matrix::ModDest::Pitch,
        depth: 0.5,
        ..Default::default()
    };
    part.update_routing_cache();
    assert!(part.lfo1_routed, "Lfo1 slot with depth must set lfo1_routed");
    // depth = 0 → false
    part.mod_matrix.slots[0].depth = 0.0;
    part.update_routing_cache();
    assert!(!part.lfo1_routed);
}
```

(Если ModSlot/ModDest поля называются иначе — сверить с mod_matrix.rs и подставить реальные; `..Default::default()` убрать, если ModSlot без Default — заполнить поля явно.)

- [x] **Step 2: красный**

Run: `cargo test --lib tc_lfo1_routed 2>&1 | tail -4`
Expected: FAIL — поля `lfo1_routed` не существует (E0560/E0609).

- [x] **Step 3: реализация** — три точки:
(a) decl рядом с lfo2_routed (:617-621):
```rust
    lfo1_routed: bool,
```
(b) init в Part::new (рядом lfo2_routed init):
```rust
            lfo1_routed: false,
```
(c) update_routing_cache (:681) — первой строкой:
```rust
        self.lfo1_routed = self.mod_matrix.slots.iter()
            .any(|s| s.source == mod_matrix::ModSource::Lfo1 && s.depth.abs() > 0.001);
```
(d) гейт lfo1 (:3455-3459):
```rust
            let lfo1_active = lfo1_wf == LfoWaveform::StepSeq
                || part.lfo1_routed
                || part.params.lfo_pitch_depth > 0.001
                || part.params.lfo_filter_depth > 0.001
                || part.params.lfo_amp_depth > 0.001
                || self.aftertouch_smooth > 0.001;
```

- [x] **Step 4: зелёный + goldens**

Run: `cargo test --lib tc_lfo1_routed 2>&1 | tail -2 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: тест PASS, всё зелёное. ЕСЛИ preset goldens красные — это ОЖИДАЕМОЕ behavior-fix (фабричный пресет маршрутизирует Lfo1): прогнать `MINI_MIDI_GOLDEN_UPDATE=1 cargo test --test golden_fx`, в коммит-сообщение записать «intentional behavior fix: LFO1 matrix routing now audible, goldens regenerated» + экранную проверку юзером.

- [x] **Step 5: коммит**

```bash
git add src/synth/mod.rs src/synth/tests.rs && git commit -m "fix: LFO1 audible when routed via mod matrix only — lfo1_routed cache + gate (QW11)"
```

### Task 12: Верификация + релизный цикл

- [x] **Step 1: полный гейт**

Run: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety && cargo deny check`
Expected: всё зелёное.

- [x] **Step 2: PR → squash merge → rebuild release → relaunch** (канон; сеть — `unset LD_PRELOAD` для gh/push).

- [x] **Step 3: Экран (юзер)** — фейдеры reverb/delay (CC66/CC54) работают после рестарта с сохранённым cc_map; GM-pick на части ≥2 не портит seed; LFO1-маршрутизация слышна (если в пресетах есть).

- [x] **Step 4: TECHDEBT.md** — добавить НОВУЮ датированную подсекцию «## QW-батч 2026-11-07 (refactor-audit) — ГОТОВО» со списком QW1-QW11 и 3 закрытых бага (CC scope flip, GM-pick layer_b, LFO1-matrix) с якорями. НЕ трогать старую статус-строку REFACTOR_AUDIT (docs/TECHDEBT.md:767) — там ДРУГАЯ нумерация QW (старый аудит: QW1=wavetable alloc и т.д.), пометка «QW1-QW11 done» falsely закроет старые QW4/QW5/QW7.

## Out of scope

- Стратегические эпики ST1-ST8 (S7-lite, транзакции, S2, S3, S4, S6, F3-персист, headless) — отдельные планы.
- Range-пин слайдеров (150 мест) — уходит в S2-план.
- Vec::push в RT-нотах — при попутном touch в S4.
