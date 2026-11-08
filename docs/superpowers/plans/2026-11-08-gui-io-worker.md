# GUI-thread I/O: SF2/MIDI-парс уходит в worker-поток Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 8411332; якоря сняты напрямую с кода:
src/gui/sf2.rs:106-142/158-180, src/gui/midi_seq.rs:281-330, src/gui/mod.rs:556/737-779)

**Goal:** выбор/загрузка SF2-саундфонта и MIDI-файла больше не фризит GUI
(парс десятков мегабайт синхронно в update()); парс уходит в одноразовый
std::thread, GUI опрашивает mpsc каждый кадр, применяет результат существующими
ControlEvent-путями (RT-сворап уже trash-защищён — Sf2(Displaced), MidiSeq).

**Architecture:** per-pick spawn std::thread (нагрузка юзер-paced, потоки
штучные); результат через std::sync::mpsc::channel — Receiver в App, try_recv
в начале update(). Пока загрузка в полёте — повторный клик игнорируется
(флаг-статус). RT-сторона НЕ меняется: движок получает тот же
LoadKeysSoundFont/LoadDrumsSoundFont/MidiSeqLoad.

**Tech Stack:** Rust std (mpsc, thread); существующие типы Arc<SoundFont> /
MidiSeqData уже Send. Тесты: логика poll-обработки тестируется подачей
результата в канал напрямую; сами парсы на потоках не юнит-тестятся
(goldens/rt_safety не задеты — RT-поток не участвует).

**Проверенные факты (8411332):**
- gui/sf2.rs:106-120: `parse_sf2(&mut self, path) -> Option<Arc<SoundFont>>` —
  File::open + rustysynth::SoundFont::new СИНХРОННО (GUI); ошибки через
  `show_toast(..., ToastKind::Error)` (114/117) — ПОЛЯ sf2_status НЕ СУЩЕСТВУЕТ
  (review I1: только в doc-комментарии).
- gui/sf2.rs:121-142 `load_sf2_keys(idx)`: parse → `self.sf2_keys_soundfont =
  Some(sf)` → `ctrl_tx.push(LoadKeysSoundFont { soundfont: sf })` →
  имя/статус. 158-180 `load_sf2_drums(idx)`: кэш (`if sf2_keys_loaded_name ==
  name { clone keys }` else parse) → LoadDrumsSoundFont.
- gui/midi_seq.rs:281-330 `do_load_midi()`: fs::read + parse_midi
  СИНХРОННО; Ok → GUI-список треков (298-306) + счётчики + (далее по телу —
  push MidiSeqLoad с Box данных); Err → toast.
- Прецедент фоновой операции в App: `midi_seq_file_pick: Arc<Mutex<Option<String>>>`
  (mod.rs:556, init 779, опрос midi_seq.rs:79-85).

---

## Test Cases (from the requirement)

- TC1/TC2/TC4 — КОД-РЕВЬЮ УРОВЕНЬ (App-конструируемого harness в gui-тестах
  НЕТ — все #[cfg(test)] модули pure-function: mod.rs:2635, led_console.rs:365,
  theme.rs:257, note_flash.rs:53, layers.rs:718; review I8): применяющая
  логика выносится в чистые fn по возможности, верификация = ревью + экран.
- TC3 (guard): повторный клик при in-flight (в т.ч. КРОСС-KIND keys↔drums и
  unload «(none)» при in-flight) → не спавнит второй поток / не перезаписывает
  rx — код-ревью гварда.
- TC4: midi-результат → список треков + MidiSeqLoad как в синхронной ветке
  (поля data эквивалентны).
- Негатив: канал пуст (обычный кадр) — try_recv Err → no-op; RX при
  завершённом sender — не паникуем (Disconnected = toast «loader died»).
- startup-restore: SF2 из config при старте — async (см. Task 1 Step 2
  STARTUP-СЕМАНТИКА); порядок engine-событий сохранён.
- MIDI empty path — молча, как сейчас (гвард до spawn).
- ~~~timeout~~ — парс может длиться сколько угодно, GUI жив; ~~~clock~~ — n/a.

---

### Task 1: SF2 worker

**Files:** Modify `src/gui/mod.rs` (App поля: `sf2_worker_rx:
Option<std::sync::mpsc::Receiver<(Sf2WorkerKind, String, Option<Arc<rustysynth::SoundFont>>)>>`,
`sf2_loading: bool`; init-литерал 737-779), `src/gui/sf2.rs` (parse_sf2 →
статическая fn без self: `fn parse_sf2_file(path: &Path) -> Option<Arc<SoundFont>>`
— тело нынешнего parse_sf2 минус self-статусы), `src/gui/settings.rs`/callers
load_sf2_keys/load_sf2_drums.

- [ ] Step 1: тип `Sf2WorkerKind { Keys, Drums }` (+ путь/имя). spawn в
  load_sf2_keys/load_sf2_drums:
```rust
        // ГВАРД: общий sf2_loading в ОБОИХ фн (review I7) — иначе второй
        // spawn перезапишет rx и in-flight результат потеряется; гвард
        // покрывает и unload-ветку «(none)» (settings.rs 815-821/845-851 —
        // выгрузка при in-flight перезаписалась бы late-apply).
        if self.sf2_loading { return; }
        // keys-cache (sf2.rs:127-131, симметрично drums-кэшу 164-168):
        // короткое замыкание ДО spawn и ДО sf2_loading.
        self.sf2_loading = true;
        let (tx, rx) = std::sync::mpsc::channel();
        let path = path_buf.clone();
        std::thread::spawn(move || {
            let res = std::fs::File::open(&path)
                .map_err(|e| format!("open: {e}"))
                .and_then(|mut f| rustysynth::SoundFont::new(&mut f)
                    .map_err(|e| format!("parse: {e}")))
                .map(std::sync::Arc::new);
            let _ = tx.send((Sf2WorkerKind::Keys, name, res));
        });
        self.sf2_worker_rx = Some(rx);
        // review M9: во время загрузки кадры 4 Гц — ускоряем отклик индикатора
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
```
  (тип канала: `(Sf2WorkerKind, String, Result<Arc<SoundFont>, String>)` —
  review M6: сохраняем диагностику open/parse; drums-ветка: кэш
  sf2_keys_loaded_name==name сохраняется ДО spawn — клон Arc мгновенный,
  worker не нужен; worker только для реального parse.)
- [ ] Step 2: poll в update() — БЕЗУСЛОВНО, в top-level секции (рядом с
  audio-hotplug блоком mod.rs:~860), НЕ рядом с midi_seq_file_pick опросом —
  тот живёт внутри панели MIDI Seq (midi_seq.rs:79-85, dispatch mod.rs:1426)
  и мёртв при закрытой панели → sf2_loading завис навсегда (review I2):
```rust
            if let Some(rx) = &self.sf2_worker_rx {
                match rx.try_recv() {
                    Ok((kind, name, res)) => {
                        self.sf2_worker_rx = None; self.sf2_loading = false;
                        match res {
                            Ok(sf) => /* apply_sf2_result: тело нынешнего post-parse */,
                            Err(e) => self.show_toast(format!("SF2 load failed: {e}"), ToastKind::Error),
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.sf2_worker_rx = None; self.sf2_loading = false;
                        self.show_toast("SF2 loader thread died".into(), ToastKind::Error);
                    }
                }
            }
            while sf2_loading || midi_loading { ctx.request_repaint_after(33ms); break; }
```
  (рефакторинг: post-parse тела load_sf2_keys/load_sf2_drums выделить в
  `apply_sf2_result(&mut self, kind, name, sf: Arc<SoundFont>)` — общий для
  worker-путей; синхронного пути больше нет.
  STARTUP-СЕМАНТИКА (review I3): init-restore из config (mod.rs:1864-1878)
  зовёт load_sf2_keys/drums → теперь ASYNC: окно появляется до загрузки SF2.
  Порядок событий в ринге сохраняется (post-parse пуши per-part SF2
  modes/programs читают ТЕКУЩИЙ self.parts — после performance restore
  1880+; engine-события FIFO) — рассуждение зафиксировать комментарием у
  init-вызова + ЭКРАН-ЧЕК «SF2 из config восстанавливается на старте» в
  Task 3.)
- [ ] Step 3: UI-статус: кнопки загрузки SF2 при `sf2_loading` показывают
  «Loading...» и игнорируют клик (guard в load_sf2_keys/load_sf2_drums).
- [ ] Step 4: тест: App-harness если существует (gui-тесты в crate? если
  App не собирается в тестах — логику poll вынести в fn
  `poll_sf2_worker(&mut self)` и тестовать на уровне «результат применён»
  через подставной receiver; если App-harness невозможен — код-ревью ветки +
  ручная проверка); gates: clippy; cargo test; rt_safety (не задет).
- [ ] Step 5: коммит `fix: SF2 parsing off the GUI thread — worker + mpsc (techdebt GUI I/O)`.

### Task 2: MIDI worker

**Files:** Modify `src/gui/midi_seq.rs` (do_load_midi 281-330), `src/gui/mod.rs`
(`midi_worker_rx: Option<mpsc::Receiver<(String, Result<Box<MidiSeqData>, String)>>>, midi_loading: bool`).

- [ ] Step 1: spawn (fs::read + parse_midi на потоке; Box<MidiSeqData> Send).
  EMPTY-PATH ГВАРД сохраняется ДО spawn (сейчас do_load_midi молча выходит —
  midi_seq.rs:283-285; review M5); тексты ошибок сохранить текущие
  («Error reading file:/Error parsing MIDI:»):
```rust
        let path = self.midi_seq_path.trim().to_string();
        if path.is_empty() { return; }
        if self.midi_loading { return; }
        self.midi_loading = true;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = std::fs::read(&path)
                .map_err(|e| format!("Error reading file: {e}"))
                .and_then(|b| crate::synth::midi_player::parse_midi(&b)
                    .map_err(|e| format!("Error parsing MIDI: {e}"))
                    .map(Box::new));
            let _ = tx.send((path, res));
        });
        self.midi_worker_rx = Some(rx);
```
- [ ] Step 2: poll-блок в ТОМ ЖЕ безусловном top-level месте update() (рядом
  с SF2 poll, review I2): Ok(Box(data)) → СУЩЕСТВУЮЩЕЕ тело Ok-ветки
  do_load_midi (298-316: список треков, тост, push MidiSeqLoad{data: Box})
  выделить в `apply_midi_result(&mut self, data: Box<MidiSeqData>)`; Err →
  toast; Disconnected → toast «loader died». Guard `midi_loading` на
  повторный enter/pick; request_repaint_after(33ms) при loading — общий
  блок со SF2.
- [ ] Step 3: тест (по тем же правилам, что Task 1 Step 4); gates: clippy;
  cargo test; rt_safety.
- [ ] Step 4: коммит `fix: MIDI parse off the GUI thread — worker + mpsc (techdebt GUI I/O)`.

### Task 3: Верификация

- [ ] Step 1: полный гейт: clippy / cargo test / rt_safety / deny / goldens
  (`git status tests/golden` пустой).
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.
- [ ] Step 3: экран-чеклист: загрузка большого SF2 — GUI жив («Loading...»
  индикатор, кадры ≥30fps), по завершении инструмент работает; MIDI-файл
  большой — список треков без фриза; SF2 из config на СТАРТЕ восстанавливается
  (async-семантика, review I3).

## Out of scope
- Persistent worker-поток/пул — штучные spawn достаточны (юзер-paced).
- Отмена загрузки (cancel) — не запрошено.
- RT-аллокации SF2-load — отдельный план (2026-11-08-rt-allocations.md, Out
  of scope там же).
