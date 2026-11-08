# QW3-batch: верифицированные фиксы 3 аудитов Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

status: executed 2026-11-08 (финал-ревью APPROVE; preset_hashes regen: 40 строк = легитимный sound change фикса Envelope)
anchor-checked: 2026-11-08 (база = master 663b03d; все якоря перепроверены 3
параллельными верификаторами против 663b03d: 41 CONFIRMED, 3 PARTIAL-поправки
учтены в тексте задач, 0 REFUTED)

**Goal:** починить 14 верифицированных дефектов из аудитов mod-matrix/LFO,
data layer, rhythm-кластера — только механические фиксы с ясной семантикой;
фиче-решения юзера (27 dead dests, swing-feel, dual persistence, Envelope на
ключевых LFO без ретрига) — вне скоупа, TECHDEBT.

**Architecture:** точечные правки в src/synth/{lfo,part,voice,mod,drum,
sampler,step_seq,looper}.rs + src/cc_map.rs + src/config.rs + gui/{browser,
mod}.rs. Каждый таск: юнит-тест (если тестируемо без аудио-стека) → фикс →
гейт → коммит. Goldens: таски 2/4 меняют звук ТОЛЬКО для tempo_sync/pb-range
пресетов — полный гейт покажет; красный golden = STOP.

**Tech Stack:** Rust; тесты cargo test (unit в файлах + tests/golden).

**Проверенные факты (HEAD 663b03d):**
- lfo.rs:379-380: `coeff = 0.999 - deform * 0.099`; deform-слайдеры −1..1
  (params.rs:1078/1104/1127/1147/1173/1209) → deform < −0.091 = нестабильный
  полюс, NaN абсорбируется в `sh_smooth`, переживает LoadPatch (Lfo живёт в
  Part::new, part.rs:205-206).
- lfo.rs:251: `tempo_sync_cycles_per_beat(rate as u8)` — таблица 0..=15
  (lfo.rs:63-86), `as u8` от Hz; free-run компенсация блочности = `rate * bl`
  на call-site (part.rs:929/940/947/952/983-987/996-1000), sync-ветка `* bl`
  пропускает → синхронный LFO в block_len раз медленнее.
- lfo.rs:390-414: env-стадии 1=attack, 2=decay→0.5, 3=sustain 0.5, 4=release
  (никуда не ведёт), 0=idle; НИКТО не пишет stage=1 → Envelope всегда −1.0.
  trigger_mode (lfo.rs:224-241) вызывается на note-on для lfos[0..3]
  (part.rs:306-309); Scene LFO не ретригается (part.rs:977).
- part.rs:1030: `pitch_bend: effective_pb(...) / 2.0` — хардкод ±2 st при
  range до 24 (part.rs:66-68) → source до ±12 при range 24; dest Pitch range
  24 (mod_matrix.rs:277) × depth 1 = ±288 полутонов.
- voice.rs:200-254 note_on не трогает poly_aftertouch (пишется только
  mod.rs:1127 и init voice.rs:125; даже reset() ~195 не чистит).
- mod.rs:1133: `ControlChange { cc, value, .. }` — channel деструктурируется
  в `..`; CC2/CC11/CC64/CC1-4 применяются ко всем партам без фильтра канала,
  в отличие от Aftertouch/PolyAT (mod.rs:1118-1122, фильтр `channel != 9`).
- sampler.rs:388-400: set_part_mode аллоцирует канал (alloc_channel 381-386)
  БЕЗ отправки program; API посылки: `self.keys.midi(ch, 0xC0, program, 0)`
  (sampler.rs:425); кэш `part_program[part]` (sampler.rs:208/408-412).
  browser.rs:113-116 (кнопка тоггла) и пост-closure picked_switch (~410-414)
  пушат только SetPartSf2Mode, без program.
- mod.rs:2243-2251/2270-2274: оба пути закрытия capture ставят
  looper_rec_cycles=0 без set_master_rec_state(0,0); сброс только в
  master_stop (680) и disarm (761). GUI-гейт rec_state==2 (drums.rs:995).
- mod.rs:672-692 master_stop: нет trim_after; ёмкость cycle*64 ставится в
  2235; kept-формула закрытия: rec_cycles*cycle_ticks (2245-2247).
- drum.rs:751/774: pending_pattern: u8, NO_PATTERN=0xFF (701); queue_pattern
  immediate-ветка (799-801) и set_pattern_immediate (823-827) не чистят.
- drum.rs:1216-1225 sf2_choke_take: `group_last_note[g-1]`, g не валидируется
  (NUM_CHOKE_GROUPS=2, drum.rs:37) → OOB panic на RT-потоке (latent).
- drum.rs:1320-1322: клик-гейт `current_step != prev_step`; length==1 →
  шаг никогда не меняется (tick 950 `% length.max(1)`).
- looper.rs:156 current_layer: u8; инкременты 327/655 и `+1` в update_atoms
  768 — без насыщения.
- step_seq.rs:239-247: один шаг за блок (`if`), phase_acc остаток хранит —
  при block_len > samples_per_step шаги пропускаются.
- config.rs:27-66: Sf2Settings derive(Default) → block_size:0 (serde-default
  = 8, config.rs:68); Config::default кладёт 0 в свежий конфиг.
- cc_map.rs:233-247 from_cc_map сериализует только Some; from_config
  (160-184) стартует от default_map() → снятый бинд дефолтного CC
  (gui/settings.rs:553-554 bindings[cc]=None) воскресает при рестарте.
  Дефолтные CC известны из default_map() (cc_map.rs:117-155).
- gui/mod.rs:1883 (было 1873): `if self.load_performance_by_name(&name)` без
  else; gui/layers.rs:326 `.ok()`; gui/performance.rs:15-17 `Err(_) => return
  false` — рестарт перфа молча падает каждый старт.
- mod.rs:380 `midi_seq_buf` Vec::new(); только clear (midi_player.rs:173),
  push без cap (209) → realloc в RT-колбэке на плотных блоках.

---

## Test Cases (from the requirement)

- TC1: SmoothNoise LFO, deform=−1.0: 5000 блоков tick → значение конечно
  (f32::is_finite), не inf/NaN.
- TC2: LFO tempo_sync, bpm=120, rate=4.0 (индекс 4 = 1/4 = 1 beat): один
  tick_with_deform при bl=128, sr=48000 продвигает фазу ровно на
  (2.0 Гц × 128 / 48000) — как free-run 2 Гц.
- TC3: LFO waveform=Envelope: env_retrigger() → tick за ~0.25 c атакует до
  ~1.0 (bipolar → ~+1.0), затем спадает до 0.5 (sustain), потом цикл заново;
  без env_retrigger на keyed-LFO — stage 0 (старое поведение до ноты).
- TC4: PitchBend source при range=24: source при полном отклонении ≈ ±1.0
  (не ±12).
- TC5: voice.note_on на голосе с poly_aftertouch=0.7 → поле 0.0.
- TC6: ControlChange{cc:64, value:127, channel:9} НЕ включает sustain
  синт-партов; channel:0 — включает.
- TC7: set_part_mode(part, true) после смены программы: канал получает
  part_program[part] (test/diagnostic getter part_program()).
- TC8: закрытие quantized capture → master_rec_state atom == 0 (Playing/Idle),
  не 1/2.
- TC9: master_stop во время capture → loop_length_ticks == rec_cycles ×
  cycle_ticks (обрезано), не cycle×64.
- TC10: queue_pattern(3) quantized → set_pattern_immediate(5) → после wrap
  текущий 5 (pending 3 забыт).
- TC11: sf2_choke_take с g=200 → ранний выход, без panic.
- TC12: length=1 паттерн: метроном кликает каждый шаг-период.
- TC13: current_layer инкременты насыщаются на 255 (u8::MAX), не паникуют/
  не wrap-ятся.
- TC14: блок с > 4096 midi-событиями (например 4200): midi_seq_buf.len()
  останавливается на MIDI_SEQ_BUF_CAP, без realloc и паники (дроп
  осознанный, RingBuffer-канон кодовой базы).
- TC15: Learn→Clear на дефолтном CC55 → рестарт приложения → бинд остаётся
  снятым.
- TC16: Config::default().sf2.block_size == 8.

## Negative Matrix

- deform = +1.0 → coeff 0.9 — стабильный, поведение не меняется (тест TC1
  покрывает только отрицательную сторону — позитивная ветка не трогается).
- rate как Hz > 15 при tempo_sync (старые пресеты-мусор) → clamp min(15) —
  тест TC2-вариант: rate=20.0 → индекс 15, без паники.
- part ≥ 16 в set_part_mode → ранний return (существующий гвард 389) — не
  трогается.
- capture не активен при master_stop → trim не вызывается (гвард по
  looper_capture_quantized).
- rec_cycles = 0 при close → set_master_rec_state(0,0) идемпотентен.
- pending_pattern = NO_PATTERN при queue(immediate) — присваивание None
  идемпотентно.
- config без cc_map (None) → default_map, cleared пуст — существующее
  поведение не трогается.
- config без cleared-поля (старый JSON) → serde default пустой Vec.
- Envelope на Scene LFO без нот → цикл AD продолжается (документировано в
  плане); на keyed-LFO до первой ноты — stage 0 (−1.0), как раньше.
- channel != 9 у CC — поведение не меняется (гвард только на 9).
- ~~~clock skew~~ — n/a (один клок). ~~~timeout внешних вызовов~~ — n/a.
- duplicate/concurrent: set_part_mode(true) дважды — второй раз канал уже
  есть (is_none гвард 390), program не дублируется.

---

### Task 1: lfo.rs — SmoothNoise стабильный coeff (NaN-яд)

**Files:** Modify `src/synth/lfo.rs:375-383`; тест в `src/synth/lfo.rs` (tests-модуль, создать если нет).

- [x] Step 1: failing-тест в `mod tests` внизу lfo.rs:
```rust
    #[test]
    fn smooth_noise_stays_finite_for_negative_deform() {
        let mut lfo = Lfo::new(48000.0);
        lfo.unipolar = false;
        for _ in 0..5000 {
            let v = lfo.tick_with_deform(5.0, LfoWaveform::SmoothNoise, -1.0);
            assert!(v.is_finite(), "diverged: {v}");
        }
    }
```
  Проверить имя конструктора (`Lfo::new(sample_rate)`) по part.rs:205-206;
  если сигнатура иная — адаптировать по факту, семантику теста не менять.
- [x] Step 2: `cargo test smooth_noise` → FAIL (diverged/inf).
- [x] Step 3: фикс lfo.rs:379:
```rust
                // deform < -0.091 давал coeff > 1 = неустойчивый полюс
                // (экспоненциальный разлёт → inf → NaN навсегда). Клэмп
                // держит полюс внутри единичного круга.
                let coeff = (0.999 - deform * 0.099).min(0.999);
```
- [x] Step 4: `cargo test smooth_noise` → PASS; `cargo test --test golden` байт-стабилен.
- [x] Step 5: коммит `fix: SmoothNoise LFO unstable filter pole → NaN poisoning (audit mod-matrix #5)`.

### Task 2: lfo.rs + part.rs — tempo-sync: масштаб блока внутри Lfo + клэмп индекса

**Files:** Modify `src/synth/lfo.rs` (struct Lfo поле block_len, effective_rate 249-256), `src/synth/part.rs` (update_mods пролог + 6 call-site без изменений формы); тест в lfo.rs.

- [x] Step 1: failing-тест в lfo.rs:
```rust
    #[test]
    fn tempo_sync_advances_like_freerun_at_division_hz() {
        // rate = индекс деления 4 (1 beat) при bpm 120 → 2.0 Гц
        let bl = 128.0_f32;
        let mut synced = Lfo::new(48000.0);
        synced.tempo_sync = true;
        synced.set_bpm(120.0);
        synced.set_block_len(bl);
        let mut free = Lfo::new(48000.0);
        for _ in 0..64 {
            let s = synced.tick_with_deform(4.0, LfoWaveform::Sine, 0.0); // индекс 4
            let f = free.tick_with_deform(2.0 * bl, LfoWaveform::Sine, 0.0); // 2 Гц
            assert!((s - f).abs() < 1e-4, "sync {s} vs free {f}");
        }
    }
```
- [x] Step 2: `cargo test tempo_sync` → FAIL (sync фаза в 128 раз медленнее).
- [x] Step 3: lfo.rs — struct Lfo: поле `block_len: f32` (init 128.0 в
  конструкторе, рядом с sample_rate/bpm) + метод:
```rust
    /// Длина блока в сэмплах — tick вызывается раз на блок, sync-ветка
    /// компенсирует блочность сама (audit mod-matrix #3b).
    pub fn set_block_len(&mut self, bl: f32) { self.block_len = bl.max(1.0); }
```
  и effective_rate:
```rust
    fn effective_rate(&self, rate: f32) -> f32 {
        if self.tempo_sync {
            // rate = ИНДЕКС деления 0..15 (не Hz); ×block_len: tick раз на
            // блок, advance_phase добавляет hz/sample_rate за вызов
            let idx = (rate as u8).min(15);
            self.bpm / 60.0 * tempo_sync_cycles_per_beat(idx) * self.block_len
        } else {
            rate
        }
    }
```
- [x] Step 4: part.rs update_mods пролог (до LFO-тиков, ~920):
```rust
        for l in self.lfos.iter_mut().chain(self.scene_lfos.iter_mut()) {
            l.set_block_len(bl);
        }
```
  Call-site (6 шт: 929/940/947/952/983-987/996-1000) НЕ менять — они уже
  шлют «sync: raw, free: ×bl»; теперь sync-ветка масштабирует сама.
  Комментарий на 926-928 обновить: «индекс деления 0..15, clamp 15».
- [x] Step 5: `cargo test tempo_sync` → PASS; полный `cargo test`; golden
  красный = STOP (reviewer подтвердил: golden-свитч не включает tempo_sync,
  фабричные пресеты без него — ожидается зелёный).
- [x] Step 6: коммит `fix: LFO tempo-sync — block-length scaling inside Lfo + division index clamp (audit mod-matrix #3a,b)`.

### Task 3: lfo.rs + part.rs — Envelope: attack на note-on, AD-цикл

**Files:** Modify `src/synth/lfo.rs` (метод env_retrigger рядом с trigger_mode 224-241; env arm 384-417), `src/synth/part.rs` (3 сайта trigger_mode: 306-309 latch, 345-348 mono, 405-408 poly); тест в lfo.rs.

- [x] Step 1: failing-тест (rate масштабируется на bl=128 — каждый tick =
  один 128-сэмпловый блок; атака 0.25 c = 94 блока):
```rust
    #[test]
    fn envelope_attacks_after_retrigger_and_loops() {
        let mut lfo = Lfo::new(48000.0);
        lfo.env_retrigger();
        let rate = 5.0 * 128.0; // bl=128: dt = rate/sr на tick (как в живом тракте)
        let mut peak: f32 = -2.0;
        for _ in 0..120 {
            let v = lfo.tick_with_deform(rate, LfoWaveform::Envelope, 0.0);
            peak = peak.max(v);
        }
        assert!(peak > 0.9, "no attack, peak={peak}");
        // цикл: после спада (sustain-stage перезапускает атаку) значение
        // снова уходит вниз — минимум следующего окна отрицателен (bipolar)
        let mut mid: f32 = 2.0;
        for _ in 0..400 { mid = mid.min(lfo.tick_with_deform(rate, LfoWaveform::Envelope, 0.0)); }
        assert!(mid < -0.5, "no decay/loop, min={mid}");
    }
```
- [x] Step 2: сначала добавить заглушку `pub fn env_retrigger(&mut self) {}`,
  `cargo test envelope` → FAIL compile-независимо (peak ≈ −1.0 — env не
  атакует); затем замена тела (см. Step 3).
- [x] Step 3: lfo.rs:
```rust
    /// Envelope one-shot: (re)start attack (audit mod-matrix #4 — stage 1
    /// никем не писался, волна была константной -1.0).
    pub fn env_retrigger(&mut self) {
        self.env_stage = 1;
        self.env_value = 0.0;
    }
```
  и stage 3 → цикл (lfo.rs:405): семантика = повторяющийся attack-decay
  (sustain-холд без note-off хука превратил бы сцену/ноту в ступор 0.5):
```rust
                    3 => {
                        // Нет note-off хука к LFO — «sustain» превращается в
                        // цикл AD (атака заново), иначе ступор на 0.5 навсегда.
                        self.env_stage = 1;
                        self.env_value = 0.0;
                    }
```
- [x] Step 4: part.rs — ВСЕ ТРИ сайта trigger_mode (306-309 latch, 345-348
  mono, 405-408 poly): после каждой группы trigger_mode добавить env-ретриг
  для соответствующих lfos[i], с waveform-параметром, который читает ИМЕННО
  этот сайт (grep контекст: latch-ветка использует lfo_waveform/lfo2_waveform
  …; mono/poly — те же params). Шаблон (по одному на каждый lfo сайта):
```rust
                if self.params.lfo_waveform == 5.0 { // Envelope (from_param 5)
                    self.lfos[0].env_retrigger();
                }
```
  Scene LFO не ретригается (part.rs:977) — их Envelope живёт циклом AD из
  stage-3-перезапуска.
- [x] Step 5: PASS; полный тест; golden стабилен (reviewer: golden-пресеты
  не используют waveform 5 — случайный, но проверенный факт).
- [x] Step 6: коммит `fix: LFO Envelope waveform never attacked — env_retrigger on note-on + AD loop (audit mod-matrix #4)`.

### Task 4: mod.rs + part.rs — PitchBend source нормировка на диапазон

**Files:** Modify `src/synth/part.rs` (EngineModInputs поле + использование :1030), `src/synth/mod.rs` (producer ~1056-1068).

- [x] Step 1: EngineModInputs (part.rs Copy-структура): добавить поле
  `pitch_bend_norm: f32` (нормированный ±1 ДО умножения на range). Producer
  в mod.rs:1056-1068 уже имеет norm на руках — положить в inputs рядом с
  pitch_bend_semitones. Все точки сборки EngineModInputs (grep
  `EngineModInputs {`) дополнить полем.
- [x] Step 2: part.rs:1030:
```rust
            // Нормированный ±1 ОТ ПРОДЮСЕРА (хардкод /2.0 = ±2 st давал
            // source ±12 при range 24 → до ±288 полутонов на dest; делить
            // semitones на текущее значение нельзя — это не range).
            pitch_bend: effective_pb(self.ignore_pb, inputs.pitch_bend_norm),
```
  (effective_pb — identity/zero-out ignore_pb; norm уже ±1.)
- [x] Step 3: гейт полный; golden зелёный (reviewer: PB не входит в
  golden-рендер). Sound change только для range ≠ 2 пресетов.
- [x] Step 4: коммит `fix: PitchBend mod-source normalized (±1) at producer, not hardcoded /2 (audit mod-matrix #10)`.

### Task 5: voice.rs — poly_aftertouch сброс на note_on

**Files:** Modify `src/synth/voice.rs:209-212 (note_on prologue)`.

- [x] Step 1: в начале note_on после `self.age = age;`:
```rust
        // Новая нота на переиспользованном голосе не должна наследовать
        // полифоническое давление предыдущей клавиши (audit data #stale-AT).
        self.poly_aftertouch = 0.0;
```
  (voice.rs `reset()` НЕ существует — reviewer; только note_on.)
- [x] Step 2: тип поля сверить (f32) по voice.rs:125.
- [x] Step 3: гейт (cargo test).
- [x] Step 4: коммит `fix: stale per-voice poly aftertouch on voice reuse (audit mod-matrix #stale-AT)`.

### Task 6: mod.rs — ControlChange фильтр drum-канала

**Files:** Modify `src/synth/mod.rs:1133` + точка application (1163-1178).

- [x] Step 1: деструктуризация с каналом:
```rust
            MidiEvent::ControlChange { cc, value, channel, .. } => {
```
  (если channel в variant называется иначе — grep MidiEvent::ControlChange
  определение в events.rs и использовать фактическое имя поля).
- [x] Step 2: после блока 120/123 (паника остаётся глобальной — канал
  независим), перед application-блоком:
```rust
                // Drum-канал (9 = GM ch10): CC на нём не должен управлять
                // синт-партами (breath/expression/sustain/CC1-4), как
                // Aftertouch/PolyAT выше (audit mod-matrix #CC-channel).
                if channel == 9 { return; }
```
  Если обработчик не может `return` (тип Option<ControlEvent>) — заменить
  на структуру `if channel != 9 { ...application block... }`; контракт
  `#[must_use]` сохранить (return должен возвращать значение/None по
  фактической сигнатуры). ПРИНЯТЫЙ СТЭОРЭФФЕКТ: channel-9 CC исчезают из
  CcReceived GUI-фидбека/learning (mod.rs:1198) — drum-канал CC-learn не
  фича, осознанно.
- [x] Step 3: гейт + коммит `fix: drum-channel (9) CC no longer drives synth parts (audit mod-matrix #8)`.

### Task 7: sampler.rs + browser.rs — SF2 program на свежем канале

**Files:** Modify `src/synth/sampler.rs:388-400`, `src/gui/browser.rs:113-116` (кнопка тоггла), `src/gui/browser.rs:461-471` (post-closure picked_switch_to_sf2).

- [x] Step 1: sampler.rs — program сразу при аллокации:
```rust
        if enabled && self.part_channel[part].is_none() {
            self.part_channel[part] = self.alloc_channel();
            // Свежий/переиспользованный канал стартует на program 0 от
            // rustysynth — выставляем кэш парта немедленно (иначе браузерный
            // тоггл DSP→SF2 играет Grand Piano, а кэш/г UI говорят X;
            // reuse-канал наследует патч предыдущего occupant'а).
            if let Some(ch) = self.part_channel[part] {
                if let Some(synth) = &mut self.keys.synth {
                    synth.midi(ch as i32, 0xC0, self.part_program[part] as i32, 0);
                }
            }
        } else if !enabled {
```
- [x] Step 2: browser.rs кнопка тоггла (~113-116): после
  `SetPartSf2Mode { .. }` пуш добавить:
```rust
                    if !cur_sf2 {
                        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                            part, program: self.parts[part].sf2_program, bank: 0 });
                    }
```
  (только в SF2-сторону; DSP-сторона программы не касается).
- [x] Step 3: browser.rs post-closure `picked_switch_to_sf2` (461-471): в
  ветке `if self.parts[part].sf2_mode != sf2` после SetPartSf2Mode push:
```rust
                if sf2 {
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                        part, program: self.parts[part].sf2_program, bank: 0 });
                }
```
  (SetPartFull-путь уже шлёт program после mode — не дублируется, verifier
  подтвердил порядок 1430-1438.)
- [x] Step 4: гейт + rt_safety (синт.midi — существующий RT-вызов) + коммит
  `fix: SF2 channel gets cached program on alloc; browser toggle sends program (audit data #F1)`.

### Task 8: mod.rs — master_rec_state сброс при закрытии capture

**Files:** Modify `src/synth/mod.rs:2243-2251, 2270-2274`.

- [x] Step 1: в ОБОИХ путях закрытия (2243-2251 — штатное закрытие на
  boundary, 2270-2274 — fallback смерти клока) рядом с
  `looper_rec_cycles = 0`:
```rust
                    // REC/CLOSING-атом надо гасить ЗДЕСЬ: единственные сбросы
                    // были в master_stop/disarm — GUI висел в «REC n тактов»/
                    // «CLOSING» с мёртвыми +1/−1 (audit rhythm #1).
                    self.set_master_rec_state(0, 0);
```
- [x] Step 2: гейт; ручная проверка-тест: если есть test capture-семантики —
  добавить assert на atom после close; если нет — не писать новый
  characterization-тест (это отдельный TECHDEBT-пререквизит), полагаться на
  гейт + экран.
- [x] Step 3: коммит `fix: master REC/CLOSING atom reset on capture close (audit rhythm #1)`.

### Task 9: mod.rs — master_stop обрезает недозаписанный луп

**Files:** Modify `src/synth/mod.rs:672-692` (вставка ДО строки 674
`looper_capture_quantized = false` и 678 `looper_rec_cycles = 0`).

- [x] Step 1: в master_stop, если capture был в полёте — обрезать И длину
  (trim_after ужимает события, но НЕ трогает loop_length_ticks; закрытие на
  boundary в ~2248 делает оба вызова — повторить пару):
```rust
            // Стоп посреди quantized capture: обрезать луп до записанных
            // циклов (события + длина), иначе он остаётся cycle*64 с тишиной
            // в хвосте (audit rhythm #2).
            if self.looper_capture_quantized {
                let kept = self.looper_rec_cycles.saturating_mul(self.looper_cycle_ticks);
                self.looper.trim_after(kept);
                self.looper.set_loop_len_ticks(kept);
            }
```
  (saturating_mul — как на пути закрытия 2245-2247; trim_after/set_loop_len_ticks — looper.rs 373/353.)
- [x] Step 2: гейт + коммит `fix: master_stop trims half-captured loop to recorded cycles (audit rhythm #2)`.

### Task 10: drum.rs — pending_pattern гигиена + choke гвард + метроном len=1

**Files:** Modify `src/synth/drum.rs:797-805, 823-827, 1216-1225, 1320-1322`.

- [x] Step 1: queue_pattern immediate-ветка (799-801):
```rust
        if !self.playing || self.patterns[self.current_pattern as usize].length <= 1 {
            self.current_pattern = pattern;
            self.pending_pattern = NO_PATTERN; // немедленный выбор перебивает очередь (audit rhythm #7)
            self.sync_pattern_atom();
        } else {
```
- [x] Step 2: set_pattern_immediate (823-827): та же строка после
  `self.current_pattern = pattern;`.
- [x] Step 3: choke гвард (1216-1225), первой строкой sf2_choke_take:
```rust
        if g == 0 || g as usize > NUM_CHOKE_GROUPS { return; } // latent OOB panic на RT (audit rhythm #8)
```
- [x] Step 4: метроном length-1 (1320-1322): гейт сейчас
  `current_step != prev_step` — на length==1 шаг навсегда 0. Нужен флаг
  переноса фазы из секвенсора: в `StepSequencer` (drum.rs:733) завести
  `pub step_wrapped: bool` (+ init в `StepSequencer::new()` drum.rs:760)
  — true, если на этом блоке произошёл перенос фазы шага: ставить в ветке
  wrap `phase_acc >= samples_per_step + swing_offset` (drum.rs:937), сбрасывать
  в начале tick; stale-значение при `!playing` безвредно (метроном-гейт
  требует playing). Метроном-гейт в DrumEngine::tick:
```rust
        let beat_boundary = if self.patterns[self.current_pattern as usize].length <= 1 {
            self.sequencer.step_wrapped
        } else {
            self.current_step != prev_step
        };
```
  (имена self.sequencer/поля сверить по 922-965; length>1 поведение без
  изменений).
- [x] Step 5: гейт + тесты drum_choke/metronome (tc6-10 существуют) + коммит
  `fix: drum pending_pattern hygiene, choke bounds guard, metronome on length-1 patterns (audit rhythm #7,8,9)`.

### Task 11: looper.rs + step_seq.rs — насыщение layer + catch-up шагов

**Files:** Modify `src/synth/looper.rs:327, 655, 768`, `src/synth/step_seq.rs:239-247`.

- [x] Step 1: looper.rs — 3 инкремента → saturating:
```rust
        self.current_layer = self.current_layer.saturating_add(1);
```
  и в update_atoms (768): `self.current_layer.saturating_add(1)` — сверить
  типы atom-публикации.
- [x] Step 2: step_seq.rs — замена `if` на catch-up `while`; swing_offset
  (233-237) пересчитывается ВНУТРИ каждой итерации (он зависит от чётности
  ТЕКУЩЕГО шага — вынос наружу при ≥2 шагах в блоке дрейфует тайминг,
  reviewer):
```rust
            // block_len > samples_per_step (1/32 @ высокий BPM, большие буферы):
            // один `if` пропускал шаги — догоняем все переносы (audit rhythm #17)
            while self.phase_acc >= samples_per_step {
                let swing_offset = /* существующая формула 233-237, от текущего step */;
                self.phase_acc -= samples_per_step + swing_offset;
                ...тело существующего advance (шаг+1, события, atoms)...
            }
```
  (сохранить фактическую арифметику 233-247: `samples_per_step +
  samples_per_step * swing * 0.5` на нечётных; УСЛОВИЕ while тоже с
  swing_offset, пересчитанным в теле каждой итерации:
  `while self.phase_acc >= samples_per_step + swing_offset {` — иначе
  swing-задержанный нечётный шаг стрельнёт рано; atoms/step-событие — по
  последнему шагу).
- [x] Step 3: гейт (rt_safety обязателен — step_seq в RT-тракте) + коммит
  `fix: looper current_layer saturating; pitch-seq catch-up for dense blocks (audit rhythm #11,17)`.

### Task 12: config.rs + cc_map.rs + settings.rs — снимок дефолтного CC переживает рестарт

**Files:** Modify `src/cc_map.rs:106-155 (структура/def), 160-184 (from_config), 233-247 (from_cc_map)`.

- [x] Step 1: CcMapRaw — поле:
```rust
    /// CCs, unbound пользователем относительно default_map (снятые дефолтные
    /// бинды переживают рестарт — audit data #F11). serde default = [] (старые
    /// конфиги без поля).
    #[serde(default)]
    pub cleared: Vec<u8>,
```
- [x] Step 2: from_cc_map — автодерив: снимки = дефолтные CC, которых нет в
  живой карте:
```rust
        let cleared = Self::default_map()
            .bindings
            .iter()
            .enumerate()
            .filter(|(i, slot)| slot.is_some() && map.bindings[*i].is_none())
            .map(|(i, _)| i as u8)
            .collect();
        Self { bindings, cleared }
```
- [x] Step 3: from_config — после цикла overwrite:
```rust
        // Снятые пользователем дефолтные бинды: default_map их вернул выше —
        // гасим явно (без этого «Clear MIDI» воскресал дефолты при рестарте).
        for cc in &raw.cleared {
            let cc = *cc as usize;
            if cc < 128 && cc != 64 {
                map.bindings[cc] = None;
            }
        }
```
- [x] Step 4: тест в cc_map.rs (mod tests): default_map → снять bindings[55] →
  from_cc_map → from_config(круг) → bindings[55] == None, bindings[48] жив.
- [x] Step 5: гейт + коммит `fix: cleared default CC bindings persist across restart (audit data #F11)`.

### Task 13: config.rs — Sf2Settings ручной Default

**Files:** Modify `src/config.rs:27-28, добавить impl Default`.

- [x] Step 1: из derive убрать `Default`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sf2Settings {
```
- [x] Step 2: после структуры:
```rust
impl Default for Sf2Settings {
    /// derive(Default) давал block_size: 0 (serde-default = 8) — свежий
    /// конфиг нёс 0 до клэмпа в sampler (audit data #F8).
    fn default() -> Self {
        Self {
            file_path: None,
            keys_file_path: None,
            drums_file_path: None,
            layer_a_sf2: false,
            layer_a_program: 0,
            layer_b_sf2: false,
            layer_b_program: 0,
            drums_sf2: false,
            drums_remaps: Default::default(),
            drums_vels: Default::default(),
            block_size: default_sf2_block_size(),
            sample_offset_ms: 0.0,
        }
    }
}
```
- [x] Step 3: тест в config.rs tests:
```rust
    #[test]
    fn sf2_settings_default_block_size_is_not_zero() {
        assert_eq!(crate::config::Sf2Settings::default().block_size, 8);
    }
```
- [x] Step 4: гейт + коммит `fix: Sf2Settings::default block_size 8 (derive Default gave 0) (audit data #F8)`.

### Task 14: gui — молчаливые фейлы ресторана перфа + RT-cap midi_seq_buf

**Files:** Modify `src/gui/mod.rs:1883-1885`, `src/gui/layers.rs:326`, `src/gui/performance.rs:15-17`, `src/synth/midi_player.rs:209 (push сайт)`.

- [x] Step 1: gui/mod.rs 1883-1885 — сохранить существующий success-лог
  (`log::info!("[init] performance restored: {name}")`), добавить else-ветку:
```rust
            } else {
                log::warn!("perf restore: failed to load last_performance {:?} — defaults kept", name);
            }
```
- [x] Step 2: gui/layers.rs:326 `.ok()` → лог:
```rust
            if let Err(e) = preset::load_performance(path) {
                log::warn!("perf combo load failed: {e}");
            }
```
- [x] Step 3: gui/performance.rs:15-17:
```rust
        Err(e) => { log::warn!("apply_performance failed: {e}"); return false; }
```
- [x] Step 4: RT-cap: push находится в midi_player.rs:209
  (`out.push((ch, d.clone()))` внутри process_block), НЕ в mod.rs. В
  midi_player.rs рядом с обработкой:
```rust
/// RT-контракт: плотный блок (стопка событий на wrap) раньше растил буфер
/// → malloc в аудио-колбэке (audit rhythm #13). Cap = осознанный дроп.
pub const MIDI_SEQ_BUF_CAP: usize = 4096;
```
```rust
                if out.len() < MIDI_SEQ_BUF_CAP {
                    out.push((ch, d.clone()));
                }
```
- [x] Step 5: гейт (rt_safety обязателен) + коммит
  `fix: log silent perf-restore failures; cap midi_seq_buf (RT alloc) (audit data #F6, rhythm #13)`.

### Task 15: Верификация стадии

- [x] Step 1: полный гейт: `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `cargo test --features rt-assert --test rt_safety`,
  `cargo deny check`, goldens байт-стабильны (`git status tests/golden`
  пустой).
- [x] Step 2: PR → squash → rebuild → launch (DISPLAY=:0, окно живо).
- [x] Step 3: экран-чеклист юзеру: Scene LFO (SmoothNoise deform<0 — не
  молчит; Tempo Sync — клик на деление; Envelope — цикл), браузер
  DSP→SF2-тоггл играет ЗАПИСАННЫЙ инструмент, REC-статус гаснет после
  закрытия capture, Clear CC на дефолтном — переживает рестарт.

## Out of scope (фиче-решения юзера, TECHDEBT)
- 27 dead mod-destinations — подключить или спрятать из GUI (TECHDEBT).
- Swing feel (инверт + темпо-сдвиг, cycle=1+swing/4) — правка меняет грув,
  слушать юзером.
- Dual persistence mute/note_mode (perf vs parts_state) — чей приоритет.
- SF2 16-й парт starvation retry, sustain-маска после паники, skip_samples
  побочный дроп, pitch_bend_range в load_keys_soundfont, SMPTE/wrap-tempo,
  midi_seq mute-зеркало, LED-window optimistic paging, drum atoms freeze,
  gate_changed dead code — LOW, в TECHDEBT-пул.
