# SF2 Channel Gain Restore (perf-load vol/pan loss) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-03 (база = master 0622aa4; все сниппеты вычитаны из текущего кода; аудит-репорт юзера «загружаю перф — напутает громкость/панораму, лечится перезагрузкой перфа», 2026-11-03)

**Goal:** Устранить потерю громкости/панорамы/мьюта SF2-парта при выделении ему
канала rustysynth (загрузка перфа со сменой движка DSP↔SF2 и audio-reconnect):
канал получал CC7/CC10 ПРЕДЫДУЩЕГО жильца (или GM-дефолт), слайдеры при этом
показывали правильные значения.

**Architecture:** Фикс в ОДНОЙ точке — обработчике `ControlEvent::SetPartSf2Mode`
(src/synth/mod.rs ~:2925-2943): `set_part_mode` (выделение канала) вызывается
ДО ре-применения CC10+CC7; no-op-гард заменён на `mode_changed`-гейт дрена.
Порядок-независимо чинит все пути: perf-load (push-порядок Volume→Pan→Mute→…
→Sf2Mode), audio-reconnect (push_full_part_state), спавн парта.

**Tech Stack:** Rust; regression-тесты — amplitude-based через существующий
паттерн tests/layer_switch_debug.rs (локальный SC-55.sf2, автопропуск при
отсутствии; RT-safe: только handle_control/tick_block, без аллокаций в
рендер-цикле).

**Проверенные чтением кода факты (не пересматривать):**
- Каналы: `part_channel: [Option<usize>; 16]` (sampler.rs:213), стартуют `None`
  (:249); выделяются ТОЛЬКО в `set_part_mode(part, true)` (:384-400,
  `alloc_channel` — свободный из MELODIC_CHANNELS, 15 каналов, 9 = ударка);
  освобождаются в `set_part_mode(part, false)` (`take()`).
- `set_layer_volume` (sampler.rs:455-460) и `set_channel_pan` (:449-453) молча
  выходят при `part_channel_of(part) == None` — CC в никуда.
- Обработчик SetPartSf2Mode (synth/mod.rs:2925-2943): no-op-гард
  (`l.sf2_mode == enabled` → `return None`), затем drain, `l.sf2_mode = enabled`,
  CC10 (`set_channel_pan`) — ДО `set_part_mode` (:2942), т.е. для свежего
  свитчера канал ещё None и CC10 ТЕРЯЕТСЯ; CC7 (громкость) и мьют-гейт
  (`set_layer_volume`) не посылаются ВООБЩЕ.
- Push-порядок GUI: apply_performance (mod.rs:2856-2872) и push_full_part_state
  (:2325-2349) шлют Volume/Mute/Pan ДО SetPartSf2Mode → на момент их прихода у
  свитчера/свежего движка канала нет → все три теряются; выделенный позже
  канал несёт CC7/CC10 прежнего жильца или GM-дефолт (CC7=100).
- Unmute-обработчик SetPartMute (synth/mod.rs:2554-2557) восстанавливает
  CC7 из `l.volume` — тоже мимо канала на свитчере.
- Тестовый паттерн: tests/layer_switch_debug.rs:40-110 — `render_peak`,
  `load_font()` (SC-55.sf2, skip при отсутствии), `SynthEngine::new(SR)` +
  `set_patches` + `handle_control(LoadKeysSoundFont)` + события; амплитудная
  ассерт-семантика (`SILENCE = 1e-3`).
- NoteOn-роутинг: SF2-парт звучит через sampler (канал обязателен) —
  proves harness (perf_load_both_sf2_layers_speak говорит «part speaks»).
- RT-инварианты: правка только в обработчике ControlEvent (GUI→engine, RT-зона)
  — новые вызовы (`set_channel_pan`, `set_layer_volume`, `set_part_mode`) уже
  используются в RT-пути сегодня; аллокаций/локов не добавляется.

---

## Test Cases (from the requirement)

Требование: после загрузки перфа / reconnect звук SF2-парта соответствует
слайдерам громкости/панорамы и мьюту. GUI-зеркало уже корректно (юзер-факт:
«слайдеры правильные, звук нет»).

- TC1 (volume-lost-on-switch): part: Vol 0.0 → Pan → Mute false → Range →
  Sf2Mode(true) → Sf2Program → NoteOn → пик ≈ 0 (тишина). ДО фикса — слышно
  (канал с дефолтным CC7).
- TC2 (mute-held-across-switch): Vol 1.0 → Mute(true) → … → Sf2Mode(true) →
  NoteOn → тишина. Зелёный и ДО фикса: замьюченные парты скипаются на
  роутинге (synth/mod.rs:2259) — тест пинит роутинг-скип; мьют-гейт CC7 в
  фиксе — корректность состояния канала, отдельного красного теста не даёт.
- TC3 (pan-on-switch): Vol 1.0 → Pan(-1.0) → … → Sf2Mode(true) → NoteOn →
  peak_left ≥ 4×peak_right. ДО фикса — FAIL (CC10 уходит до аллокации канала).
- TC4 (volume-value-on-switch, positive): два одинаковых SF2-свитчера, Vol 1.0
  и Vol 0.1 → peak(0.1) < peak(1.0)×0.5 — CC7 применён СО ЗНАЧЕНИЕМ, не только
  mute-гейт. ДО фикса — FAIL.
- TC5 (no-op idempotent + baseline): Sf2Mode(true) дважды (второй — no-op) +
  Vol поверх; парт говорит с установленной громкостью, без паники. Проходит
  и до, и после (санити-бейзлайн порядка Volume-ПОСЛЕ-mode).
- TC6 (tails on no-op): SF2-парт ДЕРЖИТ ноту → no-op Sf2Mode → сустейн
  продолжается (уровень A в 4+ раза выше контроля B с РЕАЛЬНЫМ флипом в DSP,
  который дренит+освобождает канал). Пин семантики «no-op не дренит»
  (synth/mod.rs:2926-2927).

## Negative Matrix

- SF2-файл отсутствует на машине — тесты автопропускаются (паттерн
  `load_font()` layer_switch_debug.rs; все 6 TC гейтятся одинаково).
- Каналы исчерпаны (>15 SF2-партов) — `alloc_channel` → None, CC по-прежнему
  early-return: парт молчит. Существующее ограничение, вне скоупа.
- false→false no-op с существующим каналом — невозможно по инварианту
  (переход в DSP освобождает канал `take()`, sampler.rs:388-393).
- Drain на mode_changed режет хвосты — СОХРАНЁННАЯ семантика (TC6 пинит
  обратное: no-op не дренит); свитчер дренился и до фикса.
- RT-safety: новые вызовы уже в RT-пути, аллокаций нет; `cargo test
  --features rt-assert --test rt_safety` в гейте.
- `enabled=false` ветка: set_part_mode освобождает канал; CC не шлём
  (гейт `if enabled`) — некому.
- pan вне [-1;1] из битого JSON — существующий `.clamp(-1.0, 1.0)` в обработчике.
- Повторный Sf2Program после фикса — порядок не меняется (Program шлётся
  GUI-стороной ПОСЛЕ Sf2Mode и попадает в уже существующий канал — как сегодня
  для не-свитчеров).

---

### Task 1: Regression-тесты (TDD — красные до фикса)

**Files:**
- Create: `tests/sf2_channel_gain.rs`

- [ ] **Step 1: написать тесты** (паттерн — layer_switch_debug.rs:40-110):

```rust
//! #49-followup / SF2 channel gain regression (2026-11-03 user report:
//! "загружаю перф — напутает громкость и панораму, лечится перезагрузкой").
//! Root cause: SetPartSf2Mode allocated the rustysynth channel AFTER the
//! Volume/Mute/Pan events (which early-return with no channel), so the
//! channel kept the previous occupant's CC7/CC10 (or the GM default).
//! Tests use the GUI push order verbatim (apply_performance, mod.rs:2822).
//! Requires the local soundfont; skipped automatically if it is absent.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;
const SF2_PATH: &str = "/home/dima/.local/share/mini_midi_synth/sf2/SC-55.sf2";

fn load_font() -> Option<std::sync::Arc<rustysynth::SoundFont>> {
    let mut file = std::fs::File::open(SF2_PATH).ok()?;
    rustysynth::SoundFont::new(&mut file).ok().map(std::sync::Arc::new)
}

fn engine_with_sf2(sf: std::sync::Arc<rustysynth::SoundFont>) -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    e.set_patches(preset::load_all_patches());
    let _ = e.handle_control(ControlEvent::LoadKeysSoundFont { soundfont: sf });
    e
}

/// GUI push order for ONE part from apply_performance (mod.rs:2856-2872),
/// reduced to the volume/pan/mute/mode essentials. `lo/hi` — диапазон парта:
/// роутинг звучит во ВСЕ включённые немьютнутые парты с подходящим range
/// (synth/mod.rs:2258-2271), поэтому тесты с двумя партами обязаны дать
/// непересекающиеся диапазоны.
#[allow(clippy::too_many_arguments)]
fn gui_switch_part_to_sf2(e: &mut SynthEngine, part: usize, vol: f32, pan: f32, mute: bool, lo: u8, hi: u8) {
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part, volume: vol });
    let _ = e.handle_control(ControlEvent::SetPartRange { part, min_note: lo, max_note: hi });
    let _ = e.handle_control(ControlEvent::SetPartPan { part, pan });
    let _ = e.handle_control(ControlEvent::SetPartMute { part, mute });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartSf2Program { part, program: 0, bank: 0 });
}

fn peaks(e: &mut SynthEngine, note: u8, blocks: usize) -> (f32, f32) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let (mut pl, mut pr) = (0.0f32, 0.0f32);
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            pl = pl.max(l[i].abs());
            pr = pr.max(r[i].abs());
        }
    }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note });
    (pl, pr.max(1e-9))
}

fn overall_peak(e: &mut SynthEngine, note: u8, blocks: usize) -> f32 {
    let (pl, pr) = peaks(e, note, blocks);
    pl.max(pr)
}

#[test]
fn tc1_zero_volume_survives_engine_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 0.0, 0.0, false, 0, 127);
    let p = overall_peak(&mut e, 60, 200);
    assert!(p < SILENCE, "vol 0.0 must stay silent after DSP->SF2 switch (peak {p})");
}

#[test]
fn tc2_mute_survives_engine_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, true, 0, 127);
    let p = overall_peak(&mut e, 60, 200);
    assert!(p < SILENCE, "muted part must stay silent after switch (peak {p})");
}

#[test]
fn tc3_pan_survives_engine_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, -1.0, false, 0, 127);
    let (pl, pr) = peaks(&mut e, 60, 200);
    assert!(pl > pr * 4.0, "pan -1.0 lost on switch: left {pl}, right {pr}");
}

#[test]
fn tc4_volume_value_applied_on_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    // Непересекающиеся диапазоны: иначе NoteOn звучит в ОБОИХ партах и
    // quiet ≈ loud независимо от CC7. Дренаж хвоста перед вторым замером.
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, false, 0, 59);
    let loud = overall_peak(&mut e, 40, 200);
    for _ in 0..100 { let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE]; e.tick_block(&mut l, &mut r); }
    gui_switch_part_to_sf2(&mut e, 1, 0.1, 0.0, false, 60, 127);
    let quiet = overall_peak(&mut e, 72, 200);
    assert!(quiet < loud * 0.5, "vol 0.1 ({quiet}) not quieter than 1.0 ({loud})");
}

#[test]
fn tc5_noop_switch_keeps_volume_and_no_panic() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, false, 0, 127);
    // no-op switch (SF2 -> SF2), затем volume поверх — как perf-load пушит
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 0.5 });
    let p = overall_peak(&mut e, 60, 200);
    assert!(p > SILENCE, "no-op switch must keep the part audible (peak {p})");
}

#[test]
fn tc6_noop_switch_does_not_cut_tail() {
    // A: держим ноту (Drawbar Organ — бесконечный сустейн, умеренный релиз),
    // делаем no-op Sf2Mode — сустейн продолжается.
    // B (контроль): РЕАЛЬНЫЙ флип в DSP — drain(release)+free — релиз затухает.
    // ОКНО 100 блоков (~69ms): в 10-блочном релиз не успевает отлиочь от
    // сустейна (урок исполнения 2026-11-03: red pre-fix при pa≈pb).
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut a = engine_with_sf2(sf.clone());
    let mut b = engine_with_sf2(sf);
    for e in [&mut a, &mut b] {
        gui_switch_part_to_sf2(e, 0, 1.0, 0.0, false, 0, 127);
        let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: 0, program: 16, bank: 0 });
        e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    }
    let _ = a.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true }); // no-op
    let _ = b.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: false }); // drain(release)+free
    let mut la = [0.0f32; BLOCK_SIZE]; let mut ra = [0.0f32; BLOCK_SIZE];
    let mut lb = [0.0f32; BLOCK_SIZE]; let mut rb = [0.0f32; BLOCK_SIZE];
    let (mut pa, mut pb) = (0.0f32, 0.0f32);
    for _ in 0..100 {
        a.tick_block(&mut la, &mut ra);
        b.tick_block(&mut lb, &mut rb);
        for i in 0..BLOCK_SIZE {
            pa = pa.max(la[i].abs());
            pb = pb.max(lb[i].abs());
        }
    }
    a.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    b.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(pa > pb * 4.0, "no-op switch behaved like a real flip: a {pa}, b {pb}");
}
```

- [ ] **Step 2: прогнать — зафиксировать красные**

Run: `cargo test --test sf2_channel_gain 2>&1 | tail -12`
Expected: tc1/tc3/tc4 FAIL (канал с дефолтным CC7=100 и центром); tc2/tc5/tc6
PASS — tc2 зелёный ДО фикса легитимно: замьюченные парты скипаются на
роутинге (synth/mod.rs:2259), мьют-асимметрия на слух не проявляется
(мьют-гейт в фиксе нужен для корректности состояния канала, не для этого
теста). Пики в выводе — диагностика. Компиляция чистая.

- [ ] **Step 3: коммит красных тестов**

```bash
git add tests/sf2_channel_gain.rs && git commit -m "test: SF2 channel gain regressions — vol/pan/mute lost on DSP->SF2 switch and reconnect (red)"
```

### Task 2: Фикс обработчика SetPartSf2Mode

**Files:**
- Modify: `src/synth/mod.rs` (~:2925-2943)

- [ ] **Step 0: амение tc6 (урок исполнения, red-коммит 4489e41)** — в
tests/sf2_channel_gain.rs заменить тело tc6 на версию из секции Test Cases /
Task 1 Step 1 ЭТОЙ версии плана (Drawbar Organ program 16, окно 100 блоков;
в 10-блочном окне релиз неотличим от сустейна — pa≈pb). Прогнать
`cargo test --test sf2_channel_gain tc6` — tc6 теперь ЗЕЛЁНЫЙ на красном
коммите. Коммит: `git add tests/sf2_channel_gain.rs && git commit -m "test: tc6 — organ sustain, 100-block window (10-block window can't tell release from sustain)"`.

- [ ] **Step 1: переписать обработчик** — заменить ПЛЕЧО `SetPartSf2Mode`
в `handle_control` (src/synth/mod.rs ~:2925-2943) целиком на:

```rust
            ControlEvent::SetPartSf2Mode { part, enabled } => {
                // No-op switch (perf load pushes current state for every part):
                // draining here would cut SF2 tails on every perf change.
                // Drain only on a REAL mode flip (pinned by tc6).
                let mode_changed = self.parts.get(part)
                    .map(|l| l.sf2_mode != enabled)
                    .unwrap_or(false);
                if mode_changed {
                    if let Some(l) = self.parts.get_mut(part) {
                        // Engine switch: both voice owners must be drained.
                        // ОБЯЗАТЕЛЬНО: единственный writer sf2_mode — иначе
                        // mode_changed залипает в true и роутинг ломается.
                        l.drain_notes(&mut self.sampler, part, false);
                        l.sf2_mode = enabled;
                    }
                }
                // Allocate/free the channel FIRST: volume/pan/mute events of
                // the perf-load and reconnect push orders arrive BEFORE this
                // switch and early-return while the channel is still absent.
                self.sampler.set_part_mode(part, enabled);
                // (Re)apply per-channel gains: a freshly allocated channel
                // keeps the previous occupant's CC7/CC10 or GM defaults —
                // the "перф напутал громкость/панораму, лечится
                // перезагрузкой" report (2026-11-03).
                if enabled {
                    if let Some(l) = self.parts.get(part) {
                        let cc = (l.pan.clamp(-1.0, 1.0) * 0.5 + 0.5) * 127.0;
                        self.sampler.set_channel_pan(part, cc as i32);
                        self.sampler
                            .set_layer_volume(part, if l.mute { 0.0 } else { l.volume });
                    }
                }
            }
```

ПЛЕЧО БЕЗ `None`: match в `handle_control` statement-typed — все плеча
заканчиваются `;`, функция возвращает `None` в хвосте (synth/mod.rs:3097-3101).
Сниппет выше заканчивается блоком `}` без выражения — как соседние плечи.

- [ ] **Step 2: прогнать — все зелёные**

Run: `cargo test --test sf2_channel_gain 2>&1 | tail -10 && cargo test --test layer_switch_debug 2>&1 | tail -4`
Expected: 6/6 sf2_channel_gain ok (tc2 зелёный был и до — см. Step 2 Task 1); layer_switch_debug зелёные
(семантика хвостов не тронута).

- [ ] **Step 3: rt_safety + clippy**

Run: `cargo clippy --all-targets -- -D warnings 2>&1 | tail -2 && cargo test --features rt-assert --test rt_safety 2>&1 | tail -2`
Expected: чисто.

- [ ] **Step 4: коммит**

```bash
git add src/synth/mod.rs && git commit -m "fix: SF2 channel gains re-applied after allocation — perf-load/reconnect no longer inherit previous occupant's CC7/CC10 (vol/pan/mute loss)"
```

### Task 3: Верификация и сборка

- [ ] Полный гейт: `cargo clippy --all-targets -- -D warnings && cargo test && cargo test --features rt-assert --test rt_safety`
- [ ] PR → squash merge → пересборка release в main checkout → перезапуск
- [ ] Экран (юзер): загрузить перф со сменой движков — громкость/панорама
  соответствуют слайдерам сразу; muted SF2-парт молчит; повторная загрузка
  больше не «чинит» (чинить нечего)
- [ ] TECHDEBT: новый раздел «SF2 channel gains loss (vol/pan/mute)» — ГОТОВО
  со ссылкой на PR

## Out of scope

- Канальный голод (>15 SF2-партов) — существующее ограничение alloc_channel.
- DSP-путь громкости/панорамы (l.volume/l.pan в tick_block) — не задействован
  в баге, не трогаем.
- Пересмотр push-порядка GUI (Volume до Sf2Mode) — фикс order-independent,
  порядок менять не нужно.
