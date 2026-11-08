# RT-аллокации: drum undo, Twist voice, pool swap-not-clobber Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-08 (HEAD 8411332; якоря: tests/rt_safety.rs FAMILIES и
8 ignored-пинов, src/synth/oscillator.rs:450-477/490-513, src/synth/drum.rs:742/858-907)

**Goal:** убрать аллокации/free на RT-потоке: drum undo/redo Box (×2 сайта),
Twist voice rebuild (пин 29.0), 7 pool-семейств clobber (пины 6/9/10/13/14/15/23).
SF2-load и midi_player.load в колбэке — НЕ здесь (это worker-архитектура,
пункт очереди «GUI-thread I/O»).

**Architecture:**
- Drum undo: `Option<Box<[DrumPattern; MAX]>>` → предвыделенные буферы
  (Box в `new()` на не-RT потоке) + swap-семантика, никаких Box::new на RT.
- Twist: `twist_voice: Option<Box<TwistVoice>>` инициализируется Some в
  `Oscillator::new()` (не-RT); note-on делает `*voice = TwistVoice::new(..)`
  — move-assign В СУЩЕСТВУЮЩИЙ Box = запись без alloc/free.
- Pools: `return_buf` получает «keep-larger + overflow-слот» семантику,
  `take_buf` из пустого слота использует существующую capacity
  (resize без realloc когда capacity≥n). Ничего не освобождается на RT:
  излишки ЖИВУТ в overflow-слоте (память ~28KB/слот ×4 ×16 партов максимум
  ~1.8MB — приемлемо), **расширение Trash под Vec<f32> запрещено**
  (слот-кост 27KB×32 — см. комментарий trash.rs:17-21) — юзер-решение, не здесь.
- Диагностика переборов: TDD через СУЩЕСТВУЮЩИЕ ignored-пины — каждый пин =
  воспроизводящий abort-тест; фиксишь механизм → прогон пина → un-ignore →
  добавить семейство в FAMILIES.

**Tech Stack:** Rust; tests/rt_safety.rs (assert_no_alloc гвард); goldens:
ун-игнор пинов не меняет рендер; ТРЕБУЕТСЯ прогон golden_fx после каждого
pool-фикса (пулы влияют на KS/CP/BWG/Brass/Sax/Bowed/BassGuitar рендер —
параметры идентичны, но порядок переиспользования буферов может изменить
значения, если init полагается на «свежий очищенный» буфер — take_buf уже
делает fill(0.0), содержимое не утечёт).

**Проверенные факты (8411332):**
- tests/rt_safety.rs: FAMILIES (~247-267) без 8 KNOWN-RED; пины `*_note_on_allocates`
  на 304/310/316/322/328/334/340/346 с точными сайтами: общий механизм пулов —
  Voice::init (voice.rs:249+) → Oscillator::reset/init_* → reclaim_buffers
  (oscillator.rs:490) → return_buf (475-477 `*pool = buf` — clobber: падение
  старого Vec если слот занят); take_buf (467-472 `mem::take` + `resize(n,0)`
  + `fill(0)` — из ПУСТОГО слота resize с capacity 0 = alloc!).
- oscillator.rs:450-453: pool_a..d = `Vec::with_capacity(MAX_DELAY)`; MAX_DELAY
  = 7000 f32 = 28000 B (комментарий пинов).
- oscillator.rs:432: `twist_voice: Option<TwistVoice>` (НЕ Box — value),
  TwistVoice::new Box-аллоцирует mi-plaits Voice внутри (пин 346: сайт
  oscillator.rs:2752, struct :64).
- drum.rs:742 `undo_snapshot: Option<Box<[DrumPattern; MAX_PATTERNS]>>`;
  Box::new на 865 (redo) и 896 (snapshot-push); undo 907 возвращает Box.

---

## Test Cases (from the requirement)

- TC-DRUM: guarded snapshot → edit → undo → redo → cancel_redo cycle —
  ни одной alloc/free (cancel_redo сегодня дропает Box на RT — review M9a).
- TC-TWIST: пин twist_note_on_allocates un-ignored → зелёный; 29.0 в FAMILIES.
- TC-POOL×7: каждый из 7 пинов по очереди un-ignored → зелёный; семейство
  возвращено в FAMILIES; golden_fx байт-стабилен ПОСЛЕ КАЖДОГО семейства.
- TC-REUSE: take_buf из занятого-до-этого слота не ре-аллоцирует (buf
  capacity ≥ n → resize без heap op) — покрывается пинами.
- Негатив: overflow-слот занят + третий return → debug_assert + счётчик
  metrics (не println — RT!), буфер с бОльшей capacity остаётся в системе
  (главный/overflow), МЕНЬШИЙ держится в overflow, а вытесненный — ЕСТЬ
  НЕДОПУСТИМ: см. Task 3 Step 1 (инвариант ≤2 на слот доказуем
  чередованием; при нарушении — STOP и доклад юзеру).

## Negative Matrix

- patch load с pool-семейства на непул-семейство: reclaim возвращает, take
  не происходит — излишек живёт в слоте/overflow (не free) ✓.
- Oscillator::drop — только на не-RT пересборке движка (документировано).
- DrumPattern MAX_PATTERNS — const, буферы фиксированного размера.
- Twist: синт параметров между нотами — in-place rebuild покрывает (новый
  Voice::new с новыми параметрами в существующий Box).
- ~~~timeout~~ — n/a; ~~~конкурентность~~ — RT однопоточный.

---

### Task 1: drum undo/redo — предвыделенные буферы + recycle round-trip

**Files:** Modify `src/synth/drum.rs:742, 858-922`; `src/synth/mod.rs`
(ParamFeedback::DrumPatternsSync путь 950-953/1731-1746 + НОВЫЙ
ControlEvent::DrumPatternsSyncReturn хендлер); `src/synth/events.rs`
(новый variant).

- [ ] Step 1: failing-тест (rt-гвард): snapshot → edit → undo → redo →
  cancel_redo (drum.rs:876-879 сегодня дропает redo Box на RT через
  clear_pattern:895/save_snapshot:921) — ни одной alloc/free.
- [ ] Step 2: дизайн (review C2: return-Box load-bearing — летит в GUI через
  ParamFeedback::DrumPatternsSync и освобождается ТАМ; прецедент возврата
  коробок — DrumLoadKit «boxes travel back», mod.rs:1746):
  - поля: `undo_buf`/`redo_buf`/`sync_buf`: `Box<[DrumPattern; MAX_PATTERNS]>`
    (создаются ТОЛЬКО в конструкторе, не-RT) + `undo_used`/`redo_used: bool`;
  - снапшот-сайты 865/896: `*self.undo_buf = self.patterns; self.undo_used = true;`
    (swap-копия массива, без Box::new); cancel_redo 876-879: `redo_used = false`
    (Box не дропается — это и был RT-free);
  - undo()/redo(): `std::mem::swap(&mut self.patterns, &mut *self.undo_buf);`
    затем GUI-синк из ПУЛА: `sync_buf_pool: [Option<Box<[DrumPattern;8]>>; 2]`
    (заполняется в конструкторе); если есть свободный box: `*box = self.patterns;`
    → вернуть `Some(box)` (уходит в GUI); свободного нет → вернуть None +
    metrics-счётчик (синк best-effort, документировано «without allocating»);
  - НОВЫЙ ControlEvent::DrumPatternsSyncReturn(Box<[DrumPattern;8]>):
    GUI, прочитав паттерны, возвращает коробку (`ctrl_tx.push`) → engine-хендлер
    кладёт её в свободный слот sync_buf_pool (store на RT без аллокаций).
  Реализатор сверяет фактические сигнатуры undo/redo (862-917) и ParamFeedback
  путь (950-953, 1731-1742).
- [ ] Step 3: gates: clippy; cargo test (существующие drum undo/redo тесты
  1404-1445 + rt-гвард тест); rt_safety.
- [ ] Step 4: коммит `fix: drum undo/redo pre-allocated buffers + sync-box recycle — no Box::new on RT (audit rhythm debt)`.

### Task 2: Twist — voice allocate-once в Oscillator::new

**Files:** Modify `src/synth/oscillator.rs` (поле 432; new() 436-462;
init_twist ~2860-2871 — сайт ленивой инициализации).

- [ ] Step 1: факт (review C1 — план-версия была регрессией): `TwistVoice` —
  Box-newtype (`struct TwistVoice(Box<plaits Voice>)`, oscillator.rs:64),
  внутри `TwistVoice::new` Box::new. init_twist УЖЕ гвардит
  `if self.twist_voice.is_none()` (2868-2871) — пин ловит ПЕРВУЮ ноту
  (ленивый init внутри guarded_note_cycle). Правильный фикс = создать voice
  в `Oscillator::new` (не-RT) и сделать ленивую ветку defensive.
- [ ] Step 2: реализация: поле остаётся `Option<TwistVoice>` (НЕ Box — M6,
  TwistVoice уже newtype); `Oscillator::new`:
  `twist_voice: Some(TwistVoice::new(/* дефолтные параметры, списать с сайта
  2869-2870 */))`; в init_twist ветку is_none оставить как defensive
  (debug_assert + создание — не должен случаться: у существующего осциллятора
  voice уже есть; НО при смене osc_type reclaim не трогает twist_voice —
  инвариант сохраняется).
- [ ] Step 3: un-ignore пина twist_note_on_allocates; добавить `(29.0, "Twist")`
  в FAMILIES (таблица станет 28 записей — 27.0 Wavetable намеренно исключён,
  покрыт QW-планом, rt_safety.rs:238-239 — review M7).
- [ ] Step 4: gates: clippy; cargo test; rt_safety; golden_fx стабилен
  (Twist рендер не меняется: та же voice, тот же init).
- [ ] Step 5: коммит `fix: Twist voice allocated at oscillator creation — lazy init allocates on first note (audit rhythm debt)`.

### Task 3: pool swap-not-clobber (7 семейств, TDD по пинам)

**Files:** Modify `src/synth/oscillator.rs` (пулы 450-453; take_buf 467-472;
return_buf 475-477; reclaim_buffers 490-513); пины 304-345 по одному.

- [ ] Step 1: инвариант и механизм:
  - поле `pool_overflow: [Option<Vec<f32>>; 4]` (init None в new());
  - `take_buf(pool, overflow, n)`: источник = main если непусто, иначе
    overflow если непусто, иначе pool сам (capacity 0); после take:
    `if buf.capacity() >= n { resize+fill }` — существующий resize УЖЕ не
    ре-аллоцирует при capacity≥n (Vec::resize не трогает heap при len<cap);
    пустой слот → resize = ALLOC (это и ловят пины на «второй» буфер);
  - `return_buf(pool, overflow, buf)`: main пуст → main=buf; main занят →
    сравнить capacity: больший остаётся в main, меньший идёт в overflow;
    overflow занят → СВЕРИТЬ capacity: если меньший из трёх —
    `debug_assert!(false, "pool overflow: 3rd buffer for slot")` + metrics-счётчик
    + вытесненный с меньшей capacity... СТОП-правило: если этот кейс
    реально воспроизводится пином — НЕ изобретать самовольно, ДОЛОЖИТЬ
    (юзер-решение по trash-расширению). Инвариант ≤2/слот ожидаем
    (reclaim/take баланс 1:1 на семейство).
- [ ] Step 2: цикл на КАЖДОЕ семейство (порядок: 6.0 KS → 15.0 Brass →
  23.0 Sax → 13.0 BassGuitar → 14.0 BowedString → 9.0 CommutedPiano →
  10.0 BandedWG — от простого к многобуферному):
  a) un-ignore пина → `cargo test --test rt_safety <pin>` → если GREEN
     (mechanism уже починил) → в FAMILIES; если RED → abort-сообщение
     укажет сайт → применить механизм → GREEN;
  b) `cargo test --test golden_fx` после КАЖДОГО семейства — красный =
     STOP (буферное содержимое утекло: искать fill-логику);
  c) коммит на семейство: `fix: pool <Family> note-on allocation-free (audit S1)`.
- [ ] Step 3: полный rt_safety: 0 ignored осталось из 7 пуловых (twist уже в
  Task 2); FAMILIES полный (29 семейств).
- [ ] Step 4: gates: clippy / cargo test / rt_safety / deny / goldens +
  `git status tests/golden` пустой.
- [ ] Step 5: коммит общий (если останутся незакрытые — список в отчёт
  DONE_WITH_CONCERNS).

### Task 4: Верификация

- [ ] Step 1: полный гейт; suite rt_safety запускать ОБЫЧНО (после un-ignore
  пинов --ignored пуст — review M8).
- [ ] Step 2: PR → squash → rebuild → launch. НЕ ВЫПОЛНЯТЬ до команды юзера.

## Out of scope
- SF2-load / midi_player.load в колбэке — worker-архитектура (пункт 6
  очереди «GUI-thread I/O»).
- Расширение Trash::PoolBuf — слот-кост 27KB×32; юзер-решение.
- Pool capacity-профилирование (реальные n vs MAX_DELAY) — отдельная
  оптимизация памяти.
