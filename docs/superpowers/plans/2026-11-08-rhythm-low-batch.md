# Rhythm-LOW батч: 6 фиксов (14-16, 19-21) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (якоря от rhythm-верификатора против 663b03d;
QW3 #106 трогал drum.rs/midi_player.rs/gui-mod — СВЕРЯТЬ ПО ИМЕНАМ, номера
могли съехать ±30)

**Goal:** 6 верифицированных LOW/MED-LOW находок rhythm-аудита: hung SF2 drum
note-off при смене инструмента (14), SMPTE ~2× скорость (15), wrap-темпо
120 на один блок (16), midi_seq mute-зеркало без resync (19), LED-window
optimistic paging (20), замороженные drum-атомы при disabled (21).
(13 — midi_seq_buf realloc — УЖЕ сделано в QW3 Task 14, MIDI_SEQ_BUF_CAP.)

**Architecture:** точечные правки src/synth/{mod,midi_player,drum}.rs +
gui/{midi_seq,mod}.rs. 14 — память маршрута ноты; 15 — ветка SMPTE в
тайм-менеджере; 16 — не сбрасывать темпо на wrap; 19 — revert зеркала при
дропе push; 20 — атом-ресинк окна; 21 — перенос store до early-return.

**Tech Stack:** Rust; юнит-тесты на midi_player (файлы синтезируются в
тестах — прецедент существующих midi-тестов); goldens не затрагиваются
(рендер-путь пресетов не меняется).

**Проверенные факты (663b03d):**
- 14: mod.rs:2380-2394 — note-offs маршрутизируются по seq_route[ch];
  route-2 arm ре-проверяет ТЕКУЩИЙ `instr == Drums` (~2390) и падает в
  `_ => {}` при смене → sampler.drum_note_off не послан → висячий сэмпл.
  Контракт в комментарии seq_route (mod.rs:292-295): маршрутизация должна
  следовать за note-on.
- 15: midi_player.rs:249-256 — SMPTE division (fps×sub) пишется в
  ticks_per_beat и потребляется как метрический (~178); default/reset
  us_per_beat=500_000 (140) → ~2× скорость (комментарий «deliberate
  approximation»).
- 16: midi_player.rs:137-142 reset_position сбрасывает us_per_beat=500_000;
  wrap 219-221 → один блок на 120 BPM до tick-0 TempoChange.
- 19: gui/midi_seq.rs — оптимистичная запись track.muted/instrument (176,
  229-231, 253-255), пуш `let _ =` (264-275); движок применяет
  mod.rs:2037-2050; зеркало пересобирается только на загрузке файла (298).
  rtrb 512 слотов + CountingProducer — дроп редок, но не корректируется.
- 20: gui/mod.rs:2491-2496 — DrumSeqPattern push, затем
  follow_window(next, false) оптимистично; пат-окно ставится 2522-2524;
  drum_current_pattern ресинкится от атома (gui/mod.rs:956), окно — нет.
- 21: drum.rs:1308 `if !self.enabled` early-return ДО store атомов
  play/rec/step (1313-1315) → GUI-индикаторы замораживаются; DrumSeqPlay
  (mod.rs:1664-1667) тем временем переключает sequencer.playing.

---

## Test Cases (from the requirement)

- TC-21: drum engine disabled + DrumSeqPlay toggle → play_atom отражает
  состояние (не заморожен).
- TC-16: файл с начальным темпом 140 BPM, loop on → после wrap первый блок
  НЕ отыгрывается на 120 (us_per_beat сохранён/пере-применён до рендера).
- TC-15: синтезированный SMPTE-файл (30fps/4sub) → событие на tick N
  звучит через N/(fps*sub) секунд от старта (±блок), независимо от tempo
  events.
- TC-14: seq-канал route-2, нота drums активна, трек переключён на Piano →
  note-off доставлен drum-движку (hang отсутствует) — механизм: канал в
  bitmask активных drum-нот.
- TC-19: push в полный rtrb (мок/пепел — если в тесте трудно заполнить
  кольцо: код-ревью revert-логики) → локальное зеркало откатилось.
- TC-20: дроп DrumSeqPattern push → окно ресинкится от атома при следующей
  смене (механизм код-ревью; атом-ресинк тест через существующий gui-atom
  паттерн невозможен без GUI-харнесса — код-ревью).

## Negative Matrix

- 14: канал не в bitmask (обычные синт-ноты seq) — путь не меняется.
- 15: метрический файл (division < 0x8000) — ветка SMPTE не активируется.
- 16: темпо-событие на tick 0 — работает как раньше (пере-применение).
- 19: push успешен — зеркало не трогается (revert только на Err).
- 21: engine enabled — поведение бит-в-бит прежнее.
- ~~~timeout~~ — n/a; дублирующий toggle — 21 идемпотентен.

---

### Task 1: (21) drum.rs — атомы до early-return

**Files:** Modify `src/synth/drum.rs:~1308-1315`.

- [ ] Step 1 (review M6): ДОБАВИТЬ store play/rec/step атомов до
  `if !self.enabled` early-return (из ТЕКУЩИХ полей sequencer — disabled путь
  больше сообщить не может), ПОСТ-ТИК store (1322-1324) СОХРАНИТЬ (перенос
  вместо дублирования сдвинул бы step-атом на блок раньше → сломал бы
  beat-flash gui/mod.rs:959-963). Итого: store до early-return + store после
  tick — без изменений на enabled-пути.
- [ ] Step 2: gates: clippy; `cargo test` (drum suite); rt_safety.
- [ ] Step 3: коммит `fix: drum atoms updated even when engine disabled (audit rhythm #21)`.

### Task 2: (16) midi_player.rs — wrap не сбрасывает темпо

**Files:** Modify `src/synth/midi_player.rs:137-142 (reset_position), 219-221`.

- [ ] Step 1: failing-тест: MidiSeqData собирается НАПРЯМУЮ (SMF-writer'а в
  тестах НЕТ; прецеденты: tests/sustain_audit.rs:864-872 empty_seq_data,
  sampler.rs:708): tempo 140 (us_per_beat=428_571) на tick 8 (НЕ tick 0),
  loop on; process_block через wrap → us_per_beat после wrap == 428_571 (не
  500_000). Поле us_per_beat pub (review M10).
- [ ] Step 2: fix (review C1): reset_position НЕ сбрасывает us_per_beat
  (поле живёт между wrap'ами), а `load()` ЯВНО выставляет 500_000 перед
  первым parse — иначе новый файл унаследует темп предыдущего:
```rust
    pub fn load(&mut self, ...) {
        ...
        self.us_per_beat = 500_000; // fresh file: default tempo until first TempoChange
        ... reset_position() ...
    }
    pub fn reset_position(&mut self) {
        self.cursor = 0; // + прочие поля по факту; us_per_beat НЕ сбрасывать
    }
```
  (под фактические поля/сигнатуры по месту 137-147.)
- [ ] Step 3: gates: clippy; cargo test; rt_safety.
- [ ] Step 4: коммит `fix: loop wrap keeps current tempo — no one-block 120BPM hiccup (audit rhythm #16)`.

### Task 3: (15) midi_player.rs — SMPTE честный тайминг

**Files:** Modify `src/synth/midi_player.rs` (parse 249-256; advance/tick
логика ~165-230).

- [ ] Step 1: failing-тест: SMF-байты собираются ВРУЧНУЮ через parse_midi
  (единственный байт-прецедент: tests/midi_parse.rs:105-116, division 0xe7 0x28).
  ВАЖНО (review I2): SMPTE division хранит fps ОТРИЦАТЕЛЬНЫМ байтом:
  division = ((-(fps as i8)) as u8) << 8 | sub → 30fps/4sub = 0xE204 (НЕ
  0x8000|fps<<8 — check_smf_header (src/midi.rs:243-253) допускает только
  −24/−25/−29/−30). Нота на tick N → assert время ≈ N/(fps*sub) сек
  (±1 блок).
- [ ] Step 2: реализация: поле `smpte_ticks_per_sec: Option<f32>` (None для
  метрических). Parse: если division & 0x8000 → Some(fps*sub as f32), tempo
  events ИГНОРИРУЮТСЯ в этой ветке. Advance-ветка process_block: при Some(tps)
  `ticks_advanced = samples * tps / sample_rate` вместо us_per_beat-математики.
  НАСТРОЙКА (review M9): юзер-BPM override на SMPTE-файлах не действует
  (SMPTE = true-time; задокументировать комментарием у поля).
  НОВЫЕ ЛИТЕРАЛЫ СТРУКТУРЫ (review I5): `MidiSeqData` строится литералом в
  tests/sustain_audit.rs:864-872 (empty_seq_data) и src/synth/sampler.rs:708 —
  оба добавить `smpte_ticks_per_sec: None`. Сброс при load.
- [ ] Step 3: gates: clippy; cargo test; rt_safety.
- [ ] Step 4: коммит `fix: SMPTE midi files play at true fps*sub rate (audit rhythm #15)`.

### Task 4: (14) mod.rs — note-off по маршруту note-on

**Files:** Modify `src/synth/mod.rs` (route-2 arm ~2380-2394; bitmask рядом с
seq_route ~292-295; note-on сайт route-2).

- [ ] Step 1: состояние: `seq_drum_active: u16` (bitmask каналов с активными
  seq drum-нотами) рядом с seq_route. СБРОСЫ (review round-2 F1): (а)
  MidiSeqLoad-хендлер рядом с обнулением seq_route (~2012); (б) паника
  CC120/123 arm (~1143-1174, рядом с sampler.all_sound_off); (в)
  MidiSeqPlay{playing:false} arm (~2028-2046, рядом с seq_channels_release) —
  иначе stale бит уводит ПОСЛЕДУЮЩИЕ синт-нот-off канала в drum_note_off
  (то же класс зависания, что чинит #14).
- [ ] Step 2: note-on route-2 (drums) — установить бит канала; note-off:
  drum_note_off доставлять если `бит установлен || seq_route[ch]==2`
  (review M8a: пересекающиеся ноты — первый off снимает бит, второй off при
  route==2 тоже должен дойти; route = sticky-память канала, бит защищает от
  промежуточной не-drum note-on); sf2_mode-гвард существующего кода
  СОХРАНЯЕТСЯ (~2406-2410 — бит/route решают «чьё событие», sf2_mode решает
  «какой drum_note_off», review round-2 F3); остальные — как раньше.
- [ ] Step 3: gates: clippy; cargo test; rt_safety (u16 bitmask — без
  аллокаций).
- [ ] Step 4: коммит `fix: seq drum note-off follows note-on route — no hung SF2 samples (audit rhythm #14)`.

### Task 5: (19) gui/midi_seq.rs — revert зеркала при дропе push

**Files:** Modify `src/gui/midi_seq.rs` (176/229-231/253-255 записи; 264-275
пуши).

- [ ] Step 1: СТРУКТУРА (review I4): записи зеркала (176/229-231/253-255)
  происходят ВНУТРИ цикла строк таблицы и депонируются в векторы
  instr_changes/mute_changes (155-156); пуши — ПОСЛЕ цикла (264-275), где
  track/prev в скоупе НЕТ. Prev-снапшоты протаскивать через векторы:
  `Vec<(usize, bool /*new*/, bool /*prev*/)>` для mute и
  `Vec<(usize, ..., ... /*prev*/)>` для instrument (сайт SF2-программы
  253-255 тоже кладёт в instr_changes — prev нужен и там). НЕ «пере-флипать»
  mute (два тоггла в кадре сломают prev). Пуши 264-275: при Err — откат по
  prev из вектора:
```rust
                if self.ctrl_tx.push(ev).is_err() {
                    for (idx, _, prev) in &mute_changes { /* вернуть prev */ }
                }
```
  (фактическую структуру векторов/пушей реализатор читает 155-156, 264-275.)
- [ ] Step 2: gates: clippy; cargo test.
- [ ] Step 3: коммит `fix: midi_seq mirror reverts when control push drops (audit rhythm #19)`.

### Task 6: (20) gui/mod.rs — LED-окно ресинк от атома

**Files:** Modify `src/gui/mod.rs` (~2491-2496 follow_window; ~2522-2524
pat_window; ~956 атом-ресинк).

- [ ] Step 1 (review I3/M7; поле `pending_window_pattern: Option<u8>` — в
  struct App рядом с pat_window (~407), init в литерале App::new (~657,
  рядом с drum_current_pattern: 0); E0063-компилятор укажет, если пропущено):
  ОБА optimistic-сайта — StepPattern (~2493-2498)
  И SelectPattern (~2443-2449, тоже push+follow_window(idx,false)) — не двигать
  окно сразу: поле `pending_window_pattern: Option<u8>`, ставить вместо
  follow_window. В месте атом-ресинка (~956, выполняется каждый кадр):
```rust
            if self.pending_window_pattern == Some(atom_pattern) {
                self.follow_window(atom_pattern as usize, false);
                self.pending_window_pattern = None;
            }
```
  follow_start идемпотентен (no-op если idx уже в окне, led_console.rs:79-89);
  ручной пейджинг page_window (~2530-2538) не задет (pending стреляет только
  по подтверждению атомом).
- [ ] Step 2: gates: clippy; cargo test.
- [ ] Step 3: коммит `fix: console LED window follows confirmed pattern atom (audit rhythm #20)`.

### Task 7: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens.
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.

## Out of scope
- 13 — уже сделано (QW3 Task 14, MIDI_SEQ_BUF_CAP).
- 4 (boundary block-rate), 5 (arbitration), 6 (capacity wrap), 7-related
 swing — отдельные планы / юзер-решения (не в этом батче).
