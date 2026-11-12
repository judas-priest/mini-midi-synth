# Bass/Root latch: корень живёт до полного отпускания Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 8411332; якоря сняты чтением: part.rs:602-673
root_on/root_off, mod.rs:505-610 route_note_on/route_note_off, events.rs:70-83
RootDecision/NOTE_MODE_*, part.rs:227 init, part.rs:678-696 mode-switch reset)

**Goal:** в режимах Bass/Root отпускание части аккорда НЕ меняет звучащий
корень — он звучит до отпускания ВСЕХ клавиш (канон аранжерных басов); новые
нажатия пересчитывают корень как раньше (дозревание аккорда при
последовательном нажатии).

**Architecture:** root_off теперь tri-state `RootOff` (events.rs, рядом с
RootDecision): Pass (POLY/не задет — обычный off), Latch (held остались — off
сырого raw ПОДАВЛЕН, голос корня звучит дальше), Release { note } (всё
отпущено — off голоса последнего корня). root_off умирает: пересчёт таргета и
re-key по отпусканиям удалены (release никогда не пере-детектит — канон
аранжеров); root_on НЕ меняется (пересчёт по нажатиям = дозревание аккорда).
route_note_off (mod.rs): gated-ветка переписана на match по RootOff (SF2 и DSP
ветки).

**Tech Stack:** Rust; существующие note-mode тесты (grep root_on/root_off в
tests/) обновляются под latch-семантику — это ОСОЗНАННЫЙ behavior change по
решению юзера (2026-11-08, «хвост из другой ноты» при неровном отпускании).

**Проверенные факты (8411332):**
- part.rs:602-642 root_on: held[raw]=true; стек-буфер held; min_held — якорь
  басовой октавы; NOTE_MODE_ROOT → detect_root(held) + i16 rem_euclid-дельта
  (saturing_sub); re-key/Suppress по root_active; POXY/play_mode 3 — bypass.
- part.rs:646-673 root_off: held[raw]=false; пусто → root_active=None, None;
  иначе re-detect и Some(новый таргет) при изменении — ЭТО удаляем.
- mod.rs:511: `let RootDecision::Sound { release, note } = root_on` — on-сторона
  не меняется.
- mod.rs:548-610 route_note_off: SF2-ветка (mono-stack только POLY;
  sampler.note_off(raw); re-key по `next`+old_root: note_off(old)+note_on(n,
  last_vel)); DSP-ветка (parts.note_off(raw); force-release v.src_note==raw +
  sustained_notes[raw]=false — пин #24; re-key: release old + note_on(n,
  last_vel)); gated = note_mode != POLY.
- part.rs:678-696: сброс root-состояния при смене режима (SetPartNoteMode,
  mod.rs:1536-1541 «ghost chord») — не меняется.
- Педаль (#24): force-release по key-up остаётся на Release-пути (очистка
  sustained_notes[note]); на Latch — флаг не трогается (голос жив, пока аккорд).
- MSEG-релиз (review I1): сегодня gated off всегда зовёт parts.note_off(raw),
  который внутри делает `if !has_active_notes() { mseg1/2.release() }`
  (part.rs:580-583). НОВЫЙ Release-arm обязан повторить этот чек (полный
  релиз = нет активных голосов → MSEG release), иначе MSEG-патч в Bass/Root
  дронит вечно — регресс, невидимый goldens.
- Looper offs: route_looper_note_offs (mod.rs:632-645) идёт МИМО
  route_note_off — mute/stop лупера глушат звук независимо от latch
  (review: отметить в плане, легко перепутать).
- Тесты на re-key по отпусканию (review I2 — 4 шт, src/synth/tests.rs):
  root_only_release_of_root_rekeys_next_lowest (:635),
  root_mode_follows_chord_root_and_rekeys_back (:679, asserts 701-705),
  root_mode_rekeys_on_chord_member_release_even_under_synthesized_root (:754),
  root_only_rekey_releases_old_root_even_under_pedal (:813) — сценарии
  сохраняются, ожидания переворачиваются на latch (осознанный behavior
  change по решению юзера; НЕ ослаблять — те же сценарии, инвертированные
  ассерты). root_only.rs:29 — комментарий про re-key устаревает, обновить.
- drain_notes vs mode-switch reset (review M5): part.rs:678-696 — это
  drain_notes; reset при SetPartNoteMode живёт в mod.rs:1534-1544; ОБА чистят
  root_held и root_active (mod.rs:1542-1543; part.rs:695-696).
- Existing note-mode тесты: grep `root_on\|root_off` по tests/ — обновляемые
  места находит реализатор; их семантика «отпустил низкую — зазвучала
  следующая» МЕНЯЕТСЯ на latch по решению юзера.

---

## Test Cases (from the requirement)

- TC1 (главный, юзер-сценарий): Bass, аккорд C-E-G (60/64/67) → звучит C;
  отпустил C (низкий) → звучит ПО-ПРЕЖНЕМУ C; отпустил E → C; отпустил G
  (всё) → тишина; новое взятие G-B-D → G.
- TC2 (Root): C-E-G → detect_root → C; отпустил C первым → C звучит;
  полный отпуск → reset; новое взятие E-G-B → корень E.
- TC3 (async-нажатие — дозревание сохранено): G нажата → бас G; +C →
  re-key на C (detect {G,C}); +E → C. (GREEN до фикса — регресс-замок.)
- TC4: неполосный raw при latch: отпустил верхнюю ноту E (не корень) →
  off E подавлен (нет голоса на E — harmless), бас C жив.
- TC5: педаль + latch: C-E-G с педалью, отпустил C → C не resurrect-ится и
  не глушится (голос жив по latch); полный отпуск → off C очищает
  sustained_notes[C].
- TC6: смена note_mode при зажатом аккорде (SetPartNoteMode) → root-состояние
  сброшено (существующий reset part.rs:678-696) — offs после смены идут
  Pass-путём.
- TC7 (SF2-ветка): тот же TC1 через sampler (sf2_mode=true) — Latch подавляет
  sampler.note_off; Release → sampler.note_off(tr(note)).
- Негатив: POXY — off как раньше; held пуст при off (ghost off) — Pass, без
  паники; re-strike корня во время latch (C снова нажата) → root_on:
  target==active → Suppress (дубликат, без ретрига) — существующее поведение.

## Negative Matrix

-POLY-режим — RouteOff::Pass дословно (гвард первой строкой).
- ghost off (ключ не была зажата): held[raw] уже false; any() решает по
  ОСТАЛЬНЫМ — если аккорд держится, ghost off → Latch (подавлен, harmless);
  если пусто → Release{root_active} — существующий ghost-семантика.
- re-strike звучащей ноты — Suppress в root_on (без изменений).
- ~~~timeout/clock~~ — n/a; ~~~конкурентность~~ — RT однопоточный.

---

### Task 1: RootOff enum + characterization (RED)

**Files:** Create tests in `src/synth/tests.rs` (рядом с 4 re-key тестами
:635/:679/:754/:813 — их НЕ трогать в Task 1, они документируют текущее
re-key поведение; инверсия = Task 2 Step 4).

- [ ] Step 1: characterization-тесты — STATE-BASED ассерты через ТЕКУЩИЙ API
  (root_on возвращает RootDecision, root_off Option<u8>, поля root_active/
  root_held pub(super) — синт-тесты видят; НИКАКОГО нового API в Task 1):
  * `bass_latch_survives_partial_release` (TC1): mode=BASS; root_on(60/64/67);
    root_off(64) → None (сейчас Some(64) — RED здесь: latch ждёт None);
    root_active == Some(60) (сейчас Some(64) — второй RED-ассерт);
    root_off(60) → None... (RED: сейчас Some(60)); root_off(67) → None;
    root_active == None (GREEN уже).
  * `root_mode_latch_and_redetect` (TC2): mode=ROOT; on 60/64/67 →
    Sound C; off 60 → None + root_active == Some(C-таргет) (RED: сейчас
    re-key); полный отпуск → root_active == None; новое on 64/67/71 →
    detect({64,67,71}) → корень E (Sound) — GREEN (это root_on, не меняется).
  * `async_growth_rekey_preserved` (TC3): on 67 → Sound G; on 60 →
    Sound { release: Some(67), note: C }; on 64 → Suppress (target C
    unchanged). GREEN до фикса — регресс-замок (root_on не меняется).
  * `mode_switch_resets_latch` (TC6): SetPartNoteMode mid-hold (mod.rs:1534-1544
    чистит root_held+root_active) → root_off → None, root_active None.
    Ассерт ТОЛЬКО на сброс трекинга (review M6): тишина НЕ гарантируется —
    synthesized root (target≠raw) звучит до drain/pedal-up, ПРЕ-СУЩЕСТВУЮЩИЙ
    hang, Out of scope.
  * `bass_full_release_silence` (review gap): полный отпуск в gated DSP →
    нет активных голосов (is_releasing допустим) — явный silence-ассерт.
  * `gated_mseg_releases_on_full_release` (review I1): MSEG-патч в
    Bass-режиме: аккорд → полный отпуск → mseg1/2 released (доступ по
    факт. именам полей state).
- [ ] Step 2: `cargo test` → TC1/TC2 RED (сейчас off ре-key'ит: root_active
  меняется), замки/негативы GREEN. Зафиксировать список RED.
- [ ] Step 3: коммит `test: bass/root latch characterization (red = target latch semantics)`.

### Task 2: реализация — root_off tri-state + route_note_off

**Files:** Modify `src/synth/events.rs` (рядом с RootDecision 74-81 — НОВЫЙ
enum):
```rust
/// Note-off side решения note-mode гейта (TECHDEBT #24-семейство, latch-
/// семантика аранжерных басов, юзер-решение 2026-11-08): отпускание части
/// аккорда никогда не пере-детектит корень.
pub(crate) enum RootOff {
    /// POLY/ghost — обычный off по raw.
    Pass,
    /// Остались зажатые: off raw ПОДАВЛЕН, голос корня продолжает звучать.
    Latch,
    /// Все клавиши отпущены: off голоса последнего корня и сброс latch.
    Release { note: u8 },
}
```
Modify `src/synth/part.rs:646-673 (root_off)`,
`src/synth/mod.rs:548-610 (route_note_off)`, `src/synth/mod.rs:157-159`
(экспорт RootOff), `src/synth/tests.rs` (инверсия 4 re-key тестов,
см. Task 1 список), `tests/root_only.rs` (:29 комментарий).

- [ ] Step 1: part.rs root_off — тело заменить целиком (сигнатура → RootOff;
  тесты из Task 1, писавшие state-ассерты, остаются валидными — они
  проверяют root_active, теперь офиц. путь возвращает tri-state):
```rust
    /// Note-mode gate, note-off side (latch-семантика, юзер-решение
    /// 2026-11-08): отпускания НИКОГДА не пере-детектят корень — он звучит
    /// до полного отпускания (канон аранжерных басов). root_on остался
    /// точкой дозревания аккорда (re-detect по нажатиям).
    pub(super) fn root_off(&mut self, raw: u8) -> RootOff {
        self.root_held[raw as usize] = false;
        if self.note_mode == NOTE_MODE_POLY { return RootOff::Pass; }
        if self.root_held.iter().any(|&h| h) { return RootOff::Latch; }
        // Все клавиши отпущены: off голоса последнего корня и сброс.
        match self.root_active.take() {
            Some(note) => RootOff::Release { note },
            None => RootOff::Pass,
        }
    }
```
- [ ] Step 2: mod.rs route_note_off — вызов: `let next_root =
  self.parts[i].root_off(raw);` (старое чтение `old_root: p.root_active` из
  tuple-пролога УДАЛИТЬ — больше не нужно; `next`/`old_root` из старого кода
  не существуют). Импорт RootOff в use-строки mod.rs (157-159 блок).
  SF2-ветка: ПЕРЕД существующим mono/POLY-блоком:
```rust
            if gated {
                match next_root {
                    RootOff::Latch => { return; } // корень звучит — off подавлен
                    RootOff::Release { note } => {
                        self.sampler.note_off(i, tr(note));
                        return;
                    }
                    RootOff::Pass => { self.sampler.note_off(i, tr(raw)); return; }
                }
            }
```
  (mono-stack-блок внутри SF2-ветки остаётся ТОЛЬКО для POLY — существующий
  `mono && POLY` гвард уже отфильтровывает gated; gated не доходит до него.)
  DSP-ветка:
```rust
        } else {
            match next_root {
                RootOff::Latch => {} // корень звучит — off raw подавлен целиком
                                     // (пин #24: force-release только на Release)
                RootOff::Release { note } => {
                    let part = &mut self.parts[i];
                    for v in &mut part.voices {
                        if v.active && v.src_note == note { v.note_off(); }
                    }
                    part.sustained_notes[note as usize] = false;
                    // MSEG-релиз: чек из parts.note_off (part.rs:580-583) —
                    // прежний gated off звуал note_off, который делал этот
                    // чек; без него MSEG-патч дронит после полного релиза
                    // (review I1).
                    if !part.has_active_notes() {
                        part.mseg1_state.release();
                        part.mseg2_state.release();
                    }
                }
                RootOff::Pass => {
                    self.parts[i].note_off(raw);
                    if gated {
                        // Пин #24: педаль не resurrect-ит корень с отпущенной
                        // клавишей (прежний force-release по raw).
                        let part = &mut self.parts[i];
                        for v in &mut part.voices {
                            if v.active && v.src_note == raw { v.note_off(); }
                        }
                        part.sustained_notes[raw as usize] = false;
                    }
                }
            }
        }
```
  (структуру адаптировать по фактической — смысл: Latch не трогает НИКАКИЕ
  голоса; Release глушит src_note==note и чистит sustained flag; Pass =
  прежний путь, для gated — с force-release по raw.)
- [ ] Step 3: старые re-key блоки (note_off(old)+note_on(n,last_vel)) в
  ОБОИХ ветках УДАЛИТЬ — их семантика заменена latch'ем. Imports: part.rs:6
  добавить RootOff к use super::events::{...}; mod.rs:159 pub(crate) use —
  добавить RootOff. Из tuple-пролога route_note_off удалить last_vel
  (review M3: его потребители — re-key блоки — удалены; иначе unused warning
  ломает clippy-гейт). keys_down[target] при Latch НЕ чистится — НАМЕРЕННО
  (release_pedal_held part.rs:705-724 щадит latched корень = желаемое
  поведение; инвариант «keys_down = физические клавиши» нарушается
  осознанно, review M4 — комментарий в коде).
- [ ] Step 4: обновить существующие note-mode тесты, assertion'ы которых
  ждут re-key по отпусканию (список RED из Task 1 + grep root_off в tests/):
  «отпустил низкую — зазвучала следующая» → «отпустил низкую — корень
  держится; полный отпуск — тишина».
- [ ] Step 5: gates: `cargo clippy --all-targets -- -D warnings`;
  `cargo test` (все characterization зелёные); `cargo test --test golden_fx`;
  `cargo test --features rt-assert --test rt_safety`.
- [ ] Step 6: коммит `feat: bass/root modes latch the root until full release — no tail jumps on uneven release (user decision 2026-11-08)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens
  (рендер пресетов не задет — POLY-путь не тронут; `git status tests/golden`
  пустой).
- [ ] Step 2: PR → squash → rebuild → launch → ЭКРАН (юзер): аккорд в
  Bass/Root, неровное отпускание — бас не прыгает; полное отпускание →
  тишина; новое взятие — новый корень; педаль не resurrect-ит отпущенный
  корень.

## Out of scope
- Полифония и play_mode 3 (arp) — bypass как сейчас.
- «Корень = первая нажатая» (вариант 2 юзера) — не выбран.
- SF2 mono-stack (POLY last-note) — не задет.
- Подача корня в looper-record (replay получает offs по отпусканиям — latch
  применяется и на реплее, семантика равномерная).
