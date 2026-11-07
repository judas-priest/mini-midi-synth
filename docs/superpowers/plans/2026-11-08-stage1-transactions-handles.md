# Stage 1 (ST2+ST1): Транзакции SetPartFull + EngineHandles Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

status: executed 2026-11-08 (субагентно; 3 задачи + ревью; PR стадии 1)

anchor-checked: 2026-11-08 (база = master 7a5d021; анкоры собраны субагентом с HEAD,
все сниппеты verbatim; канон-исследование Tavily 2026-11-08: single boxed payload
per gesture — SPSC-канон, allocation на GUI-потоке, zero-alloc на аудио; Arc
снапшот — альтернатива, отклонена в пользу Box (trash-канон уже есть — LoadPatch);
EngineHandles — Facade/Parameter Object, Clone по Arc-полям)

**Goal:** Свап парта = 2 события в кольцо вместо 42-44 (сейчас разрыв кольца =
half-swap без детекции); реконнект/спавн парта = 1 событие вместо 20-21; 18
хэндлов движка правятся в 3 местах вместо 6.

**Architecture:** `ControlEvent::SetPartFull { part, Box<PartFullState> }` — handler
КОМПОЗИРУЕТ существующие хэндлеры (рекурсивный вызов handle_control в порядке
push_full_part_state, LoadPatch последним) — семантический паритет по
конструкции, единственный источник правды не дублируется. Box отдаётся в trash
по существующему `#[must_use]` контракту (канон LoadPatch). PartFullState несёт
`patch: Option<Box<LoadPatchEvent>>` — GUI собирает через существующий
`load_patch_from_edited`. ST1: `EngineHandles` struct (Clone по Arc-полям) +
`SynthEngine::handles()`; CommonInit/CommonInit-литерал/реконнект потребляют
структуру, App-поля остаются индивидуальными.

**Tech Stack:** Rust; TDD на parity-тестах (engine: последовательность событий vs
транзакция на свежих движках); GUI-сторона — гейты + существующие пины
(sf2_channel_gain порядок, rt_safety). Гейты: clippy -D warnings + cargo test +
rt_safety + cargo deny.

**Проверенные факты (вычитано с HEAD 7a5d021; не пересматривать):**
- `push_full_part_state` (gui/mod.rs:2444-2480): порядок событий SetPartEnabled,
  SetPartMute, SetPartVolume, SetPartRange, SetPartVelRange, SetPartPan,
  SetPartTranspose, SetPartNoteMode, SetPartSustain, SetPartIgnorePb,
  SetPartIgnoreMw, SetPartSf2Mode, 8×SetMacro, [SetPartSf2Program if sf2] = 20-21.
- `do_swap_parts_push` (gui/mod.rs:2524-2532): 2×push_full_part_state +
  2×send_edited_params + persist = 42-44 событий.
- `send_edited_params` (gui/mod.rs:2414-2419) → `ControlEvent::load_patch_from_edited`
  (synth/mod.rs:395-425) строит `LoadPatch(Box<LoadPatchEvent>)`.
- Хэндлеры с change-detection: SetPartMute (drain только на переходе),
  SetPartSf2Mode (no-op если не изменился; flip → drain + sampler.set_part_mode +
  re-apply CC10/CC7) — композиция через рекурсию сохраняет их семантику ТОЧНО.
- Heap-return контракт: `handle_control` `#[must_use]` (synth/mod.rs:2505-2509);
  аудио-callback отдаёт spent в trash (audio.rs:193-194); LoadPatch handler
  возвращает опустошённый Box с старым wavetable/mseg назад (synth/mod.rs:2511-2583).
- Хэндл-пуллы: init_common lib.rs:225-254 (17 шт), CommonInit struct :81-146,
  литерал :329-391; App-литерал :773-979 (31 назначение из c.*); реконнект
  settings.rs:269-295 (17 пуллов). engine.set_program_change_atom клон
  :271 (движку ATOM отдаётся, не пуллится).
- PartState (gui/mod.rs:156-188) — GUI-зеркало; engine Part не хранит patch_idx.
- Тесты: part_control_events_land_in_engine_fields (tests.rs:1166-1202),
  full_state_re_send_restores_a_fresh_engine (:1204-1241) + helper
  push_full_part_state в тестах (:1243-1262, уже отстаёт от GUI на 4 поля);
  sf2_channel_gain.rs:28-42 пинит apply-порядок (SetPartSf2Mode ПОСЛЕ gains);
  rt_safety использует SetPartEnabled в rt-зонах.
- Recursion guard: LoadPatch handler НЕ вызывает handle_control → глубина
  рекурсии SetPartFull = 1. `_ => {}` в матче — default arm остаётся.

---

## Test Cases (from the requirement)

- TC1 (parity, ядро стадии): два свежих движка; первому — ПОСЛЕДОВАТЕЛЬНОСТЬ
  индивидуальных событий (канонический порядок push_full_part_state + LoadPatch),
  второму — ОДИН SetPartFull с тем же PartFullState. Наблюдаемые состояния равны:
  enabled/mute/volume/min-max/vel/pan/transpose/note_mode/sustain_enabled/
  ignore_pb/ignore_mw/sf2_mode/macro_vals + params (patch применился).
- TC2 (parity sf2): sf2_mode=true + program=5 — обе ветки сходятся в sf2_mode.
- TC2b (macros без patch): SetMacro-ветка наблюдаема (0.1*i в macro_vals) —
  без LoadPatch-перезаписи (сценарий реконнекта/спавна).
- TC3 (mute parity): SetPartMute НЕ имеет change-detection (ревью) — mute=true
  на замьюченном парте drain'ит И в последовательности, И в транзакции; тест
  пинит РАВЕНСТВО веток, не no-drain.
- TC4 (heap-return): SetPartFull с patch:Some на парте с активным wavetable —
  handle_control возвращает Some(LoadPatch) с старым wavetable назад (trash-контракт).
- TC5 (ring-емкость): свап = ровно 2 события (конструктивно — GUI шлёт 2 SetPartFull;
  проверка код-ревью + отсутствие промежуточных push в diff).
- TC6 (handles): `engine.handles()` возвращает все 17-18; подключения в
  reconnect_audio идемпотентны (audio_hotplug политику не ломает).

## Negative Matrix

- ~~null~~ — нет Option-полей кроме patch; patch=None = патч не трогается.
- part ≥ parts.len() в SetPartFull → handler игнорирует (get_mut None; патч-ветка
  тоже гвардится), паритет с индивидуальными хэндлерами.
- Кольцо переполнено перед SetPartFull → push Err, событие НЕ доставлено ЦЕЛИКОМ
  (это и есть цель: нет half-state), `let _ =` канон (CountingProducer считает).
- Рекурсия: SetPartFull внутри SetPartFull невозможна (GUI не строит; handler
  строит только известные варианты).
- LoadPatch возвращает Some → SetPartFull handler должен ПРОБУЛИТЬ его в trash
  (return Some), не глотать — иначе утечка Box в аудио-потоке (rt_safety поймает
  drop в callback только если тест шлёт патч в rt-зоне; добавляем в TC4).
- Одинаковые значения (nothing changed) → change-detection хэндлеров НЕ drain'ит
  (TC3), реконнект на свежем движке — state и так дефолтный.
- GUI PartState.patch_idx НЕ в PartFullState (engine его не хранит) — не тащить.
- macro index ≥8 → существующий guard SetMacro (кламп/игнор) — не меняем.

---

### Task 1: ST1 — EngineHandles struct

**Files:**
- Modify: `src/lib.rs` — struct EngineHandles (~:81, рядом с CommonInit),
  SynthEngine::handles() нельзя — движок в synth; метод делаем free-fn в lib.rs
  `fn engine_handles(engine: &synth::SynthEngine) -> EngineHandles` ИЛИ impl-блок
  в synth/mod.rs `pub fn handles(&self) -> crate::EngineHandles` (поле типов из
  lib — смотри: атомики типов Arc<AtomicU32>/Arc<AtomicBool>/LOOPER типов из
  drum/looper модулей; типы общедоступны) — ВЫБОР: handles() в synth/mod.rs,
  struct EngineHandles в lib.rs (pub), synth ссылается crate::EngineHandles.
- Modify: `src/synth/mod.rs` — `pub fn handles(&self) -> crate::EngineHandles`
  в impl SynthEngine (метод-фабрика, 17 пуллов из init_common :225-254).
- Modify: `src/gui/settings.rs:269-295` — реконнект: `let h = engine.handles();`
  + присвоения `self.X = h.X;`.

- [x] **Step 1: struct** — в src/lib.rs после CommonInit (или перед ним):

```rust
/// Единый бандл общих хэндлов движка (ST1: 18 хэндлов × 6 мест → 3).
/// Клон = клон Arc'ов (GUI-поток). Новый хэндл: поле здесь + строка в
/// SynthEngine::handles() + присвоение потребителю — больше нигде.
#[derive(Clone)]
pub struct EngineHandles {
    pub drum_step_atom: std::sync::Arc<std::sync::atomic::AtomicU32>,
    pub drum_play_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "gui")]
    pub drum_rec_atom: std::sync::Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "gui")]
    pub drum_pattern_atom: std::sync::Arc<std::sync::atomic::AtomicU32>,
    #[cfg(feature = "gui")]
    pub drum_undo_atom: std::sync::Arc<..>,   // типы СПИСАТЬ С CommonInit полей
    #[cfg(feature = "gui")]
    pub drum_redo_atom: std::sync::Arc<..>,
    pub looper_atoms: ..,
    pub looper_display: ..,
    pub seq_target_atom: ..,
    pub active_part_atom: ..,
    pub sustain_atom: ..,
    pub master_rec_state_atom: ..,
    pub master_rec_cycle_atom: ..,
    pub pitch_seq_step_atoms: ..,
    pub scope_buf: ..,
    pub midi_seq_play_atom: ..,
    pub midi_seq_pos_atom: ..,
}
```

ИСПОЛНИТЕЛЮ: типы 17 полей списать ТОЧНО с полей CommonInit (lib.rs:81-146).
Ревью-уточнение: step/play/rec/pattern = `Arc<AtomicU8>`; undo/redo =
`Arc<AtomicBool>` (сверять с CommonInit при каждом поле); cfg-гейтов 14 (НЕ 4): gui-gated = drum_step, drum_play, drum_rec,
drum_pattern, drum_undo, drum_redo, looper_atoms, looper_display, sustain_atom,
master_rec_state_atom, pitch_seq_step_atoms, scope_buf, midi_seq_play_atom,
midi_seq_pos_atom; БЕЗ гейта = seq_target_atom, active_part_atom,
master_rec_cycle_atom. Названия полей EngineHandles = имена полей CommonInit
1:1. Гейт после: `cargo check --lib --no-default-features` обязателен.

- [x] **Step 2: метод** — в impl SynthEngine (src/synth/mod.rs, рядом с
  accessors-секцией: seq_target_atom и т.п.):

```rust
    /// Единая точка пулла общих хэндлов (ST1). Порядок полей = CommonInit.
    pub fn handles(&self) -> crate::EngineHandles {
        crate::EngineHandles {
            drum_step_atom: self.drum_engine.step_atom(),
            drum_play_atom: self.drum_engine.play_atom(),
            #[cfg(feature = "gui")]
            drum_rec_atom: self.drum_engine.rec_atom(),
            #[cfg(feature = "gui")]
            drum_pattern_atom: self.drum_engine.pattern_atom(),
            #[cfg(feature = "gui")]
            drum_undo_atom: self.drum_engine.undo_atom(),
            #[cfg(feature = "gui")]
            drum_redo_atom: self.drum_engine.redo_atom(),
            looper_atoms: self.looper.atoms(),
            looper_display: self.looper.display(),
            seq_target_atom: self.seq_target_atom(),
            active_part_atom: self.active_part_atom(),
            sustain_atom: self.sustain_atom(),
            master_rec_state_atom: self.master_rec_state_atom(),
            master_rec_cycle_atom: self.master_rec_cycle_atom(),
            pitch_seq_step_atoms: self.pitch_seq_step_atoms(),
            scope_buf: self.scope_buffer(),
            midi_seq_play_atom: self.midi_player_play_atom(),
            midi_seq_pos_atom: self.midi_player_pos_atom(),
        }
    }
```

- [x] **Step 3: потребители** — init_common (lib.rs:225-254): 17 локальных
  let заменить на `let h = engine.handles();` — литерал CommonInit (:329-391)
  использует `h.drum_step_atom` и т.д. ИЛИ оставить 17 полей CommonInit и
  заполнить их из h (меньше диффа): `let common = CommonInit { drum_step_atom:
  h.drum_step_atom, ... }`. ВЫБОР: оставить поля CommonInit как есть (33 поля),
  литерал из h — минимальный дифф, fan-out сокращается: новый хэндл = поле
  EngineHandles + строка в handles() + поле CommonInit + литерал + App-поле +
  App-литерал = всё равно много. ЗАМЕНА CommonInit 17 полей → `pub handles:
  EngineHandles` (одно поле) + App-литерал `drum_step_atom:
  c.handles.drum_step_atom` — тогда fan-out: EngineHandles-поле + handles() +
  CommonInit-поле(1) + App-литерал-строка = 4 места, но деклараций меньше.
  ФИНАЛЬНОЕ РЕШЕНИЕ: CommonInit.handles единое поле; App-поля остаются
  индивидуальными (литерал из c.handles.*). settings.rs:269-295 → `let h =
  engine.handles(); self.drum_step_atom = h.drum_step_atom; ...` (17 присвоений
  остаются — они App-поля, но ПУЛЛЫ одним вызовом).
  ЗАДАЧА ИСПОЛНИТЕЛЯ: убрать 17 полей из CommonInit → одно; поправить оба
  литерала; поправить ДРУГИЕ использования полей CommonInit (grep `c\.` и
  `common.` по lib.rs — audio-конфиг поля остаются).

- [x] **Step 4: гейт**

Run: `cargo check --lib --no-default-features 2>&1 | tail -1 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok"`
Expected: всё зелёное.

- [x] **Step 5: коммит**

```bash
git add src/lib.rs src/synth/mod.rs src/gui/settings.rs && git commit -m "refactor: EngineHandles bundle — 17 handle pulls via SynthEngine::handles() (ST1)"
```

### Task 2: ST2a — PartFullState + SetPartFull handler (TDD)

**Files:**
- Modify: `src/synth/mod.rs` — struct PartFullState (events-секция, рядом с
  LoadPatchEvent :181-190), ControlEvent variant, handler (handle_control, рядом
  с LoadPatch arm)
- Test: `src/synth/tests.rs`

- [x] **Step 1: типы** — events-секция:

```rust
/// Полное состояние парта одной транзакцией (ST2): 1 слот кольца вместо
/// 20-22 событий. `patch` = LoadPatchEvent последним (порядок GUI-пуша).
pub struct PartFullState {
    pub enabled: bool,
    pub mute: bool,
    pub volume: f32,
    pub min_note: u8,
    pub max_note: u8,
    pub vel_min: u8,
    pub vel_max: u8,
    pub pan: f32,
    pub transpose: i8,
    pub note_mode: u8,
    pub sustain_enabled: bool,
    pub ignore_pb: bool,
    pub ignore_mw: bool,
    pub sf2_mode: bool,
    pub sf2_program: u8,
    pub macro_vals: [f32; 8],
    pub patch: Option<Box<LoadPatchEvent>>,
}
```

variant в ControlEvent:
```rust
    SetPartFull { part: usize, state: Box<PartFullState> },
```

- [x] **Step 2: падающий тест** — src/synth/tests.rs:

```rust

// ── ST2: SetPartFull parity с последовательностью событий ───────────

fn load_patch_event_for(patch: &crate::preset::Patch, part: usize) -> LoadPatchEvent {
    let mut ev = crate::synth::ControlEvent::load_patch_from(
        part, patch,
    );
    // load_patch_from возвращает ControlEvent — распаковать до LoadPatchEvent
    match ev {
        ControlEvent::LoadPatch(boxed) => *boxed,
        _ => unreachable!(),
    }
}

#[test]
fn tc_set_part_full_parity_with_event_sequence() {
    let patches = crate::preset::load_factory_patches();
    let seq_events = |e: &mut SynthEngine| {
        // канонический порядок push_full_part_state (gui/mod.rs:2444-2480)
        for ev in [
            ControlEvent::SetPartEnabled { part: 0, enabled: true },
            ControlEvent::SetPartMute { part: 0, mute: false },
            ControlEvent::SetPartVolume { part: 0, volume: 0.7 },
            ControlEvent::SetPartRange { part: 0, min_note: 36, max_note: 84 },
            ControlEvent::SetPartVelRange { part: 0, vel_min: 1, vel_max: 120 },
            ControlEvent::SetPartPan { part: 0, pan: 0.5 },
            ControlEvent::SetPartTranspose { part: 0, semitones: -3 },
            ControlEvent::SetPartNoteMode { part: 0, mode: crate::synth::NOTE_MODE_POLY },
            ControlEvent::SetPartSustain { part: 0, on: true },
            ControlEvent::SetPartIgnorePb { part: 0, on: true },
            ControlEvent::SetPartIgnoreMw { part: 0, on: false },
            ControlEvent::SetPartSf2Mode { part: 0, enabled: false },
        ] {
            drop(e.handle_control(ev));
        }
        for i in 0..8 {
            drop(e.handle_control(ControlEvent::SetMacro { part: 0, index: i, value: 0.1 * i as f32 }));
        }
        drop(e.handle_control(ControlEvent::load_patch_from(0, &patches[0])));
    };
    let tx_events = |e: &mut SynthEngine| {
        let state = crate::synth::PartFullState {
            enabled: true, mute: false, volume: 0.7,
            min_note: 36, max_note: 84, vel_min: 1, vel_max: 120,
            pan: 0.5, transpose: -3,
            note_mode: crate::synth::NOTE_MODE_POLY,
            sustain_enabled: true, ignore_pb: true, ignore_mw: false,
            sf2_mode: false, sf2_program: 0,
            macro_vals: [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7],
            patch: Some(Box::new(load_patch_event_for(&patches[0], 0))),
        };
        drop(e.handle_control(ControlEvent::SetPartFull { part: 0, state: Box::new(state) }));
    };
    let mut a = SynthEngine::new(48_000.0);
    a.set_patches(patches.clone());
    seq_events(&mut a);
    let mut b = SynthEngine::new(48_000.0);
    b.set_patches(patches.clone());
    tx_events(&mut b);
    let l = &a.parts[0];
    let r = &b.parts[0];
    assert_eq!(l.enabled, r.enabled);
    assert_eq!(l.volume, r.volume);
    assert_eq!(l.min_note, r.min_note);
    assert_eq!(l.max_note, r.max_note);
    assert_eq!(l.vel_min, r.vel_min);
    assert_eq!(l.vel_max, r.vel_max);
    assert_eq!(l.pan, r.pan);
    assert_eq!(l.transpose, r.transpose);
    assert_eq!(l.note_mode, r.note_mode);
    assert_eq!(l.sustain_enabled, r.sustain_enabled);
    assert_eq!(l.ignore_pb, r.ignore_pb);
    assert_eq!(l.ignore_mw, r.ignore_mw);
    assert_eq!(l.sf2_mode, r.sf2_mode);
    assert_eq!(l.macro_vals, r.macro_vals);
    assert_eq!(l.params.filter_cutoff, r.params.filter_cutoff, "patch applied");
    assert_eq!(l.params.osc_type, r.params.osc_type);
}

#[test]
fn tc_set_part_full_parity_muted_part_drains_both_ways() {
    // ВАЖНО (ревью 2026-11-08): SetPartMute НЕ имеет change-detection —
    // mute=true на уже замьюченном парте ДРЕЙНИТ голоса И в последовательной,
    // И в транзакционной ветке. Паритет проверяем: обе ветки дают один
    // результат (голоса сняты), никаких no-drain пинов.
    let patches = crate::preset::load_factory_patches();
    let mk = |tx: bool| -> SynthEngine {
        let mut e = SynthEngine::new(48_000.0);
        e.set_patches(patches.clone());
        drop(e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true }));
        drop(e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true }));
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
        let state = crate::synth::PartFullState {
            enabled: true, mute: true, volume: 0.7,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0,
            note_mode: crate::synth::NOTE_MODE_POLY,
            sustain_enabled: false, ignore_pb: false, ignore_mw: false,
            sf2_mode: false, sf2_program: 0,
            macro_vals: [0.0; 8],
            patch: None,
        };
        if tx {
            drop(e.handle_control(ControlEvent::SetPartFull { part: 0, state: Box::new(state) }));
        } else {
            // последовательность как в GUI push_full_part_state (mute=true)
            drop(e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true }));
            drop(e.handle_control(ControlEvent::SetPartMute { part: 0, mute: true }));
            drop(e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 0.7 }));
        }
        e
    };
    let a = mk(false);
    let b = mk(true);
    let drained_a = !a.parts[0].voices.iter().any(|v| v.active);
    let drained_b = !b.parts[0].voices.iter().any(|v| v.active);
    assert_eq!(drained_a, drained_b, "mute drain parity");
    assert!(drained_b, "SetPartMute(true) drains regardless of prior state");
}

#[test]
fn tc_set_part_full_sf2_parity() {
    // TC2 (ревью: был в списке, не было теста): sf2_mode=true + program
    let patches = crate::preset::load_factory_patches();
    let build = |tx: bool| -> SynthEngine {
        let mut e = SynthEngine::new(48_000.0);
        e.set_patches(patches.clone());
        if tx {
            let state = crate::synth::PartFullState {
                enabled: true, mute: false, volume: 0.8,
                min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
                pan: 0.0, transpose: 0,
                note_mode: crate::synth::NOTE_MODE_POLY,
                sustain_enabled: false, ignore_pb: false, ignore_mw: false,
                sf2_mode: true, sf2_program: 5,
                macro_vals: [0.0; 8],
                patch: None,
            };
            drop(e.handle_control(ControlEvent::SetPartFull { part: 0, state: Box::new(state) }));
        } else {
            drop(e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true }));
            drop(e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true }));
            drop(e.handle_control(ControlEvent::SetPartSf2Program { part: 0, program: 5, bank: 0 }));
        }
        e
    };
    let a = build(false);
    let b = build(true);
    assert_eq!(a.parts[0].sf2_mode, b.parts[0].sf2_mode);
    assert!(b.parts[0].sf2_mode, "transaction enables sf2");
}

#[test]
fn tc_set_part_full_macros_without_patch() {
    // Ревью: с patch:Some LoadPatch перезаписывает macro_vals из пресета —
    // SetMacro-ветка не наблюдаема (таутология). Здесь patch:None — сценарий
    // реконнекта/спавна: SetMacro единственный писатель.
    let patches = crate::preset::load_factory_patches();
    let seq = |e: &mut SynthEngine| {
        drop(e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true }));
        for i in 0..8 {
            drop(e.handle_control(ControlEvent::SetMacro { part: 0, index: i, value: 0.1 * i as f32 }));
        }
    };
    let tx = |e: &mut SynthEngine| {
        let state = crate::synth::PartFullState {
            enabled: true, mute: false, volume: 0.8,
            min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
            pan: 0.0, transpose: 0,
            note_mode: crate::synth::NOTE_MODE_POLY,
            sustain_enabled: false, ignore_pb: false, ignore_mw: false,
            sf2_mode: false, sf2_program: 0,
            macro_vals: [0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7],
            patch: None,
        };
        drop(e.handle_control(ControlEvent::SetPartFull { part: 0, state: Box::new(state) }));
    };
    let mut a = SynthEngine::new(48_000.0);
    a.set_patches(patches.clone());
    seq(&mut a);
    let mut b = SynthEngine::new(48_000.0);
    b.set_patches(patches.clone());
    tx(&mut b);
    assert_eq!(a.parts[0].macro_vals, b.parts[0].macro_vals);
    assert_eq!(b.parts[0].macro_vals[3], 0.3, "SetMacro branch is observable");
}
#[test]
fn tc_set_part_full_heap_return() {
    // patch:Some на парте с активным wavetable → Some(LoadPatch) назад
    // (trash-контракт; Box НЕ дропается на аудио-потоке)
    let patches = crate::preset::load_factory_patches();
    let mut e = SynthEngine::new(48_000.0);
    e.set_patches(patches.clone());
    drop(e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true }));
    drop(e.handle_control(ControlEvent::load_patch_from(0, &patches[0])));
    let state = crate::synth::PartFullState {
        enabled: true, mute: false, volume: 0.8,
        min_note: 0, max_note: 127, vel_min: 1, vel_max: 127,
        pan: 0.0, transpose: 0,
        note_mode: crate::synth::NOTE_MODE_POLY,
        sustain_enabled: false, ignore_pb: false, ignore_mw: false,
        sf2_mode: false, sf2_program: 0,
        macro_vals: [0.0; 8],
        patch: Some(Box::new(load_patch_event_for(&patches[1], 0))),
    };
    let spent = e.handle_control(ControlEvent::SetPartFull { part: 0, state: Box::new(state) });
    assert!(spent.is_some(), "patch-carrying transaction must return heap event");
    assert!(matches!(spent, Some(ControlEvent::LoadPatch(_))));
}
```

ИСПОЛНИТЕЛЮ: проверь `ControlEvent::load_patch_from(part, &Patch)` сигнатуру
(synth/mod.rs рядом с load_patch_from_edited :395 — есть вариант от Patch);
NOTE_MODE_POLY путь (crate::synth::NOTE_MODE_POLY или super::NOTE_MODE_POLY);
`e.parts[0]` приватное — tests.rs в том же модуле, ок; macro parity: в
sequence-ветке SetMacro кладёт value и в l.macro_vals И l.params.macro_vals.

**Step 3: красный** — Run: `cargo test --lib tc_set_part_full 2>&1 | tail -4`
Expected: E0560/E0609 (нет PartFullState/variant).

**Step 4: handler + Trash-вариант.** RT-КОНТРАКТ (аудио-поток): ни alloc, ни
free в arm'е. Box<PartFullState> и spent LoadPatch-box не дропаются здесь —
уходят в trash (канон: Sf2 mod.rs:2152, MidiSeq mod.rs:3085-3091; spent
LoadPatch уже ездит как Trash::Control — audio.rs:193-194).

(a) src/trash.rs — добавить вариант:
```rust
    /// An emptied full-part transaction box (ST2). Read on the audio thread,
    /// freed on the GUI thread.
    PartFull(Box<crate::synth::PartFullState>),
```

(b) handler — в handle_control, рядом с LoadPatch arm:

```rust
            ControlEvent::SetPartFull { part, state } => {
                // ST2: транзакция = 1 слот кольца. Композиция СУЩЕСТВУЮЩИХ
                // хэндлеров в каноническом порядке push_full_part_state —
                // семантический паритет по конструкции (change-detection
                // sf2_mode живёт в SetPartSf2Mode arm'е; SetPartMute НЕ имеет
                // change-detection — паритет: drain'ит в обеих формах).
                // RT: state-box и spent LoadPatch-box НЕ дропать здесь — trash.
                let macro_vals = state.macro_vals;
                let sf2_mode = state.sf2_mode;
                let sf2_program = state.sf2_program;
                let sub: [ControlEvent; 12] = [
                    ControlEvent::SetPartEnabled { part, enabled: state.enabled },
                    ControlEvent::SetPartMute { part, mute: state.mute },
                    ControlEvent::SetPartVolume { part, volume: state.volume },
                    ControlEvent::SetPartRange { part, min_note: state.min_note, max_note: state.max_note },
                    ControlEvent::SetPartVelRange { part, vel_min: state.vel_min, vel_max: state.vel_max },
                    ControlEvent::SetPartPan { part, pan: state.pan },
                    ControlEvent::SetPartTranspose { part, semitones: state.transpose },
                    ControlEvent::SetPartNoteMode { part, mode: state.note_mode },
                    ControlEvent::SetPartSustain { part, on: state.sustain_enabled },
                    ControlEvent::SetPartIgnorePb { part, on: state.ignore_pb },
                    ControlEvent::SetPartIgnoreMw { part, on: state.ignore_mw },
                    ControlEvent::SetPartSf2Mode { part, enabled: sf2_mode },
                ];
                let mut spent: Option<ControlEvent> = None;
                for ev in sub {
                    if let Some(heap) = self.handle_control(ev) {
                        // среди 12 plain-data arm'ов heap не бывает — guard
                        spent = Some(heap);
                        break;
                    }
                }
                if spent.is_none() {
                    for (index, &value) in macro_vals.iter().enumerate() {
                        if let Some(heap) = self.handle_control(
                            ControlEvent::SetMacro { part, index, value },
                        ) {
                            spent = Some(heap);
                            break;
                        }
                    }
                }
                if spent.is_none() && sf2_mode {
                    if let Some(heap) = self.handle_control(
                        ControlEvent::SetPartSf2Program {
                            part, program: sf2_program, bank: 0,
                        },
                    ) {
                        spent = Some(heap);
                    }
                }
                // patch ПОСЛЕДНИМ (GUI-порядок: full state, затем LoadPatch).
                // Box по указателю: DerefMut part; НЕ распаковывать
                // (`*boxed` = free на аудио-потоке — канон LoadPatch :2512-2514).
                let mut patch_spent: Option<ControlEvent> = None;
                if spent.is_none() {
                    if let Some(boxed) = state.patch.as_mut() {
                        boxed.part = part; // DerefMut, no alloc
                        patch_spent = self.handle_control(
                            ControlEvent::LoadPatch(state.patch.take().unwrap()),
                        );
                    }
                }
                // Trash: spent LoadPatch (heap) + сам state-box (иначе free
                // случился бы на аудио-потоке).
                if let Some(t) = self.trash.as_mut() {
                    if let Some(heap) = spent {
                        t.dispose(crate::trash::Trash::Control(heap));
                    }
                    if let Some(heap) = patch_spent {
                        t.dispose(crate::trash::Trash::Control(heap));
                    }
                    t.dispose(crate::trash::Trash::PartFull(state));
                }
                None
            }
```

Семантические замечания (из ревью, не игнорировать):
- SetPartMute БЕЗ change-detection (synth/mod.rs:2593-2613): mute=true на уже
  замьюченном парте drain'ит голоса. Паритет: и последовательность, и транзакция
  ведут себя одинаково (TC3).
- SetPartSf2Mode С change-detection (:2968-2983): no-op при неизменном режиме.
- Порядок Apply: patch последним = GUI-порядок push_full_part_state→
  send_edited_params. В apply_performance сейчас patch ПЕРВЫМ — проверено
  ревьюером: end-state не расходится (LoadPatch не трогает volume/pan/range/
  sampler-каналы), инверсия допустима; закомментировать в apply_performance
  (Task 3).
- borrow: все поля Copy (macro_vals [f32;8], patch Option — take после sub).

**Step 5: зелёный + полный гейт**

Run: `cargo test --lib tc_set_part_full 2>&1 | tail -3 && cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok" && cargo test --features rt-assert --test rt_safety 2>&1 | tail -1`
ПЛЮС: добавить в tests/rt_safety.rs кейс — SetPartFull с patch:Some внутри
assert_no_alloc-зоны ( handler не аллоцирует и не освобождает: box'ы уходят
в trash; assert_no_alloc ловит И alloc И dealloc — rt_safety.rs:236-238).
Expected: 5 тестов PASS (parity, mute-parity, sf2, macros-no-patch, heap-return), всё зелёное.

**Step 6: коммит**

```bash
git add src/synth/mod.rs src/synth/tests.rs && git commit -m "feat: SetPartFull transaction — one ring slot per part gesture, composes existing handlers (ST2a)"
```

### Task 3: ST2b — GUI переключается на транзакции

**Files:**
- Modify: `src/gui/mod.rs` — push_full_part_state (:2444-2480), do_swap_parts_push (:2524-2532), reset_and_push_part (:2486-2506), apply_performance (~:3300-3403)

- [x] **Step 1: push_full_part_state → транзакция** (тело заменить):

```rust
    fn push_full_part_state(&mut self, part: usize) {
        // ST2: 1 слот кольца вместо 20-21 события. Патч не входит (None) —
        // его несёт send_edited_params отдельным heap-событием, как раньше.
        let p = &self.parts[part];
        let state = crate::synth::PartFullState {
            enabled: p.enabled, mute: p.mute, volume: p.volume,
            min_note: p.min_note, max_note: p.max_note,
            vel_min: p.vel_min, vel_max: p.vel_max,
            pan: p.pan, transpose: p.transpose,
            note_mode: p.note_mode,
            sustain_enabled: p.sustain_enabled,
            ignore_pb: p.ignore_pb, ignore_mw: p.ignore_mw,
            sf2_mode: p.sf2_mode, sf2_program: p.sf2_program,
            macro_vals: p.macro_vals,
            patch: None,
        };
        let _ = self.ctrl_tx.push(ControlEvent::SetPartFull { part, state: Box::new(state) });
    }
```

- [x] **Step 2: do_swap_parts_push** — патч входит в транзакцию (свап = 2
  слота). Заменить:

```rust
    fn do_swap_parts_push(&mut self, a: usize, b: usize) {
        // ST2: свап = 2 транзакции с патчем внутри (было 42-44 события;
        // разрыв кольца больше не даёт half-swap).
        for part in [a, b] {
            let p = &self.parts[part];
            let patch = ControlEvent::load_patch_from_edited(
                part, &p.edited_params,
                self.patches.get(p.patch_idx),
            );
            let patch = match patch {
                ControlEvent::LoadPatch(boxed) => Some(boxed),
                _ => unreachable!(),
            };
            let state = crate::synth::PartFullState {
                enabled: p.enabled, mute: p.mute, volume: p.volume,
                min_note: p.min_note, max_note: p.max_note,
                vel_min: p.vel_min, vel_max: p.vel_max,
                pan: p.pan, transpose: p.transpose,
                note_mode: p.note_mode,
                sustain_enabled: p.sustain_enabled,
                ignore_pb: p.ignore_pb, ignore_mw: p.ignore_mw,
                sf2_mode: p.sf2_mode, sf2_program: p.sf2_program,
                macro_vals: p.macro_vals,
                patch,
            };
            let _ = self.ctrl_tx.push(ControlEvent::SetPartFull { part, state: Box::new(state) });
        }
        self.persist_parts_state();
    }
```

(Порядок «state потом patch» внутри одной транзакции = прежний порядок «full
state, потом LoadPatch» — GUI-порядок сохранён.)

- [x] **Step 3: reset_and_push_part** — tail: `load_edited_params(part);` +
  `send_edited_params(part);` + `push_full_part_state(part);` заменить на
  сборку транзакции с patch (паттерн Step 2, part один) — ИЛИ (проще, тоже
  корректно) оставить send_edited_params и push_full_part_state (теперь 2
  события вместо 21-22). РЕШЕНИЕ: простое (2 события) — семантика та же,
  дифф меньше. Патч-в-транзакцию — только свапу (2 слота).

- [x] **Step 4: apply_performance** — per-part блок (~14-15 событий) заменить
  сборкой PartFullState c patch (load_patch_from_edited с edited_params
  перфа) — 8 транзакций + SetSustain{false} + tail SetPartEnabled. ВАЖНО:
  apply_performance берёт данные из СВОИХ локальных структур (parts из файла
  перфа), НЕ из self.parts — собрать PartFullState из тех же источников.
  РЕШЕНО ревью: apply_performance хардкодит `bank: 0` (gui/mod.rs:3385-3387) —
  поле sf2_bank в PartFullState НЕ нужно. Ещё два факта ревью:
  (1) apply сейчас шлёт LoadPatch ПЕРВЫМ (:3366), транзакция — ПОСЛЕДНИМ:
  end-state не расходится (LoadPatch не трогает volume/pan/range/сэмплер-
  каналы), инверсия допустима — оставить комментарий в коде об этом;
  (2) min_note/max_note для транзакции брать из self.parts[i] (stale до-сплит
  значения — ровно то, что send_part_range пушит сегодня :2648-2652), НЕ из
  key_low/key_high перфа; apply_split_ranges (layers.rs:670-691) остаётся
  хвостом и перепушивает диапазоны после.

- [x] **Step 5: гейт**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -1 && cargo test 2>&1 | grep -cE "test result: ok" && cargo test --features rt-assert --test rt_safety 2>&1 | tail -1 && cargo test --test sf2_channel_gain 2>&1 | tail -2`
Expected: всё зелёное (sf2_channel_gain пинил СТАРЫЙ порядок apply — если
тест шлёт события напрямую, он не ломается; если тест ссылался на GUI-функцию
— проверить и поправить пин на транзакцию, осознанно).

- [x] **Step 6: коммит**

```bash
git add src/gui/mod.rs && git commit -m "feat: GUI gestures via SetPartFull — part swap = 2 ring slots, spawn/reconnect = 1 (ST2b)"
```

### Task 4: Верификация стадии + релизный цикл

- [x] **Step 1: полный гейт** — clippy + cargo test + rt_safety + cargo deny.
- [x] **Step 2:** PR → squash merge → rebuild release → ЗАПУСТИТЬ приложение
  (ночь: запуск разрешён, проверка запускаемости) → логи `adb`/stderr не смотрим
  дольше 5 с, процесс жив = ок.
- [x] **Step 3:** TECHDEBT подсекция + roadmap: чекбокс стадии, «УТРОМ: что
  проверить» — пункты: свап партов работает (драг ⠿), спавн парта, реконнект
  аудио (Settings → Reconnect) сохраняет состояние партов, apply перфа.
- [x] **Step 4:** Tavily ПОСЛЕ итерации: сверить «boxed payload + trash» с
  канонами (reclamation off audio thread) — расхождение → пометка.

## Out of scope

- Arc-снапшот вместо Box — отклонено (исследование: Box+trash канон уже есть).
- Прикладной backpressure/overflow-counter UI — кольцо и так считает потери
  (CountingProducer); сигнал в GUI — по фидбеку.
- App-поля в sub-structs (168 полей) — стадия 4 (S4-gui).
