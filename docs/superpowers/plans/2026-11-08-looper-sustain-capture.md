# Лупер: захват сустейна (deferred note-off при педали) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 8411332; якоря сняты напрямую: looper.rs:574-589
record_event + active_notes-трекер; mod.rs:467-479 set_sustain; 280 sustain
AtomicBool; record_for_looper mod.rs:652-658; сайты записи off 625/1017/1052)

**Goal:** при записи лупера зажатая педаль НЕ «съедает» сустейн: note-off
записывается не в момент отпускания клавиши, а в момент отпускания ПЕДАЛИ —
длина ноты в лупе = тому, что слышал юзер. Воспроизведение повторит
сустейн-эффект без трогания глобальной педали (piano-roll подход: удлинённый
гейт).

**Architecture:** НЕ записывать CC64 в луп (педаль глобальная — реплей задел
бы живую игру). Вместо: в record_for_looper для note-off (velocity==0), если
педаль вниз — НЕ писать off, а положить (note, part) в pending-список; при
pedal-up (в set_sustain, ветка on→off) — сбросить весь pending как обычные
offs (текущий tick). При закрытии capture (master_stop/close) — flush
оставшегося pending (иначе зависшие active_notes). Playback не меняется
вообще (обычные события, просто более длинные гейты).

**Tech Stack:** Rust; characterization-тесты в tests/master_rec_sync.rs-стиле
(harness уже есть); goldens не задеты (looper вне preset-рендера).

**Проверенные факты (8411332):**
- looper.rs:574-596 record_event: vel==0 → off (гвард active_notes[note]!=part
  → ignore); vel>0 → on, active_notes[note]=part. Актив-трекер уже решает
  «off принадлежит держателю».
- mod.rs:467-479 set_sustain(value): `let on = value >= 64;` — ветка pedal-up
  = `if !on` (476-478) — точка flush'а pending.
- mod.rs:280 `sustain: Arc<AtomicBool>` — текущее состояние педали, читается
  в record_for_looper.
- PER-PART (review I2): педаль применяется как `on && part.sustain_enabled`
  (475, «bass without sustain») — часть с выключенным сустейном НЕ тянется
  живьём → её off НЕ деферить (иначе луп удлиняет то, что не звучало).
- sustain_release (477): DSP-ноты после pedal-up затухают плавно ЖИВЬЁМ, луп
  режет по pedal-up — известное расхождение, Out of scope (review M5).
- mod.rs:652-658 record_for_looper: guard source_mix → looper.record_event.
  Вызовы off: 625, 1017/1052 (MIDI note-off) + 2436-2443 (arp gate-off цикл,
  on на 2450) — review I1. УЗКОЕ МЕСТО: арп-гейты ритмичные — defer рвал бы
  паттерн на рваные гейты и забивал бы pending за один проход → РЕШЕНИЕ:
  arp-generated offs НЕ деферятся (пишутся сразу).
- ВЫХОДЫ из Recording/Overdubbing — ВСЕ СЕМЬ (review round-2 F1, HEAD c80f62a):
  master_stop mod.rs:694; quantized close 2269; fallback 2296; free-close
  (master_toggle_record_free_looper) 825; overdub-layer finish на boundary
  2233 (Overdubbing→Playing); LooperStopRecord 1784 (looper.rs:341-348);
  LooperTogglePlay при Overdubbing 1785 (→ stop → Idle). Pending без flush
  на любом из них — stale offs/зависшие ноты.

---

## Test Cases (from the requirement)

- TC1 (главный, characterization): MasterRecord → note-on(60) → note-off(60)
  ПРИ pedали вниз → в лупе off НЕТ; pedal-up → off появился, его tick ≈
  момент pedal-up. После close: воспроизведение — нота звучит до pedal-up.
- TC2: pedal вниз, но нота отпущена ПОСЛЕ pedal-up (обычный порядок) →
  off записывается сразу на note-off, pending пуст (deferred только для
  off-во-время-педали).
- TC3: pedal вниз, capture закрывается (MasterStop) с висящим pending →
  pending сброшен (offs записаны/active_notes почищены), луп не держит
  зависшие ноты.
- TC4: pedal вверх при НЕ-активной записи → flush no-op (нет паники/мусора).
- TC5: несколько нот в pending (аккорд при педали) → все offs на pedal-up.
- TC6: ре-страйк ноты при pending (педаль вниз, та же нота снова) — два
  pending-энтри; flush: первый off чистит active_notes, второй guard-dropped
  (looper.rs:586-588) — дубли не доходят (закрепить тестом, review I4).
- TC7: педаль через wrap: wrap forced-off (702-712) чистит active_notes БЕЗ
  записи off → поздний flush guard-dropped → слышимая длина = wrap. Зафиксить
  тестом (review I4).
- TC8: overdub с pending (defer работает и в Overdubbing — review M8).
- TC9: AllNotesOff/CC123 при записи → set_sustain(0) ЛЕГИТИМНО триггерит
  flush (1149/1847) — offs записаны до mute (закрепить тестом, review I4).
- TC10: часть с sustain_enabled=false: off пишется сразу, pending не растёт
  (review I2).
- Негатив: pedal-up вне записи — set_sustain как раньше; wrap Overdubbing с
  pending — offs после wrap пишутся с новым tick.
- ~~~timeout~~ — n/a.

---

### Task 1: characterization — зафиксировать ТЕКУЩЕЕ (сломанное) поведение

**Files:** Modify `tests/master_rec_sync.rs` (по образцу tc13/tc14).

- [ ] Step 1: тест `sustain_captured`: capture → note-on → note-off при
  педали (SetSustain через handle_control, mod.rs:1840-1842 — harness
  master_rec_sync поддерживает) → pedal-up → close → ЦЕЛЕВОЕ поведение:
  off на pedal-up. RED до Task 2, зелёный после (TDD; review M7 — одна
  формулировка, без «assert текущее»).
- [ ] Step 2: тесты TC2-TC10 (TC2/TC4/TC6/TC7/TC9/TC10 зелёные сразу —
  регрессионный замок; TC3/TC5/TC8 red→green как TC1).
- [ ] Step 3: gates: clippy; cargo test (RED-тесты перечислить в отчёте).
- [ ] Step 4: коммит `test: looper sustain capture characterization (red = target behavior)`.

### Task 2: deferred note-off

**Files:** Modify `src/synth/mod.rs` (поля + set_sustain + record_for_looper +
close-пути), возможно `src/synth/looper.rs` (если flush нужен helper — по
факту record_event достаточно).

- [ ] Step 1: поля SynthEngine: `looper_sustain_pending: [(u8, u8); 16]` +
  `looper_sustain_pending_len: usize` (НЕ Vec: push на RT = аллокация).
  Максимум 16 одновременных pending; переполнение → off записывается
  немедленно как раньше + metrics-счётчик.
- [ ] Step 2: record_for_looper: ветка velocity==0:
```rust
        if velocity == 0
            && self.sustain.load(..) // педаль вниз
            && self.parts[part as usize].sustain_enabled // review I2
            && !is_arp_off // arp-сайты 2436-2443 помечены: off сразу (review I1)
            && self.looper.is_capturing() // Recording ИЛИ Overdubbing (review M8)
        {
            push в pending (note, part); // фикс-массив, overflow → off сразу
            return;
        }
```
  is_capturing — новый pub fn в looper.rs (state ∈ {Recording, Overdubbing});
  defer_ok по сайтам (review round-2 F2: сайт 625 — внутри flush_arp_note_off
  615-628, это ARP-generated off → false!): TRUE только 1017/1052 (arp-ветка
  1003-1010 перехватывает arp offs раньше — безопасно); FALSE 625 и 2442.
  ВЕТКА ПОСЛЕ source_mix guard'а (review round-2 F3: иначе части вне записи
  копят pending впустую, съедая лимит 16).
- [ ] Step 3: set_sustain: flush ОДИН РАЗ после per-part цикла (не внутри
  него — review M6): рекурсии нет без флага — `sustain.store(false)` на 469
  стоит ДО ветки, record_for_looper уже видит педаль вверх и пишет off
  напрямую (зафиксировать комментарием).
- [ ] Step 4: ВСЕ СЕМЬ выходов (review round-2 F1): flush-ДО-выхода на
  694 / 825 / 2233 / 2269 / 2296 (запись ещё валидна: flush ПЕРЕД
  trim_after 679/2260 и ПЕРЕД toggle_record 694/2269/2296 — record_event
  требует Recording/Overdubbing, looper.rs:575). Для 1784 (LooperStopRecord)
  и 1785 (TogglePlay→Idle): flush ДО stop (состояние ещё Recording/Overdubbing)
  — выбрать один вариант и закрепить (рекомендация: flush везде, pending
  очищается в конце каждого пути).
  При этом fallback 2296 — flush ДО toggle_record 2296, затем pending = 0.
- [ ] Step 4b: инвариант: после ЛЮБОГО выхода pending_len == 0 (закрепить
  debug_assert).
- [ ] Step 5: gates: clippy; cargo test (все characterization зелёные,
  включая вчерашние red); rt_safety (фикс-массив — без аллокаций);
  golden_fx.
- [ ] Step 6: коммит `feat: looper captures sustain — note-off deferred to pedal-up (user decision 2026-11-08)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens.
- [ ] Step 2: PR → squash → rebuild → launch → ЭКРАН (юзер): записать с
  педалью — воспроизведение держит ноты до отпускания педали; педаль при
  воспроизведении не влияет на луп (global pedal осталась только для живой
  игры — сверить!).

## Out of scope
- Запись CC64 в луп как события (глобальная педаль затронула бы живую игру
  при реплее) — отклонено юзер-решением (piano-roll подход выбран).
- SF2-специфика: rustysynth держит ноты каналом — deferred off корректно
  повторяет услышанное и для SF2 (проверить экраном).
