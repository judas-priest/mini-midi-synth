# S1: RT-safe `set_sample_rate` (REFACTOR_AUDIT S1, = старый TECHDEBT #12)

anchor-checked: 2026-10-30 — все якоря сняты чтением кода на master @ a4426fc в этой сессии.
Номера строк в REFACTOR_AUDIT.md частично совпадают, но всё ниже перепроверено по текущим исходникам.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Каждый таск исполняется СВЕЖИМ субагентом, получающим только этот документ.

**Goal:** сделать `SynthEngine::set_sample_rate` (и весь путь, достижимый из него, кроме
заранее принятого исключения — см. «Документируем, не меняем») allocation-free, чтобы вызов
в cpal-колбэке (`src/audio.rs:199-205`, реакция на `jack_sr` atomic из error-callback) не
делал ни одной heap-alloc/free. Подход по аудиту (сверка подтвердила): **allocation-free
reset в колбэке** — preallocated-буферы / resize-in-capacity, ноль `*self = Self::new` на
пути из колбэка. Полный rebuild стрима (settings.rs) не трогаем.

## Координация с S0-планом (важно)

`docs/superpowers/plans/2026-10-30-refactor-s0-characterization.md` в Task 2 Step 4 определяет
два known-RED теста с `#[ignore]`:

- `set_sample_rate_is_allocation_free` (S0 TC9)
- `drum_set_sample_rate_is_allocation_free` (S0 TC10)

**Разделение работ:**

- **Если S0 уже выполнен** (тесты с этими именами лежат в `tests/rt_safety.rs` как `#[ignore]`):
  Task 1 этого плана пропускается; в Task 2 / Task 6 снимается `#[ignore]` и тест делается зелёным.
- **Если S0 ещё НЕ выполнен** (текущее состояние master @ a4426fc: в `tests/rt_safety.rs` ровно
  5 тестов, ignored-тестов нет): Task 1 добавляет эти два теста **с точно такими же именами,
  телами и текстами `#[ignore]`, как в S0-плане** — тогда при последующем выполнении S0 его
  шаг «снять ignore после S1» отработает без конфликтов (тесты уже существуют и зелёные;
  исполнитель S0 просто увидит их не-ignored и пропустит дедупликацию).

Проверка состояния: `grep -n "set_sample_rate" tests/rt_safety.rs`.

TC-C (mid-render) — дополнительный тест этого плана, в S0 отсутствует.

## Полная инвентаризация alloc-сайтов цепочки `set_sample_rate`

Цепочка: `audio.rs:204` → `SynthEngine::set_sample_rate` (`src/synth/mod.rs:2647-2690`) →
16× `Part::set_sample_rate` (`mod.rs:976-980`) → 8× `Voice::set_sample_rate` на часть
(`voice.rs:178-191`) + LFO/mseg; `drum_engine.set_sample_rate` (`drum.rs:1054-1090`);
`sampler.set_sample_rate` (`sampler.rs:310-322`); 30 синглтон-FX (`mod.rs:2656-2687`);
`looper.set_sample_rate` (via drum? нет — looper тикается отдельно; его `set_sample_rate`
в цепочку движка не входит, но включён в инвентарь для полноты — фикс не требуется).

### АЛЛОЦИРУЮТ (чиним)

| # | Сайт | Что аллоцирует | Фикс |
|---|------|----------------|------|
| A1 | `drum.rs:1072` `*self = Self::new(sample_rate)` | 3 `Arc<AtomicU8>` (`drum.rs:1009-1011`) + drop старых 3 Arc; плюс временный `StepSequencer::new()` на `drum.rs:1062` — ещё 2 `Arc<AtomicBool>` (`drum.rs:764,766`) + drop | Явные обновления полей, танец удалить |
| A2 | `reverb.rs:298-300` `*self = Self::new(sr)` | 11 `vec!`-аллокаций + 11 free: `pre_delay_buf` (reverb.rs:271), 4× input `DattorroAP` (:262 → :84), `mod_ap_l/r` (:275,:281), `delay_l1/l2/r1/r2` (:276,:279,:282,:285 → DelayLine::new :147) | len-поле + capacity под SR_CAP, resize-in-place |
| A3 | `spring_reverb.rs:84-85` `vec![0.0; MAX_DELAY]` ×2 + free | 2 Vec (16 KB ×2) | `fill(0.0)` — MAX_DELAY константа, SR-независима |
| A4 | `spring_reverb.rs:94-95` → `AllpassFilter::set_len` (`spring_reverb.rs:20-26`) | `buffer.resize(new_len, 0.0)` реаллоцирует все 16 allpass-буферов при росте SR | capacity под SR_CAP при конструировании, `set_len` только clamp+fill |
| A5 | `chorus.rs:73` `self.buffer = vec![0.0; BUFFER_SIZE]` + free | 1 Vec 16 KB; BUFFER_SIZE=4096 константа (chorus.rs:5), SR-независима | `fill(0.0)` |
| A6 | `bbd_ensemble.rs:46` `self.buf = vec![0.0; BUF]` + free | 1 Vec 16 KB; BUF=4096 константа (bbd_ensemble.rs:9) | `fill(0.0)` |
| A7 | `voice.rs:178-191` пересборка `self.oscs = [Oscillator::new(sr); 3]` | На каждый из 128 голосов (16 частей × `VOICES_PER_LAYER=8`, mod.rs:381,528): 3× `Arc::new(Vec::new())` (oscillator.rs:641) + **drop старых OscState** → free pooled delay-line буферов (`pool_a..d`). Итого 384 Arc alloc/free + free пулов | In-place: `osc.sample_rate = sr`, OscState не трогаем (note_on полностью реинициализирует все 3 осциллятора — voice.rs:245, 364, 382 `reset()` + per-type `init_*`) |

### УЖЕ ALLOC-FREE (не трогаем; проверено чтением тел set_sample_rate)

`eq.rs:89` (dirty-флаг), `compressor.rs:50`, `overdrive.rs:75`, `tape.rs:38`, `neuron.rs:34`
(массивы-константы), `phaser.rs:36`, `flanger.rs:38`, `tremolo.rs:22`, `bitcrusher.rs:28`,
`ring_mod.rs:16`, `freq_shift.rs:78` (массивы), `rotary.rs:44` (`fill` фикс-буферов),
`resonator.rs:45` (`fill`), `delay.rs:35` (`fill`), `wave_shaper.rs:58`, `ms_tool.rs:17`,
`graphic_eq.rs:96` (`rebuild_all` — plain-структуры), `conditioner.rs:45`, `exciter.rs:54`,
`floaty_delay.rs:41` (`fill`), `reverb2.rs:61` (`fill` фикс-массивов), `combulator.rs:90`,
`treemonster.rs:39`, `nimbus.rs:73` (`fill` + сброс grain-флагов), `vocoder.rs:135`,
`convolution_reverb.rs:68` (`generate_ir` — стековый `[f32; 512]` + preallocated `ir`/`damped_ir`),
`airwindows.rs:28,81,124` (plain) и `airwindows.rs:2639-2644` (делегирование в 3 plain-подэффекта),
`lfo.rs:203`, `mseg.rs:123`, `mod.rs:976-980` (после фикса A7 — alloc-free).

`looper.rs:206-209` — alloc-free (`clear()` = сброс полей, looper.rs:310-323), **но семантически
стирает содержимое лупера** — pre-existing поведение («буферы не той высоты после смены SR»),
вне скоупа.

### Документируем, НЕ меняем (принятое поведение)

- `sampler.rs:310-322`: `Sampler::set_sample_rate` при загруженном SF2 пересобирает
  `Synthesizer::new` в колбэке (аллокации), старый heap уходит в GUI через `Displaced` →
  `trash.rs` (`mod.rs:2653-2655` `dispose_sf2`). Это **существующий принятый паттерн**
  (аудит: «in-callback rebuild с отложенным free»), полный вынос — отдельная работа
  (TECHDEBT: `Synthesizer::new` при загрузке SF2). RT-тесты этого плана используют движок
  без SF2 — `soundfont: None` → clone без аллокации → rebuild не выполняется.
- `audio.rs:159` — вызов `set_sample_rate` ДО build_output_stream (control-контекст,
  аллокации допустимы). Не трогаем: после фикса он просто становится бесплатным.

## Контракт / ограничения

1. **Поведение звука сохраняется.** Для каждого FX новая реализация `set_sample_rate`
   эквивалентна старой («пересоздать с новым SR»): буферы обнулены, SR-зависимые длины
   и коэффициенты пересчитаны, транзиент-состояния сброшены. Тапы/длины считаются от
   нового SR — тайминги bit-идентичны старым (active `len` = тот же `scale_len`).
2. **`SR_CAP = 192_000.0`.** Все буферы, длина которых зависит от SR, аллоцируются один
   раз на capacity для 192 kHz. Это максимум, который вообще может прийти из колбэка:
   `audio.rs:57` negotiate работает только по списку `[22050, 44100, 48000, 88200, 96000,
   176400, 192000]`. Если SR > SR_CAP всё же пришёл: `debug_assert!` падает в debug,
   в release `set_len` молча clamp-ит по capacity (короче хвост реверберации, но без UB/panic).
3. **Drum: никаких сохранений/восстановлений.** Явная версия обновляет ТОЛЬКО
   SR-зависимое: поля голосов + транзиент click-состояние + `group_last_note` (panic-path
   правило). Всё остальное (params, sequencer, атомы, volume, enabled, sf2_mode,
   fixed_velocity, metronome, remap, pad_vel, choke_groups) вообще не читается/не пишется —
   класс багов «новое поле молча сбросилось» исчезает по построению.
4. **Voice: состояние осцилляторов переживает смену SR.** Голоса деактивируются
   (`active = false`); следующий `note_on` вызывает `reset()` на всех трёх осцилляторах
   (voice.rs:245, 364, 382) и per-type `init_*` (voice.rs:248-320+) — все SR-зависимые
   буферы/коэффициенты пересчитываются там. Между сменой SR и следующим note_on
   осцилляторы не звучат.

## Test Cases (требования → тесты)

Все rt-тесты в `tests/rt_safety.rs` (feature `rt-assert`, глобальный аллокатор в lib).
Функциональные — в новом `tests/sr_change.rs` (без feature-gate).

| TC | Тест | Требование | Изначально |
|----|------|-----------|------------|
| TC-A | `set_sample_rate_is_allocation_free` | `engine.set_sample_rate(96_000)` под `assert_no_alloc` (движок по умолчанию, без SF2) — ноль alloc/free | KNOWN-RED (abort) |
| TC-B | `drum_set_sample_rate_is_allocation_free` | `engine.drum_engine.set_sample_rate(96_000)` под guard | KNOWN-RED (abort) |
| TC-C | `sr_change_mid_render_does_not_allocate` | JACK-сценарий: NoteOn активен, один `tick_block`, затем в guard: `set_sample_rate(96_000)` + `tick_block` | KNOWN-RED (abort) |
| TC-D | `engine_sr_change_then_note_on_stays_sane` | после `set_sample_rate(44_100 → 96_000)` новый note_on рендерит конечно (no NaN), ненулевой RMS за 64 блока | NEW, зелёный после фикса |
| TC-E | `reverb_resizes_in_place_stays_sane` | `Reverb::new(44_100)` → tick → `set_sample_rate(192_000)` → tick 1000 сэмплов: finite, wet-энергия ненулевая на импульсе | NEW |
| TC-F | `spring_and_chorus_resize_stays_sane` | `SpringReverb::new(44_100)`/`Chorus::new(44_100)` → `set_sample_rate(192_000)` → tick: finite | NEW |
| TC-G | `drum_atoms_survive_sr_change` | `Arc::ptr_eq(step_atom_before, step_atom_after)` (и play/rec) — атомы не пересоздаются | NEW |
| (есть) | `tests/drum_remap.rs:90-141` | remap/pad_vel/choke/sf2_mode/fixed_velocity/volume переживают SR-смену ×2; choke-холдеры сбрасываются | должны остаться зелёными |

### Negative matrix

| Класс | Покрытие |
|-------|----------|
| SR вверх (48k→96k, 44.1k→192k) | TC-A/D/E/F, `drum_remap.rs:115-116` (двойная смена) |
| SR вниз (96k→44.1k) | вниз покрывает TC-D (48k→44.1k в теле теста) + drum_remap.rs:101 |
| Двойной/повторный вызов (duplicate) | `drum_remap.rs:107-117` (дважды), TC-A вызвать дважды подряд в guard |
| Same-SR no-op | `audio.rs:202` гейтится `new_sr != synth_sr` — движок не зовётся; сам `set_sample_rate` при том же SR должен быть безвреден: TC-D дважды |
| Zero/garbage SR | `debug_assert!(sample_rate > 0.0)` в Reverb/Spring; из колбэка ноль невозможен (cpal negotiated), guard-тест руками не гоняется |
| Новое поле DrumEngine (regression класс) | устраняется самим фиксом (поля не перечисляются); TC-G + drum_remap пиняют сохранение |
| SR > SR_CAP | debug_assert; release-clamp покрыт тем, что `set_len` `.min(buffer.len())` — юнит-тест не нужен (недостижимо из negotiate) |
| SF2 загружен | вне скоупа (документировано выше); TC-A прогоняется на движке без SF2 по построению |
| Clock skew | N/A — один источник SR (`jack_sr` atomic), сравнение с локальным `synth_sr` в audio.rs:202 |

---

## Task 1: добавить known-RED rt-тесты (pinned `#[ignore]`)

Файл: `/home/dima/Projects/mini_midi_synth/tests/rt_safety.rs`

**Pre-check:** если тесты с этими именами уже есть (S0 выполнен) — пропустить Task 1 целиком.

- [ ] Добавить в конец файла (после `switching_between_wavetable_patches_does_not_free_in_rt`):

```rust
/// KNOWN RED (REFACTOR_AUDIT S1): set_sample_rate runs in the audio callback
/// (audio.rs:199-205) on JACK rate change, but reallocates ~30 FX
/// (reverb.rs:298-300 `*self = Self::new`, chorus.rs:73, spring_reverb.rs:82-85,
/// bbd_ensemble.rs:46) and drum/voice state (drum.rs:1054-1090, voice.rs:178-191).
/// Un-ignore after the S1 fix makes it allocation-free.
#[ignore = "KNOWN RED (audit S1): set_sample_rate reallocates FX in the callback"]
#[test]
fn set_sample_rate_is_allocation_free() {
    let mut engine = SynthEngine::new(SR);
    assert_no_alloc(|| {
        engine.set_sample_rate(96_000.0);
        engine.set_sample_rate(96_000.0); // duplicate call must also be free
    });
}

/// KNOWN RED (audit S1): DrumEngine::set_sample_rate does
/// `*self = Self::new(sr)` (drum.rs:1054-1090) — 3 Arc atoms (drum.rs:1009-1011)
/// plus a temporary StepSequencer::new (2 more Arcs, drum.rs:764-766) allocated
/// and dropped in the callback. Un-ignore after the S1 fix.
#[ignore = "KNOWN RED (audit S1): drum set_sample_rate rebuilds via Self::new (3 Arc allocs, drum.rs:1009-1011)"]
#[test]
fn drum_set_sample_rate_is_allocation_free() {
    let mut engine = SynthEngine::new(SR);
    assert_no_alloc(|| engine.drum_engine.set_sample_rate(96_000.0));
}

/// JACK mid-callback scenario (this plan's TC-C): a note is held, one block was
/// rendered, then JACK reports a new rate and the callback applies it before the
/// next block. KNOWN RED until S1 lands.
#[ignore = "KNOWN RED (audit S1): mid-render SR change reallocates FX/voices"]
#[test]
fn sr_change_mid_render_does_not_allocate() {
    let mut engine = SynthEngine::new(SR);
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    engine.tick_block(&mut l, &mut r);
    assert_no_alloc(|| {
        engine.set_sample_rate(96_000.0);
        engine.tick_block(&mut l, &mut r);
    });
}
```

(`MidiEvent`, `BLOCK_SIZE` уже в use-строке файла; `SynthEngine`, `assert_no_alloc` тоже.)

- [ ] Подтвердить RED (по одному — abort убивает весь процесс):

```bash
cargo test --features rt-assert --test rt_safety drum_set_sample_rate_is_allocation_free -- --ignored
# EXPECT: process abort ("memory allocation of N bytes failed")
cargo test --features rt-assert --test rt_safety set_sample_rate_is_allocation_free -- --ignored
cargo test --features rt-assert --test rt_safety sr_change_mid_render_does_not_allocate -- --ignored
```

- [ ] Обычный сьют зелёный (ignored не считаются):

```bash
cargo test --features rt-assert --test rt_safety
# EXPECT: зелёные + ignored-хвост (счёт относительный — зависит от порядка с S0)
```

- [ ] Commit:

```bash
git add tests/rt_safety.rs
git commit -m "test: pin known-RED set_sample_rate allocs as ignored (audit S1)"
```

---

## Task 2: drum.rs — явный set_sample_rate вместо танца save/restore

Файл: `/home/dima/Projects/mini_midi_synth/src/synth/drum.rs`

**Якорь:** `DrumEngine::set_sample_rate` = `drum.rs:1054-1090`; `DrumEngine::new` = `drum.rs:966-1013`
(3 Arc на :1009-1011); `StepSequencer::new` = `drum.rs:753-768` (2 Arc на :764,766);
`DrumVoice` enum = `drum.rs:633-641`; голоса-структуры `Clone, Copy` с полем `sample_rate`
(KickVoice :93-117, SnareVoice :176+, ClapVoice ~:348, TomVoice ~:439, MetalVoice ~:518,
ClickVoice :561-585) — присваивание `*v = XVoice::new(sr)` это plain-копия стека, БЕЗ heap.

- [ ] Заменить тело `DrumEngine::set_sample_rate` (строки 1054-1090) целиком:

```rust
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        // Called from the audio callback on JACK rate change — must be
        // allocation-free. Explicit updates only: no `*self = Self::new`
        // dance (it allocated 5 Arcs and silently reset any field the
        // save/restore list forgot). User state (params, sequencer, atoms,
        // remap, pad_vel, choke_groups, volume/enabled/sf2_mode/
        // fixed_velocity/metronome) is not touched at all.
        self.sample_rate = sample_rate;
        for voice in &mut self.voices {
            // Plain Copy-struct rebuild: fresh, inactive voice at the new SR.
            voice.set_sample_rate(sample_rate);
        }
        // Click state is transient (<30 ms) and SR-dependent — drop it.
        self.click_phase = 0.0;
        self.click_phase_inc = 0.0;
        self.click_env = 0.0;
        self.click_decay = 0.0;
        // Stale choke holders are reset (panic-path rule, was part of the dance).
        self.group_last_note = [0u8; NUM_CHOKE_GROUPS];
    }
```

- [ ] Добавить метод в `impl DrumVoice` (рядом с `kill_fade`, drum.rs:656-665):

```rust
    fn set_sample_rate(&mut self, sr: f32) {
        // Voice structs are plain Copy — rebuild is a stack assignment, no heap.
        match self {
            Self::Kick(v) => *v = KickVoice::new(sr),
            Self::Click(v) => *v = ClickVoice::new(sr),
            Self::Snare(v) => *v = SnareVoice::new(sr),
            Self::Clap(v) => *v = ClapVoice::new(sr),
            Self::Tom(v) => *v = TomVoice::new(sr),
            Self::Metal(v) => *v = MetalVoice::new(sr),
        }
    }
```

- [ ] Проверка:

```bash
cargo test --features rt-assert --test rt_safety drum_set_sample_rate_is_allocation_free -- --ignored --include-ignored
# EXPECT: ok (no abort)
cargo test drum_remap
# EXPECT: все зелёные (remap/pad_vel/choke/двойная смена/сброс choke-холдеров)
```

- [ ] Снять `#[ignore]` + KNOWN-RED doc-комментарий с
      `drum_set_sample_rate_is_allocation_free` (tests/rt_safety.rs), оставить тест.
- [ ] Commit:

```bash
git add src/synth/drum.rs tests/rt_safety.rs
git commit -m "fix: drum set_sample_rate — explicit SR-dependent field updates, no *self = Self::new (audit S1)"
```

---

## Task 3: chorus + bbd_ensemble — fill вместо vec! (константные буферы)

Оба буфера имеют SR-независимую константную длину — реаллокация там была чистым мусором.

- [ ] `/home/dima/Projects/mini_midi_synth/src/synth/chorus.rs:71-77`:

```rust
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        // BUFFER_SIZE is SR-independent — zero in place, no realloc (RT-safe).
        self.buffer.fill(0.0);
        self.write_pos = 0;
        self.bbd_lp_l = 0.0;
        self.bbd_lp_r = 0.0;
    }
```

- [ ] `/home/dima/Projects/mini_midi_synth/src/synth/bbd_ensemble.rs:44-51`:

```rust
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        // BUF is SR-independent — zero in place, no realloc (RT-safe).
        self.buf.fill(0.0);
        self.write = 0;
        self.input_lp = 0.0;
        self.lp_l = [0.0; 3];
        self.lp_r = [0.0; 3];
    }
```

- [ ] Проверка: `cargo test` (бит-идентичное поведение — существующих тестов достаточно).
- [ ] Commit:

```bash
git add src/synth/chorus.rs src/synth/bbd_ensemble.rs
git commit -m "fix: chorus/bbd set_sample_rate zero buffers in place (audit S1)"
```

---

## Task 4: spring_reverb — capacity под SR_CAP, set_len без resize

Файл: `/home/dima/Projects/mini_midi_synth/src/synth/spring_reverb.rs`

**Якоря:** `MAX_DELAY = 8192` (:6), `AllpassFilter` (:9-36) — уже имеет отдельное поле
`len`, но `set_len` (:20-26) зовёт `buffer.resize` (realloc при росте);
`SpringReverb::new` (:59-80) — allpass-буферы под текущий SR; `set_sample_rate` (:82-98).

- [ ] Добавить константу рядом с `MAX_DELAY` (:6):

```rust
/// Ceiling sample rate the allpass buffers are sized for at construction.
/// audio.rs negotiates at most 192 kHz (audio.rs:57), so set_sample_rate never
/// needs to grow a buffer — resize would be a heap realloc in the callback.
const SR_CAP: f32 = 192_000.0;
```

- [ ] Заменить `AllpassFilter::new` + `set_len` (:16-26):

```rust
    fn new(cap: usize, len: usize) -> Self {
        let len = len.max(1).min(cap.max(1));
        Self { buffer: vec![0.0; cap.max(1)], pos: 0, len }
    }

    /// RT-safe resize: clamps to the preallocated capacity and zeroes the
    /// buffer. Never reallocates (buffers are sized for SR_CAP at construction).
    fn set_len(&mut self, new_len: usize) {
        let new_len = new_len.max(1).min(self.buffer.len());
        self.buffer.fill(0.0);
        self.len = new_len;
        self.pos = 0;
    }
```

(`process` (:29-35) и `interp_read` используют `self.len`/`buf.len()` соответственно — без изменений.)

- [ ] `SpringReverb::new` — цикл инициализации allpass (:73-78):

```rust
        let ap_delays = [139, 193, 263, 349, 421, 509, 601, 701];
        for &d in &ap_delays {
            let scaled = (d as f32 * sample_rate / 48000.0) as usize;
            let cap = (d as f32 * SR_CAP / 48000.0) as usize;
            s.allpasses_l.push(AllpassFilter::new(cap, scaled));
            s.allpasses_r.push(AllpassFilter::new(cap, scaled));
        }
```

- [ ] Заменить `set_sample_rate` (:82-98):

```rust
    pub fn set_sample_rate(&mut self, sr: f32) {
        debug_assert!(
            sr <= SR_CAP + 0.5,
            "SR above SR_CAP would clamp allpass lengths (buffers sized once for SR_CAP)"
        );
        self.sample_rate = sr;
        // MAX_DELAY is SR-independent — zero in place, no realloc (RT-safe).
        self.delay_l.fill(0.0);
        self.delay_r.fill(0.0);
        self.delay_pos = 0;
        self.damp_l = 0.0; self.damp_r = 0.0;
        self.fb_l = 0.0; self.fb_r = 0.0;

        let ap_delays = [139, 193, 263, 349, 421, 509, 601, 701];
        for (i, &d) in ap_delays.iter().enumerate() {
            let scaled = (d as f32 * sr / 48000.0) as usize;
            self.allpasses_l[i].set_len(scaled);
            self.allpasses_r[i].set_len(scaled);
        }
    }
```

- [ ] Проверка: `cargo test` зелёный; существующие spring-тесты (если есть в suites) зелёные.
- [ ] Commit:

```bash
git add src/synth/spring_reverb.rs
git commit -m "fix: spring_reverb set_sample_rate — preallocate allpass capacity for SR_CAP, resize in place (audit S1)"
```

---

## Task 5: reverb (Dattorro) — len-поле, пересоздание → in-place resize

Файл: `/home/dima/Projects/mini_midi_synth/src/synth/reverb.rs`

**Якоря:** `REFERENCE_SR` :19; `scale_len` :66-68; `DattorroAP` :74-132 (`new` :82-88, `tick` :91,
`tick_mod` :105-123 использует `buffer.len()` дважды, `read_at` :127-131); `DelayLine` :139-175
(`new` :145-150, `push` :153-159, `read_at` :163-167, `oldest` :172-174); `DattorroTaps::new` :202-219;
`Reverb` struct :227-257; `Reverb::new` :260-296 (11 vec!-сайтов: :271 и через ::new-хелперы);
`set_sample_rate` :298-300 (`*self = Self::new`); `tick` :305-419 (`pre_delay_buf.len()` :323-324,
:332).

Дизайн: у `DattorroAP`/`DelayLine` появляется `len` (active) при `buffer` = capacity под
`SR_CAP`; весь tick-код читает `len`, не `buffer.len()`. `pre_delay_buf` аналогично — поле
`pre_delay_len`. Семантика after-fix bit-идентична старой (`*self = Self::new(sr)`:
обнулённые буферы, длины/тапы от нового SR, сброшенные состояния).

- [ ] Добавить константу рядом с `REFERENCE_SR` (:19):

```rust
/// Ceiling sample rate the reverb buffers are sized for at construction.
/// audio.rs negotiates at most 192 kHz (audio.rs:57), so set_sample_rate only
/// rescales active lengths inside preallocated capacity — never reallocates.
/// Memory cost vs old code at 48 kHz: ~460 KB instead of ~75 KB, allocated once.
const SR_CAP: f32 = 192_000.0;
```

- [ ] `DattorroAP` — новая структура и методы:

```rust
#[derive(Clone)]
struct DattorroAP {
    buffer: Vec<f32>, // capacity, sized once for SR_CAP
    len: usize,       // active length (<= buffer.len()); all wrap math uses this
    pos: usize,
    coeff: f32,
}

impl DattorroAP {
    fn new(cap: usize, len: usize, coeff: f32) -> Self {
        let len = len.max(1).min(cap.max(1));
        Self { buffer: vec![0.0; cap.max(1)], len, pos: 0, coeff }
    }

    /// RT-safe resize: clamp to capacity, zero, reset position. No realloc.
    fn set_len(&mut self, len: usize) {
        let len = len.max(1).min(self.buffer.len());
        self.buffer.fill(0.0);
        self.len = len;
        self.pos = 0;
    }

    #[inline]
    fn tick(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.pos];
        let v = input - self.coeff * delayed;
        self.buffer[self.pos] = v;
        self.pos += 1;
        if self.pos >= self.len {
            self.pos = 0;
        }
        delayed + self.coeff * v
    }

    /// Modulated tick: reads from a fractionally-offset position using linear
    /// interpolation, then writes at the current position.
    #[inline]
    fn tick_mod(&mut self, input: f32, lfo_offset: f32) -> f32 {
        let len = self.len;
        // Nominal read position (where we'd normally read)
        // Read from `len` samples back, offset by LFO modulation
        let read_f = self.pos as f32 + (len as f32) - lfo_offset;
        let read_floor = read_f as usize % len;
        let frac = read_f - read_f.floor();
        let a = self.buffer[read_floor];
        let b = self.buffer[(read_floor + 1) % len];
        let delayed = a + frac * (b - a);

        let v = input - self.coeff * delayed;
        self.buffer[self.pos] = v;
        self.pos += 1;
        if self.pos >= len {
            self.pos = 0;
        }
        delayed + self.coeff * v
    }

    /// Read from the buffer at `offset` samples behind the write head.
    #[inline]
    fn read_at(&self, offset: usize) -> f32 {
        let len = self.len;
        let idx = (self.pos + len - offset.min(len)) % len;
        self.buffer[idx]
    }
}
```

- [ ] `DelayLine` — аналогично:

```rust
#[derive(Clone)]
struct DelayLine {
    buffer: Vec<f32>, // capacity, sized once for SR_CAP
    len: usize,       // active length (<= buffer.len())
    pos: usize,
}

impl DelayLine {
    fn new(cap: usize, len: usize) -> Self {
        let len = len.max(1).min(cap.max(1));
        Self { buffer: vec![0.0; cap.max(1)], len, pos: 0 }
    }

    /// RT-safe resize: clamp to capacity, zero, reset position. No realloc.
    fn set_len(&mut self, len: usize) {
        let len = len.max(1).min(self.buffer.len());
        self.buffer.fill(0.0);
        self.len = len;
        self.pos = 0;
    }

    #[inline]
    fn push(&mut self, sample: f32) {
        self.buffer[self.pos] = sample;
        self.pos += 1;
        if self.pos >= self.len {
            self.pos = 0;
        }
    }

    /// Read the sample that is `offset` samples behind the write head.
    #[inline]
    fn read_at(&self, offset: usize) -> f32 {
        let len = self.len;
        let idx = (self.pos + len - offset.min(len)) % len;
        self.buffer[idx]
    }

    /// Read the oldest sample in the buffer (at current write position, about to be overwritten).
    /// This is the sample that has traveled through the entire delay line.
    #[inline]
    fn oldest(&self) -> f32 {
        self.buffer[self.pos]
    }
}
```

- [ ] `Reverb` struct (:227-257): добавить поле `pre_delay_len: usize` после `pre_delay_buf`.

- [ ] `Reverb::new` (:260-296) — все длины в двух вариантах (cap для SR_CAP, active для sr):

```rust
    pub fn new(sample_rate: f32) -> Self {
        debug_assert!(sample_rate <= SR_CAP + 0.5, "SR above SR_CAP not supported by preallocated buffers");
        let cap = |base: usize| scale_len(base, SR_CAP);
        let len = |base: usize| scale_len(base, sample_rate);
        let input_ap = std::array::from_fn(|i| {
            DattorroAP::new(cap(INPUT_AP_DELAYS[i]), len(INPUT_AP_DELAYS[i]), INPUT_AP_COEFFS[i])
        });

        Self {
            sample_rate,
            bw_state: 0.0,
            // Max pre-delay ~100ms at SR_CAP
            pre_delay_buf: vec![0.0; (SR_CAP * 0.1) as usize + 1],
            pre_delay_len: (sample_rate * 0.1) as usize + 1,
            pre_delay_pos: 0,
            input_ap,
            // Left tank
            mod_ap_l: DattorroAP::new(cap(MOD_AP_L_DELAY), len(MOD_AP_L_DELAY), MOD_AP_COEFF),
            delay_l1: DelayLine::new(cap(DELAY_L1_LEN), len(DELAY_L1_LEN)),
            damp_state_l: 0.0,
            ap_l2: DattorroAP::new(cap(AP_L2_DELAY), len(AP_L2_DELAY), TANK_AP2_COEFF),
            delay_l2: DelayLine::new(cap(DELAY_L2_LEN), len(DELAY_L2_LEN)),
            // Right tank
            mod_ap_r: DattorroAP::new(cap(MOD_AP_R_DELAY), len(MOD_AP_R_DELAY), MOD_AP_COEFF),
            delay_r1: DelayLine::new(cap(DELAY_R1_LEN), len(DELAY_R1_LEN)),
            damp_state_r: 0.0,
            ap_r2: DattorroAP::new(cap(AP_R2_DELAY), len(AP_R2_DELAY), TANK_AP2_COEFF),
            delay_r2: DelayLine::new(cap(DELAY_R2_LEN), len(DELAY_R2_LEN)),
            // Feedback
            left_out: 0.0,
            right_out: 0.0,
            // LFO
            lfo_phase_l: 0.0,
            lfo_phase_r: 0.25, // start 90° offset
            lfo_excursion: LFO_EXCURSION_BASE * sample_rate / REFERENCE_SR,
            // Taps
            taps: DattorroTaps::new(sample_rate),
        }
    }
```

- [ ] `set_sample_rate` (:298-300) — полная замена:

```rust
    pub fn set_sample_rate(&mut self, sr: f32) {
        // RT-safe (called from the audio callback): rescales active lengths
        // inside buffers preallocated for SR_CAP. Equivalent to the old
        // `*self = Self::new(sr)` — zeroed buffers, recomputed taps, reset state.
        debug_assert!(sr <= SR_CAP + 0.5, "SR above SR_CAP would clamp reverb lengths");
        self.sample_rate = sr;
        let len = |base: usize| scale_len(base, sr);
        for (i, ap) in self.input_ap.iter_mut().enumerate() {
            ap.set_len(len(INPUT_AP_DELAYS[i]));
        }
        self.mod_ap_l.set_len(len(MOD_AP_L_DELAY));
        self.delay_l1.set_len(len(DELAY_L1_LEN));
        self.ap_l2.set_len(len(AP_L2_DELAY));
        self.delay_l2.set_len(len(DELAY_L2_LEN));
        self.mod_ap_r.set_len(len(MOD_AP_R_DELAY));
        self.delay_r1.set_len(len(DELAY_R1_LEN));
        self.ap_r2.set_len(len(AP_R2_DELAY));
        self.delay_r2.set_len(len(DELAY_R2_LEN));
        self.pre_delay_buf.fill(0.0);
        self.pre_delay_len = (sr * 0.1) as usize + 1;
        self.pre_delay_pos = 0;
        self.bw_state = 0.0;
        self.damp_state_l = 0.0;
        self.damp_state_r = 0.0;
        self.left_out = 0.0;
        self.right_out = 0.0;
        self.lfo_phase_l = 0.0;
        self.lfo_phase_r = 0.25;
        self.lfo_excursion = LFO_EXCURSION_BASE * sr / REFERENCE_SR;
        self.taps = DattorroTaps::new(sr);
    }
```

- [ ] `tick` (:321-334): заменить `self.pre_delay_buf.len()` на `self.pre_delay_len`
      (два места: :323 `.min(...)` и :324 `let pd_len`):

```rust
        // --- Pre-delay ---
        let pd_samples = ((pre_delay * self.sample_rate) as usize)
            .min(self.pre_delay_len - 1);
        let pd_len = self.pre_delay_len;
```

Остальной `tick` не меняется (уже использует `tick`/`tick_mod`/`read_at`/`push`/`oldest`).

- [ ] Единственный внешний конструктор — `mod.rs:1443` `reverb: Reverb::new(sample_rate)` —
      сигнатура `new` не меняется, правок не требует. Проверить `grep -rn "Reverb::new" src/ tests/`.
- [ ] Проверка: `cargo test` зелёный.
- [ ] Commit:

```bash
git add src/synth/reverb.rs
git commit -m "fix: Dattorro reverb set_sample_rate — len-field + SR_CAP capacity, resize in place (audit S1)"
```

---

### Task 6-pre: `Filter::reset_for_sr(&mut self, sr: f32)` (Filter владеет heap comb_buf)

`Filter` владеет `comb_buf: Box<[f32; 512]>` (filter.rs:162, аллокация в
`Filter::new` :234) — каждый `Filter::new` = 1 alloc + 1 free; ×2 фильтра ×
128 голосов = 256 alloc внутри guard. Фикс: добавить в `impl Filter` метод,
копирующий ВСЕ присваивания тела `Filter::new` (прочитать filter.rs:206-240:
cutoff=8000, dirty=true, comb_idx/comb_state и прочие состояния, sample_rate)
на существующие поля — БЕЗ замены comb_buf и любых Vec/Box. Контракт: голос
после SR-сброса имеет свежий дефолт-фильтр, идентичный `Filter::new`. Если
comb_buf содержимое влияет на звук до dirty-пересчёта — обнулить нулями
на месте (write-in-place, без realloc). В сниппете Task 6 заменить
`self.filter = Filter::new(sr)` / `self.filter2 = ...` на
`self.filter.reset_for_sr(sr)` / `self.filter2.reset_for_sr(sr)`.

## Task 6: voice.rs — in-place SR update, без Oscillator::new

Файл: `/home/dima/Projects/mini_midi_synth/src/synth/voice.rs`

**Якоря:** `Voice::set_sample_rate` :178-191; `Oscillator.sample_rate` — `pub` поле
(oscillator.rs:357); `Oscillator::new` Wavetable-ветка аллоцирует `Arc::new(Vec::new())`
(oscillator.rs:641); note_on реинициализирует ВСЕ 3 осциллятора: `oscs[0].reset()` :245,
`oscs[1].reset()` :364, `oscs[2].reset()` :382 + per-type `init_*` :248-320+.
`Filter::new` (filter.rs:206), `Envelope::new` (envelope.rs:76), `FormantFilter::new`
(formant.rs:172) — plain-структуры, alloc-free.

- [ ] Заменить тело `set_sample_rate` (:178-191):

```rust
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        // RT-safe (called from the audio callback on JACK rate change): must
        // NOT rebuild oscillators — Oscillator::new heap-allocates a Wavetable
        // Arc (oscillator.rs `Arc::new(Vec::new())`) and dropping the old
        // states would free the pooled delay-line buffers here. Every note_on
        // fully re-initializes all three oscillators (reset() + per-type
        // init_*), so updating the SR field in place is sufficient; voices are
        // deactivated so nothing stale is rendered.
        self.sample_rate = sample_rate;
        for osc in &mut self.oscs {
            osc.sample_rate = sample_rate;
        }
        // Plain structs — assignments, no heap.
        self.filter = Filter::new(sample_rate);
        self.filter2 = Filter::new(sample_rate);
        self.formant_filter = FormantFilter::new(sample_rate);
        self.amp_env = Envelope::new(sample_rate);
        self.filter_env = Envelope::new(sample_rate);
        self.active = false;
    }
```

- [ ] Снять `#[ignore]` + KNOWN-RED doc-комментарий с `set_sample_rate_is_allocation_free`
      и `sr_change_mid_render_does_not_allocate` (tests/rt_safety.rs).
- [ ] Проверка RED→GREEN:

```bash
cargo test --features rt-assert --test rt_safety set_sample_rate_is_allocation_free -- --include-ignored
cargo test --features rt-assert --test rt_safety sr_change_mid_render_does_not_allocate -- --include-ignored
# EXPECT: оба ok. Если abort остался — прогнать под rust-gdb / RUST_BACKTRACE и найти
# остаточный alloc-сайт (кандидаты: пропущенный FX из инвентаря, sampler с SF2 — нет,
# движок дефолтный). Не глушить assert_no_alloc, чинить сайт.
cargo test --features rt-assert --test rt_safety
# EXPECT: все зелёные, 0 ignored (счёт rt_safety ОТНОСИТЕЛЬНЫЙ — зависит от порядка с S0)
```

- [ ] Sanity-прогон вниз/вверх руками (доп. к TC-A, в guard оба направления — временно
      поменять 96_000.0 на 44_100.0, прогнать, вернуть; в коммит не входит):

```bash
# после ручной правки:
cargo test --features rt-assert --test rt_safety set_sample_rate_is_allocation_free -- --include-ignored
```

- [ ] Commit:

```bash
git add src/synth/voice.rs tests/rt_safety.rs
git commit -m "fix: Voice::set_sample_rate updates SR in place, keeps osc state (384 Arc allocs/frees removed from callback, audit S1)"
```

---

## Task 7: функциональные тесты + полные гейты + docs

### 7a. Новый файл `tests/sr_change.rs`

```rust
//! Behavior contracts of set_sample_rate after the audit-S1 RT-safety refactor:
//! SR-dependent buffers resize in place, user state and atoms survive, and the
//! engine keeps rendering sane audio after a rate change.

use mini_midi_synth::synth::chorus::Chorus;
use mini_midi_synth::synth::reverb::Reverb;
use mini_midi_synth::synth::spring_reverb::SpringReverb;
use mini_midi_synth::synth::{MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;

fn rms(buf: &[f32]) -> f32 {
    (buf.iter().map(|s| s * s).sum::<f32>() / buf.len().max(1) as f32).sqrt()
}

fn finite(buf: &[f32]) -> bool {
    buf.iter().all(|s| s.is_finite())
}

/// TC-D: after an SR change (up and down), a fresh note_on must render finite,
/// non-silent audio.
#[test]
fn engine_sr_change_then_note_on_stays_sane() {
    let mut engine = SynthEngine::new(SR);
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];

    engine.set_sample_rate(96_000.0);
    engine.set_sample_rate(44_100.0); // down again — double call must be harmless
    engine.handle_event(MidiEvent::NoteOn { channel: 0, note: 60, velocity: 100 });
    for _ in 0..64 {
        engine.tick_block(&mut l, &mut r);
    }
    assert!(finite(&l) && finite(&r), "NaN/inf after SR change + note_on");
    assert!(rms(&l) > 1e-4, "silent after SR change + note_on (rms={})", rms(&l));
}

/// TC-E: Dattorro reverb resizes in place across the whole negotiated range.
#[test]
fn reverb_resizes_in_place_stays_sane() {
    let mut rev = Reverb::new(44_100.0);
    // Warm up with noise so the tank has energy, then change SR mid-stream.
    for i in 0..1000 {
        let s = ((i as f32) * 0.01).sin();
        let (l, r) = rev.tick(s, s * 0.5, 0.5, 0.3, 0.7, 0.02, 0.4);
        assert!(l.is_finite() && r.is_finite());
    }
    rev.set_sample_rate(192_000.0);
    let mut energy = 0.0;
    for i in 0..1000 {
        let s = ((i as f32) * 0.01).sin();
        let (l, r) = rev.tick(s, s * 0.5, 0.5, 0.3, 0.7, 0.02, 0.4);
        assert!(l.is_finite() && r.is_finite(), "NaN after SR change");
        energy += l.abs() + r.abs();
    }
    assert!(energy > 1e-3, "reverb dead after SR change (energy={energy})");
}

/// TC-F: spring reverb + chorus resize in place.
#[test]
fn spring_and_chorus_resize_stays_sane() {
    let mut spring = SpringReverb::new(44_100.0);
    spring.set_sample_rate(192_000.0);
    for i in 0..500 {
        let (l, r) = spring.tick(0.5, 0.5, 0.5, 0.6, 0.4, 0.2, 0.3, 0.0, 0.5);
        assert!(l.is_finite() && r.is_finite());
    }

    let mut chorus = Chorus::new(44_100.0);
    chorus.set_sample_rate(96_000.0);
    for i in 0..500 {
        let (l, r) = chorus.tick(0.5, 0.5);
        assert!(l.is_finite() && r.is_finite());
    }
}

/// TC-G: GUI-shared drum atoms must not be recreated by an SR change (the old
/// `*self = Self::new` dance cloned them back; a future refactor that drops
/// them would silently detach the GUI from the engine).
#[test]
fn drum_atoms_survive_sr_change() {
    let mut engine = SynthEngine::new(SR);
    let step = engine.drum_engine.step_atom();
    let play = engine.drum_engine.play_atom();
    let rec = engine.drum_engine.rec_atom();

    engine.drum_engine.set_sample_rate(44_100.0);

    assert!(Arc::ptr_eq(&step, &engine.drum_engine.step_atom()), "step_atom recreated");
    assert!(Arc::ptr_eq(&play, &engine.drum_engine.play_atom()), "play_atom recreated");
    assert!(Arc::ptr_eq(&rec, &engine.drum_engine.rec_atom()), "rec_atom recreated");
}
```

(В начало файла добавить `use std::sync::Arc;` — или Qualified path `std::sync::Arc::ptr_eq`.)

**Подписи tick проверить по исходникам перед коммитом** (аргументы могли отличаться):
`Reverb::tick` (reverb.rs:305-314 — room_size, damping, width, pre_delay, mix),
`SpringReverb::tick` (spring_reverb.rs:121-125 — size, decay, reflections, damping, spin,
_chaos, mix), `Chorus::tick` (chorus.rs:36 — input, mix). При расхождении — поправить
вызов по фактической сигнатуре, требование теста не менять.

- [ ] Проверка:

```bash
cargo test --test sr_change
cargo test drum_remap   # regression: user-state сохранение
```

### 7b. Полные гейты

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety   # 8 passed, 0 ignored
cargo deny check
```

Ожидание: всё зелёное. Если `rt_safety` падает — см. Task 6 диагностическую заметку.

### 7c. Docs

- [ ] `docs/TECHDEBT.md` §12 (:102-111): пометить РЕАЛИЗОВАНО, оставить историю:

```markdown
## 12. set_sample_rate аллоцирует в аудио-колбэке (rt-assert abort под JACK) — РЕАЛИЗОВАНО (2026-10-30, S1)
Вся цепочка `set_sample_rate` из колбэка (audio.rs:199-205) стала allocation-free:
FX ресайзят preallocated-буферы в capacity (SR_CAP=192 kHz), drum/voice обновляют
SR-поля явно, без `*self = Self::new`. Планы: docs/superpowers/plans/2026-10-30-refactor-s1-rt-safe-sr.md.
Принятое исключение: пересборка SF2-сэмплера в колбэке (sampler.rs:310-322, Displaced → trash) — отдельный TECHDEBT.
```

- [ ] `REFACTOR_AUDIT.md` строку S1 (:167-184) пометить в начале: `**РЕАЛИЗОВАНО 2026-10-30** —`
      (одной вставкой, текст аудита не переписывать).

- [ ] Commit:

```bash
git add tests/sr_change.rs docs/TECHDEBT.md REFACTOR_AUDIT.md
git commit -m "test: SR-change behavior contracts; docs: TECHDEBT #12 realized (audit S1)"
```

---

## Рискованные места (для исполнителя)

1. **voice.rs in-place** держится на инварианте «note_on реинициализирует все 3 осциллятора»
   (проверено: voice.rs:245, :364, :382). Если кто-то позже добавит осциллятор, чей
   SR-зависимый буфер живёт между note_on — он должен реинициализироваться в `reset()`.
   Комментарий в коде это документирует.
2. **Reverb len-field refactor** — самая объёмная правка. `tick_mod`/`read_at`/`push`
   обязаны читать `self.len`, НЕ `buffer.len()` — иначе буфер «заметёт» модуляцию и тапы
   за пределами active-длины. После правки S0-golden (когда появится) должен показать
   bit-идентичность до/после на одном SR.
3. **SR_CAP память**: reverb ~460 KB, spring allpasses ~180 KB при 48 kHz (vs ~75/45 KB
   раньше), аллоцируется один раз в `init_common`-цепочке (не в колбэке). Приемлемо;
   задокументировано в коде.
4. **Порядок задач важен** только для RED→GREEN: Task 2 не сделает зелёным TC-A (движок
   тянет остальные FX), поэтому его un-ignore только TC-B. TC-A/TC-C un-ignore в Task 6.
5. **Не глушить guard**: если после Task 6 abort остался — искать остаточный сайт
   инвентарём выше, а не ослаблять тест. Ослабление теста = commit marker `test-weaken`
   (требует явного одобрения пользователя).
6. **spring `set_len` clamp**: при SR > SR_CAP (недостижимо из negotiate) lengths молча
   укорачиваются — поведение задокументировано debug_assert'ом; UB нет.
