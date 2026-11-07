# Stage 2a: CC-live параметры — SSOT макро-таблица Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (база = master 737005e; канон — Tavily 2026-11-07/08:
SSOT-таблица + макро-генерация = nih-plug derive Params паттерн; подсечение
полного S2 — см. Out of scope)

**Goal:** Один макро-список 31 CC-live patch-ключа порождает: (1) getter
`Part::cc_value()`, (2) соответствующие arm'ы `Part::set_param`, (3) таблицу
range'ов для cc_map::PARAM_REGISTRY; check_pickup покрывает ВСЕ registry-ключи
(сейчас 4/32 — «ручка кажется мёртвой» из аудита S2).

**Architecture:** `cc_patch_params!` macro_rules! в patch_params.rs — единый
список `"key" => field`. Расширения: `CC_PARAM_RANGES` (key, min, max, log) —
из него собирается patch-часть PARAM_REGISTRY (labels/scope/global остаются в
cc_map); `Part::cc_value(key) -> Option<f32>` (getter-match); set_param
сохраняет ВСЕ 107 arm'ов, но 31 из них генерятся макросом (whitelist-семантика
не меняется). Мин/макс в таблице = ЕДИНЫЙ источник range'ов для CC-биндингов
(range-дрейф ks_brightness из аудита живёт в GUI-слайдерах — не в этой
подсекции; тут CC-часть становится консистентной).

**Tech Stack:** Rust declarative macros (TLBORM канон); пины: существующие
source-parse тесты ПЕРЕПИСЫВАЮТСЯ осознанно (макро-генерация невидима
include_str-парсеру — см. Task 2); goldens не затрагиваются (поведение
check_pickup меняется ТОЛЬКО для 27 ранее не покрыватых ключей: pickup
начинает работать — behavior fix, CC-движение до пересечения целевого значения
раньше давало прыжок; это и есть фикс).

**Проверенные факты (HEAD 737005e):**
- PARAM_REGISTRY cc_map.rs:26-60 — 34 ParamMeta = 2 master-global + 31 patch
  (включая Global-scope reverb_mix/delay_mix) + pitch_bend_range. У GLOBAL ключей min/max/log СВОИ (master_tone
  200..20000 log) — остаются в cc_map, макрос покрывает только 31 patch-ключ.
- check_pickup synth/mod.rs:2240-2281: globals immediate; 4-arm match
  (filter_cutoff/resonance/amp_attack/amp_release), остальные → current_val=0.0
  → target_cc на дне диапазона → pickup требует прогрузки ручки в 0 (аудит S2).
- set_param (synth/mod.rs, Part impl) — 107 arm'ов, все 31 patch-ключа
  registry присутствуют; whitelist-семантика (fallback debug_assert QW8).
- Source-parse пины (synth/tests.rs): match_arms_between("fn set_param...") —
  `tc_set_param_arms_synced_with_map` (>100 arm'ов) и
  `tc_cc_bindings_target_set_param_arms`. После Task 1 arm'ы 31 ключа уедут в
  макро-расширение в patch_params.rs — тест-парсер должен читать НОВОЕ место
  (осознанная правка пина, Task 2).
- macro_vals/organ_drawbars/geq — массивные ключи, НЕ входят в CC-live список
  (в registry их нет).
- CC-диспетчер (synth/mod.rs:2554-2560) — вызывает check_pickup и
  part.set_param по binding.scope; не меняется.

---

## Test Cases (from the requirement)

- TC1: `cc_value` для каждого из 31 ключей возвращает текущее поле (set через
  set_param — круг: set_param("filter_cutoff", 12345.0) → cc_value == 12345.0).
- TC2: `cc_value("master_volume")` → None (глобальные не в patch-таблице).
- TC3 (behavior fix): check_pickup для порта pickup-состояния: биндинг
  chorus_mix (не в старых 4 arm'ах), текущее значение 0.8 при range 0..1 →
  target_cc = 102 (≈0.8*127), НЕ 0 — пин на пересчёт target от реального
  значения (старый код дал бы 0).
- TC4 (registry): для каждого ParamMeta с scope=Patch — (min,max,log) ==
  запись в CC_PARAM_RANGES; для global — не в таблице.
- TC5 (регрессия пинов): tc_set_param_arms_synced_with_map (переписанный) —
  все arm'ы set_param (рукописные + макро) ↔ to_map; связка не теряется.
- TC6 (goldens): behavior-preserving для существующих пресетов — goldens
  зелёные БЕЗ регенерации (check_pickup не в аудио-пути рендера).

## Negative Matrix

- ~~null/missing field~~ — ключи статические (&'static str), resolve_key гейтит.
- Неизвестный ключ в cc_value → None (вне whitelist) — семантика fallback'а.
- Дубликат ключа в макро-списке → дубль arm'а = compile error (duplicate match
  arm binding не ошибка в match!... наоборот: match arm с одинаковым литералом
  — unreachable arm warning = clippy -D поймает). Пин-тест длины 31 — дубль
  сдвинул бы count.
- Global ключи в макро-списке → TC4 их отсекает (нет в CC_PARAM_RANGES).
- macro_vals/geq-массивы — не в списке, cc_value → None.
- CC-биндинг с ручными min/max из конфига — from_config сохраняет
  raw_binding.min_val (не из таблицы) — ЮЗЕРСКИЙ override остаётся старшим
  (осознанно: learn пишет фактические range'ы контрола).
- Пустой pickup_states → существующая логика, не меняется.
- Повторный вызов cc_value в RT — match по &'static str, zero-alloc, RT-safe
  (rt_safety кейс не нужен: не в callback hot path... check_pickup вызывается
  из handle_control CC-пути — RT! match — ок).

---

### Task 1: cc_patch_params! макрос + cc_value + CC_PARAM_RANGES

**Files:**
- Modify: `src/synth/patch_params.rs` — макрос + константы (в конец файла)
- Modify: `src/synth/mod.rs` — Part::cc_value (рядом с set_param), вызов
  макроса в set_param для 31 arm'ов, check_pickup (4-arm → cc_value)
- Test: `src/synth/tests.rs`

- [ ] **Step 1: макрос в patch_params.rs** (в конец файла):

```rust
/// CC-live patch-параметры — SSOT (S2a): один список порождает getter
/// (Part::cc_value), arm'ы Part::set_param и таблицу range'ов
/// CC_PARAM_RANGES для cc_map::PARAM_REGISTRY. Новый CC-live параметр:
/// добавить строку ЗДЕСЬ — больше нигде (кроме label/scope в cc_map).
macro_rules! cc_patch_params {
    ($($key:literal => $field:ident : $min:literal .. $max:literal : $log:literal),+ $(,)?) => {
        /// (key, min, max, logarithmic) — единственный источник range'ов
        /// для CC-биндингов этих ключей. Пинится тестом к PARAM_REGISTRY
        /// (tc_registry_ranges_match_cc_table); не-тестовый потребитель — 2b.
        #[allow(dead_code)]
        pub const CC_PARAM_RANGES: &[(&str, f32, f32, bool)] = &[
            $( ($key, $min, $max, $log), )+
        ];
        /// CC-live patch ключи (для пинов и cc_map-проверок). Потребители
        /// пока только в тестах → allow до подсечения 2b (ревью: иначе
        /// dead_code уронит clippy; прецедент ParamMeta cc_map.rs:13).
        #[allow(dead_code)]
        pub const CC_PATCH_KEYS: &[&str] = &[ $( $key ),+ ];
    };
}

cc_patch_params! {
    "filter_cutoff"     => filter_cutoff     : 20.0   .. 20000.0 : true,
    "filter_resonance"  => filter_resonance  : 0.0    .. 1.0     : false,
    "filter_env_amount" => filter_env_amount : 0.0    .. 15000.0 : false,
    "amp_attack"        => amp_attack        : 0.001  .. 5.0     : true,
    "amp_decay"         => amp_decay         : 0.0    .. 5.0     : false,
    "amp_sustain"       => amp_sustain       : 0.0    .. 1.0     : false,
    "amp_release"       => amp_release       : 0.001  .. 5.0     : true,
    "filter_attack"     => filter_attack     : 0.001  .. 5.0     : true,
    "filter_decay"      => filter_decay      : 0.0    .. 5.0     : false,
    "filter_sustain"    => filter_sustain    : 0.0    .. 1.0     : false,
    "filter_release"    => filter_release    : 0.001  .. 5.0     : true,
    "lfo_rate"          => lfo_rate          : 0.1    .. 20.0    : true,
    "lfo_pitch_depth"   => lfo_pitch_depth   : 0.0    .. 1.0     : false,
    "lfo_filter_depth"  => lfo_filter_depth  : 0.0    .. 1.0     : false,
    "lfo_amp_depth"     => lfo_amp_depth     : 0.0    .. 1.0     : false,
    "vel_to_filter"     => vel_to_filter     : 0.0    .. 1.0     : false,
    "noise_level"       => noise_level       : 0.0    .. 1.0     : false,
    "osc_detune"        => osc_detune        : 0.0    .. 0.05    : false,
    "portamento_time"   => portamento_time   : 0.0    .. 2.0     : false,
    "unison_detune"     => unison_detune     : 0.0    .. 50.0    : false,
    "chorus_mix"        => chorus_mix        : 0.0    .. 1.0     : false,
    "delay_mix"         => delay_mix         : 0.0    .. 1.0     : false,
    "delay_time_l"      => delay_time_l      : 0.01   .. 2.0     : false,
    "delay_time_r"      => delay_time_r      : 0.01   .. 2.0     : false,
    "delay_feedback"    => delay_feedback    : 0.0    .. 0.95    : false,
    "delay_filter"      => delay_filter      : 0.0    .. 0.95    : false,
    "reverb_mix"        => reverb_mix        : 0.0    .. 1.0     : false,
    "reverb_room_size"  => reverb_room_size  : 0.0    .. 1.0     : false,
    "reverb_damping"    => reverb_damping    : 0.0    .. 1.0     : false,
    "reverb_width"      => reverb_width      : 0.0    .. 1.0     : false,
    "reverb_pre_delay"  => reverb_pre_delay  : 0.0    .. 0.1     : false,
}
```

Range'ы = ТОЧНО копия текущих PARAM_REGISTRY (cc_map.rs:26-60), сверить
каждый построчно при переносе. 31 запись.

- [ ] **Step 2: getter** — в impl Part (рядом с set_param):

```rust
    /// Текущее значение CC-live patch-параметра (S2a: SSOT-макрос
    /// cc_patch_params! в patch_params.rs). None = ключ не CC-live.
    fn cc_value(&self, key: &str) -> Option<f32> {
        crate::synth::patch_params::cc_value_of(&self.params, key)
    }
```

и свободная fn в patch_params.rs (макрос расширяет):

```rust
/// Значение поля по CC-live ключу (генерится из cc_patch_params!).
pub fn cc_value_of(p: &PatchParams, key: &str) -> Option<f32> {
    match key {
        $( $key => Some(p.$field), )+
        _ => None,
    }
}
```

(расширить макрос третьим выходом: `pub fn cc_value_of` — добавить в
макро-тело arm `$($key => Some(p.$field),)+` внутри fn).

- [ ] **Step 3: set_param делегирование** (ЕДИНСТВЕННЫЙ путь — макрос НЕ
  может расширяться в arm-позицию чужого match на stable Rust; ревью 2026-11-08).
  В макрос добавить четвёртый выход и в Part::set_param — 31 рукописных arm'ов
  удалить, делегирование стоит ПЕРВЫМ:

```rust
    fn set_param(&mut self, key: &str, value: f32) {
        // 31 CC-live ключ — SSOT-таблица (S2a); остальные — рукописные arm'ы
        if crate::synth::patch_params::cc_set_param(&mut self.params, key, value) {
            return;
        }
        match key {
            ...
```

cc_set_param генерится макросом: `match key { $($key => { p.$field = value; true },)+ _ => false }`.

- [ ] **Step 4: check_pickup** — 4-arm блок (:2250-2258) заменить:

```rust
        let current_val = self.parts.first()
            .and_then(|l| l.cc_value(binding.param_key))
            .unwrap_or(0.0);
```

- [ ] **Step 5: тесты** — src/synth/tests.rs:

```rust

// ── S2a: CC-live SSOT ────────────────────────────────────────────────

#[test]
fn tc_cc_value_roundtrip() {
    let mut e = SynthEngine::new(48_000.0);
    for &key in crate::synth::patch_params::CC_PATCH_KEYS {
        e.parts[0].set_param(key, 0.42);
        let v = e.parts[0].cc_value(key);
        assert_eq!(v, Some(0.42), "roundtrip {key}");
    }
}

#[test]
fn tc_cc_value_globals_and_unknown_are_none() {
    let e = SynthEngine::new(48_000.0);
    assert_eq!(e.parts[0].cc_value("master_volume"), None);
    assert_eq!(e.parts[0].cc_value("pitch_bend_range"), None);
    assert_eq!(e.parts[0].cc_value("no_such_key"), None);
    assert_eq!(e.parts[0].cc_value("macro_0"), None);
}

#[test]
fn tc_check_pickup_uses_real_value_not_zero() {
    // behavior fix: ключ вне старых 4 arm'ов — target от реального значения
    let mut e = SynthEngine::new(48_000.0);
    e.parts[0].set_param("chorus_mix", 0.8);
    let binding = crate::cc_map::CcBinding {
        param_key: "chorus_mix", min_val: 0.0, max_val: 1.0,
        logarithmic: false, scope: crate::cc_map::ParamScope::Patch,
    };
    // убедиться, что pickup-состояние не picked_up:
    e.pickup_states[b'0' as usize] = crate::synth::PickupState {
        last_cc_value: None, picked_up: false,
    };
    // Прямой вызов check_pickup (метод SynthEngine, тест в том же модуле):
    let crossed = e.check_pickup(b'0', 102, binding);
    assert!(!crossed, "first call: last=None -> stores, returns false");
    let crossed2 = e.check_pickup(b'0', 127, binding);
    assert!(crossed2, "0.8 of 0..1 => target_cc=102; 127 crosses it");
}
```

ИСПОЛНИТЕЛЮ: check_pickup — метод SynthEngine (`fn check_pickup(&mut self,
cc: u8, ...)`), тест в том же модуле — прямой вызов. PickupState поля видны.
Математика пина: param_to_cc non-log = round(0.8*127)=102; первый вызов
last=None → false; второй с 127 → crossed=true. ДО фикса current_val=0.0 →
target=0 → crossed2=false (тест красный до реализации).

- [ ] **Step 5b: переписать source-parse пины (ИЗ Task 2 — ревью: иначе
  гейт Task 1 красный, т.к. 31 arm уезжает из парсер-текста set_param):**
  tc_set_param_arms_synced_with_map: match_arms_between теперь даёт ~76
  рукописных arm'ов — заменить assert >100 на >70 И добавить:
  `for &k in CC_PATCH_KEYS { assert!(map.contains_key(k), "{k}"); }`
  (белый-list CC-live ключей ↔ to_map). tc_cc_bindings_target_set_param_arms:
  union known = parsed_arms ∪ CC_PATCH_KEYS ∪ global_arms.

- [ ] **Step 6: красный → зелёный → гейт**

Run: `cargo test --lib tc_cc_value tc_check_pickup tc_set_param_arms tc_cc_bindings 2>&1 | tail -6` (красный: нет cc_value/CC_PATCH_KEYS) → реализация → зелёный.
Полный гейт: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok" && cargo test --test golden_fx 2>&1 | tail -2`

- [ ] **Step 7: коммит**

```bash
git add src/synth/patch_params.rs src/synth/mod.rs src/synth/tests.rs && git commit -m "feat: CC-live param SSOT table — cc_value getter, generated set arms, full pickup coverage (S2a)"
```

### Task 2: cc_map::PARAM_REGISTRY из таблицы + пины

**Files:**
- Modify: `src/cc_map.rs:24-61` — patch-часть PARAM_REGISTRY собирается из
  `crate::synth::patch_params::{CC_PARAM_RANGES, CC_PATCH_KEYS}`; labels
  остаются здесь (map key→label), scope/global — здесь.
- Modify: `src/synth/tests.rs` — tc_set_param_arms_synced_with_map:
  источник arm'ов теперь ДВА места (макро-таблица в patch_params.rs +
  рукописный match в mod.rs) — парсер match_arms_between("fn set_param…")
  теперь даёт ~76 рукописных; добавить проверку: каждый ключ
  CC_PATCH_KEYS ∈ to_map (заменяет утерянные макро-arm'ы в парсере);
  tc_cc_bindings_target_set_param_arms — аналогично дополнить
  CC_PATCH_KEYS-набором.

- [ ] **Step 1: registry из таблицы** — patch-записи PARAM_REGISTRY:

```rust
pub const PARAM_REGISTRY: &[ParamMeta] = &[
    // Global params (persist across patch changes)
    ParamMeta { key: "master_volume", label: "Volume", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Global },
    ParamMeta { key: "master_tone", label: "Tone", min: 200.0, max: 20000.0, logarithmic: true, scope: ParamScope::Global },
    // Patch params — range SSOT: patch_params::CC_PARAM_RANGES (S2a),
    // labels живут здесь
    ParamMeta { key: "filter_cutoff", label: "Filter Cutoff", ..range_of("filter_cutoff") },
    ...
];
```

РЕШЕНО ревью: registry остаётся ЛИТЕРАЛЬНЫМ + пин-тест равенства с таблицей
(TC4). Альтернативы отклонены: (а) const-fn сборка невозможна в const-контексте;
(б) сборка из cc_map потребовала бы pub(crate) mod patch_params (модуль
приватный, mod.rs:55) — не делать. Расхождение таблицы и registry = красный
тест; единственный источник range'ов — CC_PARAM_RANGES.

- [ ] **Step 2: пин-тесты** — src/synth/tests.rs:

```rust
#[test]
fn tc_registry_ranges_match_cc_table() {
    for m in crate::cc_map::PARAM_REGISTRY {
        if m.scope == crate::cc_map::ParamScope::Global
            && (m.key == "master_volume" || m.key == "master_tone"
                || m.key == "pitch_bend_range") { continue; }
        let r = crate::synth::patch_params::CC_PARAM_RANGES
            .iter().find(|(k, ..)| *k == m.key)
            .unwrap_or_else(|| panic!("registry key '{}' not in CC_PARAM_RANGES", m.key));
        assert_eq!((m.min, m.max, m.logarithmic), (r.1, r.2, r.3),
            "range drift for '{}'", m.key);
    }
    assert_eq!(crate::synth::patch_params::CC_PATCH_KEYS.len(), 31);
}
```

(Примечание: reverb_mix/delay_mix — Global scope, но В таблице есть; исключение
в continue-условии только для трёх действительно-несинтовых глобальных.)

- [ ] **Step 3: переписать source-parse пины** — tc_set_param_arms_synced_with_map:
  `match_arms_between` даёт только рукописные arm'ы (~76); проверка
  дополнить: `for &k in CC_PATCH_KEYS { assert!(map.contains_key(k)); }`;
  tc_cc_bindings_target_set_param_arms — известное множество дополнить
  CC_PATCH_KEYS (сейчас сверяется с парс-результатом — макро-arm'ы больше
  не в парс-тексте: `patch_arms` + `CC_PATCH_KEYS` как union).

- [ ] **Step 4: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok" && cargo test --test golden_fx 2>&1 | tail -2`

- [ ] **Step 5: коммит**

```bash
git add src/cc_map.rs src/synth/tests.rs && git commit -m "refactor: cc_map registry pinned to CC_PARAM_RANGES SSOT, source-parse pins updated for macro arms (S2a/2)"
```

### Task 3: Верификация стадии

- [ ] **Step 1:** полный гейт (clippy/test/rt_safety/deny/gui-less check).
- [ ] **Step 2:** PR → squash → rebuild → launch-check → roadmap чекбокс
  (частично: полный S2 — GUI-слайдеры и build_voice_params — остаётся;
  пометить «2a done, 2b (GUI sliders) awaiting screen check»).
- [ ] **Step 3:** TECHDEBT подсекция + «УТРОМ: что проверить» пункт: MIDI-learn
  на новом ключе (напр. chorus_mix на CC) — pickup: ручка подхватывает БЕЗ
  прыжка в 0.
- [ ] **Step 4:** Tavily постчек: macro-generated match arms — гигиена/канон.

## Out of scope (полный S2 — подсечения 2b/2c)

- GUI-слайдеры (150 мест) на таблицу — 2b, требует визуальной проверки.
- build_voice_params (111 копий) / CachedFxParams на макро — 2c.
- from_map/to_map унификация — 2c.
