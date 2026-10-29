# Seamless Patch Switch Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ 90ecfab чтением кода; аудит семантики — диалог 2026-10-29, факты 1–7)
status: executed 2026-10-29 (worktree-seamless-switch; ревью плана 3 раунда, ревью реализации SIGN OFF WITH NITS)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Смена перфа/патча становится seamless: хвосты доигрывают старым звуком,
зажатые клавиши продолжают сустейн без новой атаки, новые ноты звучат новым
патчем. Вместо политики «убить всё при смене патча» — политика «хвосты живы до
естественной смерти».

**Architecture:** Килл-блоки (release всех голосов + очистка bookkeeping)
удаляются из двух arm: `LoadPatch` (mod.rs ~2067-2082) и `MidiEvent::ProgramChange`
(mod.rs ~1903-1919). Освобождение по-прежнему гарантирует `set_sustain` pedal-up
(mod.rs:1484-1518, выпускает по src_note независимо от патча) + `Part::note_off`
без range-guard (mod.rs:835-897) + panic-пути (не трогаются). Килл-блоки в
SetPartVolume/SetPartTranspose/прочих arms НЕ ТРОГАЮТСЯ — это фиксы
зависающего сустейна из прошлых сессий. Вор голосов: приоритет «release-фазные
голоса первыми» (JUCE-практика), без quick-fade движка (задокументировано).
Latch-mismatch: смена патча с latch-режима на не-latch отпускает только
latch-голоса.

**Tech Stack:** Rust, cargo test, tests/sustain_audit.rs harness.

**ОДОБРЕНО ПОЛЬЗОВАТЕЛЕМ (2026-10-29, обсуждение seamless):** сама фича,
убийство в обоих arms, latch-mismatch release, ПЕРЕПИСЫВАНИЕ двух тестов
(`patch_switch_releases_pedal_held_notes`, `program_change_releases_pedal_held_notes`)
под новую семантику — это осознанная отмена старого контракта e91a37b
(«после смены патча ни одна нота не звучит»).

**Проверки перед каждым коммитом:**

```bash
cargo clippy --all-targets -- -D warnings
cargo test --features rt-assert --test rt_safety
```

---

## Test Cases (from the requirement)

Требование: «смена перфа: хвосты доигрывают старым звуком, зажатая клавиша
продолжает звучать без новой атаки, новые ноты — новый звук; педаль-ап после
смены освобождает всё; паника по-прежнему убивает мгновенно».

1. `patch_switch_tail_keeps_sounding` — pedal down → NoteOn → LoadPatch (другой
   патч) → сразу render_peak > SILENCE (хвост жив) → pedal up → settle → тишина.
2. `held_key_keeps_sounding_across_patch_switch` — NoteOn (без педали) →
   LoadPatch → сразу peak > SILENCE (голос не убит) → NoteOff → settle → тишина.
3. `new_notes_use_new_patch` — NoteOn патча A → NoteOff+settle → LoadPatch
   патча B (другой transpose/detune — детект по пику: у B другой характер) —
   минимум: звук ЕСТЬ и это не падение в тишину; сильная форма: сравнить пик
   ноты на A vs B — они различаются >10% (если init-пресеты B недоступны в
   тесте, слабая форма достаточна, задокументировать).
4. `program_change_seamless_tail` — pedal down → NoteOn → ProgramChange →
   сразу peak > SILENCE → pedal up → settle → тишина.
5. `panic_still_kills_after_seamless_switch` — NoteOn → LoadPatch → CC120 →
   сразу peak < SILENCE (без settle).
6. `latch_mismatch_releases_latch_voices` — LoadPatch с play_mode=3 → аккорд
   NoteOn → NoteOff (latch держит) → LoadPatch с play_mode=0 → settle → тишина.
7. `steal_prefers_releasing_voices` (unit, src/synth/tests.rs) — 16 голосов
   заняты, один в release-фазе → новый NoteOn занимает ИМЕННО release-голос
   (голоса с sustained-фазой не тронуты).

## Negative Matrix

- Смена патча на ПУСТОМ движке (нот нет) — ничего не происходит, нет паники
  (присутствует в тестах 1/4 как побочный путь первого LoadPatch).
- Двойная смена подряд (A→B→C) — хвосты A+B звенят одновременно, C звучит
  (перекрытие хвостов — ЗАДОКУМЕНТИРОВАНО как осознанное поведение).
- Педаль зажата при смене И никогда не отпускается — старые голоса звенят
  бесконечно (осознанный висяк по вине педали юзера; pedal-up лечит).
- ProgramChange с program вне диапазона — раньше килл срабатывал «даже если
  cache lookup бейлится», теперь НЕ происходит НИЧЕГО (ни смены, ни релиза) —
  осознанное изменение (тест 4 с валидным program).
- Latch→latch — latch-голоса НЕ отпускаются (играют дальше), только
  latch→не-latch releases (тест 6 + обратный кейс unit'ом не покрывается —
  code-review).
- Root-only часть — root-маски сохраняются при смене патча, re-key работает
  (байт-идентично аудиту факта 6; отдельного теста нет — существующие
  root_only тесты должны остаться зелёными).
- clock skew/timeout — неприменимо (нет внешних часов/IO).

---

### Task 1: Тесты — переписать 2 старых + 4 новых (все красные/часть зелёные)

**Files:**
- Modify: `tests/sustain_audit.rs` (~161 `patch_switch_releases_pedal_held_notes`,
  ~174 `program_change_releases_pedal_held_notes` — прочитать оба теста ЦЕЛИКОМ
  перед заменой: там паттерн переключения патча через LoadPatch — он понадобится
  в новых тестах)

- [x] **Step 1: прочитать оба старых теста целиком** (sustain_audit.rs:161-184) —
  все их механизмы уже вшиты в код ниже: переключение патча =
  `let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));`;
  педаль = `pedal_down(&mut e)` / `pedal_up(&mut e)`; ProgramChange =
  `e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 1 })`.

- [x] **Step 2: заменить оба старых теста и добавить новые** (код ниже; адаптируй
  имена хелперов/патчей под реальные из Step 1 — НЕ менять харнесс):

```rust
#[test]
fn patch_switch_seamless_tail_and_pedal_up_releases() {
    let mut e = engine_with_init_patch();
    // pedal down, play a note, release key (pedal holds it)
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    assert!(render_peak(&mut e, 4) > SILENCE);
    // switch patch (seamless): pedal-held voice KEEPS SUSTAINING.
    // Дискриминатор против kill-семантики: kill = voice в release → после
    // settle (8s) тишина; seamless = sustain продолжается → громко.
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "patch switch killed a pedal-held voice (it should keep sustaining)");
    // pedal up: everything releases
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn held_key_keeps_sounding_across_patch_switch() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    // key STILL HELD: kill-семантика выпустила бы голос в release → после
    // settle тишина; seamless → голос продолжает сустейниться
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "held key was killed by patch switch");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn program_change_seamless_tail() {
    let mut e = engine_with_init_patch();
    pedal_down(&mut e);
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    e.handle_event(MidiEvent::ProgramChange { channel: 0, program: 1 });
    settle(&mut e);
    assert!(render_peak(&mut e, 20) > SILENCE,
        "program change killed a pedal-held voice");
    pedal_up(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn new_notes_sound_after_patch_switch() {
    let mut e = engine_with_init_patch();
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    assert!(render_peak(&mut e, 20) > SILENCE, "new patch produced no sound");
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    settle(&mut e);
    assert!(render_peak(&mut e, 4) < SILENCE);
}

#[test]
fn panic_still_kills_after_seamless_switch() {
    let mut e = engine_with_init_patch();
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    e.handle_event(MidiEvent::ControlChange { channel: 0, cc: 120, value: 0 });
    assert!(render_peak(&mut e, 4) < SILENCE, "CC120 must still hard-kill");
}
```

- [x] **Step 3: latch-mismatch тест** (в `tests/sustain_audit.rs`; конструирование
  PatchParams с play_mode=3 — взять init params и поменять поле; как хелпер
  грузит патчи — из Step 1):

```rust
#[test]
fn latch_mismatch_releases_latch_voices() {
    let mut e = engine_with_init_patch();
    // LoadPatch с play_mode=3 (latch): берём второй фабричный патч и правим
    // параметр (ключ "play_mode" — см. src/synth/tests.rs:750 прецедент)
    let mut latch_patch = second_patch();
    latch_patch.params.insert("play_mode".into(), 3.0);
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &latch_patch));
    // аккорд: две ноты, отпускание — latch держит
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 127 });
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 64, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 60 });
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 64 });
    assert!(render_peak(&mut e, 4) > SILENCE, "latch must hold the chord");
    // switch to non-latch patch: latch voices released
    let _ = e.handle_control(ControlEvent::load_patch_from(0, &second_patch()));
    // release tail length depends on the new patch's release — assert DECAY,
    // not absolute silence (robust against long-release factory patches);
    // если first non-init factory patch окажется latch-ным (маловероятно) —
    // взять другой патч явно
    let right_after = render_peak(&mut e, 20);
    assert!(right_after > SILENCE, "release tail should still be decaying");
    settle(&mut e);
    settle(&mut e);
    assert!(render_peak(&mut e, 20) < right_after / 10.0,
        "latch voices must release on latch-mismatch switch (peak {} -> {})",
        right_after, render_peak(&mut e, 20));
}
```

- [x] **Step 4: stealer unit-тест** — в `src/synth/tests.rs` (там `use super::*`,
  есть доступ к Part приватно):

```rust
#[test]
fn steal_prefers_releasing_voice() {
    let mut p = Part::new(48_000.0);
    p.enabled = true; // note_on бейлится на !enabled (mod.rs:613)
    // занять ВСЕ 8 голосов (VOICES_PER_LAYER = 8, mod.rs:376) sustained-нотами;
    // Part::note_on(raw_note: u8, velocity: u8)
    for n in 36..36 + p.voices.len() {
        p.note_on(n as u8, 100);
    }
    // отпустить НЕ-самый-старый голос (40) → он в release-фазе;
    // голос 36 — самый старый активный (старый вор взял бы ЕГО)
    p.note_off(40);
    // новая нота: должна занять release-голос 40 (src_note перезапишется на 80)
    p.note_on(80, 100);
    let v_new = p.voices.iter().find(|v| v.note == 80).unwrap();
    assert_eq!(v_new.src_note, 40,
        "must reuse the RELEASED voice 40 (old stealer takes voice 36)");
    assert!(!v_new.is_releasing() && v_new.active);
    // самый старый активный (36) не тронут
    let v36 = p.voices.iter().find(|v| v.src_note == 36).unwrap();
    assert!(v36.active && !v36.is_releasing() && v36.note == 36,
        "old stealer would have taken voice 36");
}
```

(VOICES_PER_LAYER = 8, mod.rs:376; конструктор `Part::new(48_000.0)` и поля
voices — приватные, но tests.rs — child-модуль synth, доступ есть; прецедент:
src/synth/tests.rs:750 уже правит patch.params. RED-фаза: старый вор берёт голос 36 (min age) → первый ассерт ловит
v_new.src_note == 36 (ожидался 40) → FAIL. НЕ-старый релиз (40) обязателен,
иначе старый и новый вор совпадают и красной фазы нет.)

- [x] **Step 5: прогнать — зафиксировать красные**

Run: `cargo test --test sustain_audit -- --nocapture 2>&1 | tail -40`
Expected RED (до Task 2): pedal/held/program_change seamless-тесты FAIL на
своих settle-ассертах («killed a pedal-held voice» / «held key was killed»);
latch-mismatch FAIL на decay-ассерте; steal-тест FAIL на
assert_eq (v_new.src_note == 36, ожидался 40). Зелёные: panic-тест (guard) и
new_notes_sound_after_patch_switch (guard). Остальные 36 sustain-тестов зелёные.

- [x] **Step 6: Commit**

```bash
git add tests/sustain_audit.rs src/synth/tests.rs
git commit -m "test: seamless patch switch contracts (red — old kill semantics)"
```

### Task 2: Движок — убрать киллы, добавить latch-mismatch, приоритет вора

**Files:**
- Modify: `src/synth/mod.rs:2067-2082` (LoadPatch release-блок), `:1903-1922`
  (ProgramChange release-блок), вор голосов `:709-740`
- Modify: `src/synth/mod.rs` (latch-mismatch release в LoadPatch)

- [x] **Step 1: LoadPatch arm** — удалить из блока (mod.rs ~2067-2082) СТРОКИ:

```rust
                    self.sampler.part_all_notes_off(part);
                    for voice in &mut l.voices {
                        if voice.active { voice.note_off(); }
                    }
                    l.sustained_notes = [false; 128];
                    l.keys_down = [false; 128];
                    l.note_stack.clear();
                    l.latch_held.clear();
                    l.root_held = [false; 128];
                    l.root_active = None;
```

и заменить на latch-mismatch release (ВНИМАНИЕ: `l.min_note/max_note` и все
остальные присваивания блока СОХРАНЯЮТСЯ — удаляется только релиз+чистка):

```rust
                    // Seamless switch (TECHDEBT: политика «хвосты живы»):
                    // sounding voices keep the OLD patch (params are copied
                    // into the voice at note_on); pedal-up / key-offs release
                    // them naturally (set_sustain:1484-1518, note_off:835).
                    // Only a latch-mode mismatch releases the latch-held
                    // voices — a non-latch patch cannot toggle them off, and
                    // they would ring forever (audit fact 6).
                    if l.params.play_mode as u8 == 3 && params.play_mode as u8 != 3 {
                        for voice in &mut l.voices {
                            if voice.active && l.latch_held.contains(&voice.src_note) {
                                voice.note_off();
                            }
                        }
                        l.latch_held.clear();
                    }
```

(сверить: поле play_mode в PatchParams называется `play_mode`, latch хранит
src_note в `l.latch_held: Vec<u8>`; voice.src_note — u8. Если play_mode — u8
enum-число, сравнение `== 3` корректно.)

- [x] **Step 2: ProgramChange arm** — удалить идентичный блок (mod.rs ~1908-1919:
  от `self.sampler.part_all_notes_off(0);` (1908) до `l.root_active = None;` (1919) — весь
  `if let Some(l) = self.parts.get_mut(0) { ... }` оборачивал ТОЛЬКО килл,
  удалить его целиком) и добавить ТОТ ЖЕ latch-mismatch release ВНУТРЬ нового
  `if let Some(l)` ПЕРЕД `l.params = params;` (~1925) — РАНЬШЕ присваивания:
  блок сравнивает СТАРЫЙ play_mode с новым; после присваивания сравнение
  всегда ложно (ревью round-2 C2). PC делает тот же params-swap, без блока
  latch→не-latch оставит latch-голоса звенеть вечно:

```rust
                    if let Some(l) = self.parts.get_mut(0) {
                        if l.params.play_mode as u8 == 3 && params.play_mode as u8 != 3 {
                            for voice in &mut l.voices {
                                if voice.active && l.latch_held.contains(&voice.src_note) {
                                    voice.note_off();
                                }
                            }
                            l.latch_held.clear();
                        }
                    }
```

(params здесь из `preset_params_cache` — тип тот же PatchParams.)

- [x] **Step 3: вор голосов** — в fallback-ветке (mod.rs:727-733) приоритет
  release-фазных:

```rust
                _ => {
                    // Default (Poly / Piano / Latch): JUCE-style steal order —
                    // voices already in release phase die first (their tails
                    // are almost done), then the oldest sustained voice.
                    let releasing = self.voices.iter().enumerate()
                        .filter(|(_, v)| v.active && v.is_releasing())
                        .min_by_key(|(_, v)| v.age);
                    releasing
                        .or_else(|| self.voices.iter().enumerate()
                            .min_by_key(|(_, v)| v.age))
                        .map(|(i, _)| i)
                        .unwrap_or(0)
                }
```

(только default-ветка; Poly-High/Low ветки не трогать. `is_releasing()` —
voice.rs:569, существует.)

- [x] **Step 4: прогнать**

Run: `cargo test --test sustain_audit && cargo test --lib && cargo test && cargo clippy --all-targets -- -D warnings && cargo test --features rt-assert --test rt_safety`
Expected: ВСЕ зелёные, включая новые тесты Task 1 и все существующие
(38+ sustain, root_only, metronome и пр. — 203+ total). Если упали СТАРАЯ
пара тестов — они уже заменены в Task 1; любые другие падения = регресс,
разбирать, не ослаблять.

- [x] **Step 5: Commit**

```bash
git add src/synth/mod.rs
git commit -m "feat: seamless patch switch — tails ring out, held keys sustain (kill removed from LoadPatch/ProgramChange)"
```

### Task 3: Политика + техдолг

**Files:**
- Modify: `docs/TECHDEBT.md`

- [x] **Step 1:** в TECHDEBT.md добавить (после #27):

```markdown
## 28. Политика смены патча: seamless вместо kill-on-switch (2026-10-29, одобрено юзером)
Отменяет требование e91a37b «после смены патча/программы ни одна нота не
звучит». Новая политика: хвосты доигрывают СТАРЫМ патчем (параметры скопированы
в голос на note_on), зажатые клавиши сустейнятся без re-attack; pedal-up /
key-off / CC120/123 / паника — прежние механизмы освобождения. Латч-mismatch
(latch→не-latch) отпускает только latch-голоса. Вор голосов предпочитает
release-фазные. Известные компромиссы: DSP-хвост перекрашивается mod-матрицей
нового патча; педаль-висяк при никогда не отпущенной педали — по вине педали.
Планы (docs/superpowers/plans/2026-10-29-seamless-patch-switch.md) и аудит
семантики — в диалоге 2026-10-29. Компромиссы: mono note_stack и SF2 latch-ноты
ПЕРЕНОСЯТСЯ через смену патча без чистки (самозаживают: stack чистится на
pedal-up/CC123; SF2 latch отпускается pedal-up) — задокументировано ревью.
```

- [x] **Step 2: Commit**

```bash
git add docs/TECHDEBT.md
git commit -m "docs: patch-switch policy — seamless replaces kill-on-switch (#28)"
```
