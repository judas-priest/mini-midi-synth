# Tech Debt / Backlog

Recorded 2026-10-27. Ordered roughly by user-perceived impact.

## 1. Metronome tied to sequencer BPM
- Audible click (toggleable) synced to the drum-sequencer BPM; plus a visual click indicator in the GUI (step highlight already exists — add a downbeat accent + GUI flash).
- Click sound: synthesize a short tick in the drum engine (accent on beat 1) — no sample needed.
- Should follow the same BPM source as the sequencer + looper sync (`looper_sync_bpm`).

## 2. Sequencer records only one layer, not the whole performance
- Current: drum step sequencer records hits into the active drum pattern; part/looper recording targets a single layer (`active_part`). Expected: "record" captures the whole performance (all enabled parts), not just one layer.
- Needs design: multi-layer capture is what the looper already does (audio). Decide: extend looper to capture all enabled layers, or make the sequencer/PitchSeq record across parts.

## 3. Default empty init performance
- No factory "Init" performance exists — pad perf slots start empty and the first saved perf becomes the reference.
- Add `Init` performance (single part A1 = init preset, full range, no SF2) compiled into the binary the same way the Init preset is in `FACTORY_PRESETS`; reset action restores it.

## 4. Panorama: works for DSP parts, not for SF2 parts
- DSP: per-part pan applied in `tick_block` (constant-power, mod.rs ~2560).
- SF2: `sampler.tick()` output is summed into the mix BEFORE the per-part pan? — verify actual routing; rustysynth has its own stereo field per preset and our pan likely never reaches SF2 channels.
- Options: pan SF2 channels via rustysynth CC10 (check if vendor implements CC10 — likely not; vendor patch), or split sampler output per part channel and apply pan in the mix (rustysynth renders all channels mixed — needs per-channel render, expensive) — investigate before deciding.

## Carried-over (from the eternal-sustain audit, out of scope there)
- Mute is not applied in the mix — only gates new note-ons (mod.rs tick_block checks `enabled` only). Muted part keeps sounding if notes started before mute.
- `seq_program_set` desyncs `part_program[]` cache: MIDI-file program changes overwrite a live part's rustysynth channel program; next `load_keys_soundfont` restore clobbers the sequencer's program mid-playback.
- CC120 (All Sound Off) handled as release-style note_off, not an immediate kill.
- Sustain-atom divergence: `sampler.all_notes_off()` force-clears sampler pedal state while engine atom/part flags may stay ON (MidiSeqLoad/Play-stop paths) — DSP and SF2 sustain can diverge until the next pedal edge.
- Panic-path arp flushes (mod.rs ~1722/~2047/~2198) dispatch untransposed off_notes — harmless today (full drains follow) but drifts from `flush_arp_note_off`.
- `SetPartEnabled{false}` runs its cleanup block twice (drain + legacy graceful note-off) — collapse.
- `cargo deny`: rtrb 0.3.3 advisory RUSTSEC-2026-0274 — `cargo update -p rtrb` (→ 0.3.5).

## 5. Chord detection for recording
- Problem: sequencer/looper records every played note; user wants to play a chord shape and have a single chord recorded (smart chord capture), not N separate notes.
- Research when picked up: chord detection algorithms (pitch-class-set → chord name/midi chord), how Smart Chord on M-VAVE / DAW chord tracks capture; likely: detect simultaneously-pressed notes within a window, collapse to one chord event.

## 6. Root-note-only mode for a layer (bass split)
- Problem: many layers play the full chord; bass gets muddy. User wants a per-layer mode "play only the root note" (lowest played note / detected chord root), transposed down an octave — bass layer follows the chord root while other layers play the chord.
- Research when picked up: chord-root detection from held notes (same detection as #5), existing implementations (arpeggiator root mode, "bass follow" in workstations).
- Depends on #5 (root detection).

## 7. Custom sustain release (SR) — вернуть как статичную настройку
- Research done 2026-10-27 (best practices: Ravenscroft 275, Ivory II, NI Claire).
- Industry standard: TWO-VALUE model — release while pedal held (native/long) vs
  release after pedal-up (user-configurable). NOT coupled to fader/pedal
  position live: scaling the tail by pedal position makes fast pedal taps chop
  tails and is unplayable (this exact failure was reverted in 59ed413).
- Half-pedal (continuous pedal scaling release+resonance) needs a continuous
  pedal with configurable Min/Max CC range — out of scope until such a pedal.
- Re-pedal (quick off→on re-catch) already works.
- Implementation exists in git history: DSP override `Envelope::note_off_for` +
  `SetSustainRelease` (commit 9a200a4), SF2 vendor `set_release_time_scale`
  (commit 1d11ee2, vendor patch, default 1.0 = file defaults). Restore as a
  GUI static setting "Pedal-up release: default / X ms", applies to both
  engines; never read CC64 value into it.

## 8. Bank 2 (knobs/pads) config — MidiSuite под Wine не работает
- Fader bank 2 передаёт события из коробки; knob bank 2 и pad bank 2 молчат
  (проверено aseqdump: клава не шлёт ничего на этих банках — пустые назначения).
- Назначения делаются только в MidiSuite (Win/Mac/Android). Под Wine приложение
  запускается, но Flutter-окно не создаётся (BLE/WinUSB-плагины) — тупик.
- Пути: телефон + приложение MidiSuite по BT, или Windows-машина/VM.
- После назначения CC на кнобы банка 2 — забиндить через MIDI Learn; пэды банка 2
  (фактически шлют ноты 74/77/81/83/84 на канале 1) — нацелить транспорт секвенсора.

## 8.1 Bank 2 config via SysEx — feasibility audit (repos cloned to ~/Projects/mvave-research)
- Transport from Linux: POSSIBLE. M-VAVE SysEx header F0 00 32 45..., handshake
  F0 00 32 45 00 00 00 40 7F F7, proven over standard USB-MIDI ports by
  smk37-firmware-custom-mod/tools/smk_ota_win.py (winmm → maps 1:1 to ALSA).
  MidiSuite mobile works over BLE-MIDI → config protocol rides as ordinary
  MIDI bytes (SysEx), no vendor USB transfers needed.
- MISSING: the knob/pad assignment message format is undocumented in both
  repos (fm1-custom-fw has FM-1 synth OTA/voice protocol only;
  smk37-firmware-custom-mod is BLE-firmware patches + audio-only ASIO driver).
- To obtain the protocol: capture MidiSuite traffic (Windows + loopMIDI, or
  Android BLE HCI snoop log while assigning a bank-2 knob/pad), then write a
  Python replay script via amidi/mido. Fallback: firmware RE
  (firmware_work/analysis/functions_catalog.csv + docs/FIRMWARE_STRUCTURE.md
  locate the USB-MIDI SysEx dispatcher in the AC791N image).
- Manual confirms: knobs/pads messages are configurable only via MidiSuite
  (smk37-firmware-custom-mod/firmware_work/manual_text.txt:115-121).

## 9. F2 (Tone) не реагирует на ITCH wheel и mod wheel
- Колёса (pitch/mod/itch) работают на синт-движок, но F2 (Master Tone) не
  реагирует на их движение. Пользователь ожидает связь колёс с F2.
- Уточнить у пользователя желаемое поведение при выборе решения.

## 10. Хай-хет: закрытый хет должен глушить открытый (choke)
- Сценарий: сыграл открытый хет → сыграл закрытый → открытый должен
  заглохнуть (естественное поведение живой ударки). Сейчас — не заглохает.
- В drum.rs есть CHOKE_GROUPS (crash chokes on retrigger) — проверить группы
  хетов (open HH slot 46, closed HH 42, pedal HH 44): вероятно, closed/pedal
  должны к choke-группе open HH, но с учётом того, что ЗАКРЫТЫЙ глушит
  ОТКРЫТЫЙ (обратное — нет).

## 11. Диалоги ремапа пэдов живут на вкладке Drums
- Remap-модалка (полная таблица) и диалог клика по нарисованному пэду
  рисуются внутри `draw_drum_sequencer` — на других вкладках клик по пэду
  молчит, диалог не открывается. Пользователь ждёт, что клик по пэду работает
  всегда (пэды видны на любом экране, они в нижней панели).
- Фикс: вынести обе модалки из `gui/drums.rs` в главный цикл отрисовки
  (`gui/mod.rs::update`, вызов с `ctx` вместо `ui.ctx()`); кнопка Remap
  остаётся на панели ударки.

## 12. set_sample_rate аллоцирует в аудио-колбэке (rt-assert abort под JACK)
- При старте под JACK/PipeWire stream отдаёт 48000 после старта → колбэк
  вызывает `synth.set_sample_rate()` → `Part/Voice::set_sample_rate`
  аллоцирует (RawVec). В debug-сборках с `rt-assert` это мгновенный abort
  ("memory allocation of N bytes failed"), в release — скрытая RT-наррушение.
  Воспроизводится каждый запуск, если запрошенный SR != SR устройства.
- Фикс: не применять SR в колбэке — как в ветке disconnect: atomic + полная
  пересборка потока вне колбэка (set_sample_rate в audio.rs:148 безопасен),
  либо сделать Voice::set_sample_rate аллокационно-чистым (перевыделять в
  trash/на control-потоке).

USB-конфиг (2026-10-28, проверено эмпирически): конфиг-протокол (00 59-фреймы)
по USB НЕ существует. Порты: Private (hw:2,0,0) = OTA/identity — на handshake
F0 00 32 45 00 00 00 40 7F F7 отвечает статичной identity-строкой, конфиг-фреймы
в любых обёртках (raw / F0 / 00 32 45-обёртка / после echo-back) молчат;
Master (hw:2,0,1) — обычный MIDI; hw:2,0,2 — мёртв. Конфиг только GATT AE40
по BLE (smk.py).

## 13. Живой перформанс ударки: банк-2 пэды как пульт секвенсора
Заявка пользователя (2026-10-28): «набить большой много-тактовый бит ИЛИ несколько
паттернов включать по банку 2 пэдов — нажал и включился другой; туда же запись
с пэдов и клавиш». Отраслевая практика (TR-8S/Elektron/MPC, см. анализ в чате
2026-10-28): переключение паттернов КВАНТУПОВАННО по границе такта; мьюты дорожек —
главный живой инструмент; филлы в соседних паттернах; song-chain для готовых сэтов.

Пререквизит: банк-2 пэды КОНФИГУРИРУЕМЫ с PC — живая карта адресов расшифрована
(см. ~/Projects/smk37_config/PADS_NOTES.md): цвет = 1467+74*(n-17) для пэдов 17-24,
2059+74*(n-25) для 25-32, нота = цвет-4, флаг LED обязан быть FF. smk.py пишет/читает.
Банк-2 пэды шлют ноты на канале 10 (у железа; проверено). У железа есть готовые
transport-ID нот: 81=record, 82=pause/stop, 83=play (видели в SysEx-контексте).

### 13.1 Выбор паттерна с банк-2 пэдов (ПРИОРИТЕТ 1)
- Прописать smk.py ноты на пэды 25-32 (например 40..47 или свободные GM).
- В синте: эти ноты (канал 10) маппить на `DrumSetPattern { idx }` — НО с
  квантованным срабатыванием: запомнить «pending pattern», применить в
  `StepSequencer::tick` на границе такта (step == 0), НЕ мгновенно.
- GUI: подсветить pending-паттерн до вступления.

### 13.2 Транспорт на банк-2 пэды (ПРИОРИТЕТ 2)
- Пэды 17/18/19 = Record / Play / Stop (ноты 81/82/83 или любые свободные).
- Rec уже есть (`DrumEngine::set_recording`), нужна только нотная маппина
  в `handle_event` (канал 10 → ControlEvent) + LED-обратная связь невозможна
  из банка 2 надёжно (LED красится, но мигание рек-пэда сделать нельзя —
  флаг статичен; ограничение принять).
- Запись: удары с пэдов (36-51) и клавиш уже пишутся в активный паттерн
  (`record_hit`, mod.rs:1581) — работает как есть после маппинга транспорта.

### 13.3 Длинный бит: pattern chain, НЕ расширение длины (ПРИОРИТЕТ 3)
- Решение: цепочка паттернов A×k1 → B×k2 → ... (song mode), т.к. расширение
  16→64 шагов ломает `DrumPattern` (массивы [Step;16]) и GUI-сетку 4×4.
- Реализация: `chain: Vec<(pattern_idx, bars)>`, по границе такта сдвигать
  индекс; Chain-режим переключаемый (off = свободное переключение п.13.1).
- Расширение длины отдельного паттерна отложить (length поле есть в
  `DrumPattern`, но сетка/редактор не готовы).

### 13.4 Мьюты дорожек на банк-2 (ПРИОРИТЕТ 4, дёшево и мощно)
- По опыту TR-8 сцены: один 16-шаговый бит + мьюты = целое выступление.
- Пэды 20-24 (5 свободных) = мьют кик/снейр/хэт/том/перк: инверсия
  слот-мьютов движка (поле в DrumEngine, гейт в trigger_slot + SF2-ветке).
- LED: приглушённый цвет = замьючено (перезаливка цвета smk.py при мьюте
  НЕ realtime — LED банк-2 красится только через конфиг-слот, а не по MIDI;
  значит GUI-индикация мьютов обязательна, LED не трогаем).

Замечание к 13.1/13.2: канал 10 у банка-2 может совпасть с чем-то в маппинге
ударки — ноты для маппинга выбрать вне 36-51 (или гейтить по «не drum-note»).

### Связанные находки аудита (docs/DRUMS_AUDIT.md, master 1dfc256^):
Critical 1 (identity-remap 0..15 в gui/mod.rs:1099/1606/1641, drums.rs:221) и
Critical 2 (set_sample_rate wipe, drum.rs:953) НЕ исправлены в master — чинить
до/вместе с 13.x, т.к. п.13 активно трогает те же файлы.

## 14. Живой перформанс синт-частей: сцены/мьюты/фейдеры с железа (2026-10-28, согласовано с пользователем)
Пререквизит общий с п.13: банк-2 пэды конфигурируемы smk.py (карта в
~/Projects/smk37_config/PADS_NOTES.md), фейдеры — записи 2647+5*(n-1), CC-байт
перезаписываем (заводские CC 64-71; 64 конфликтует с sustain — учесть!).

### 14.1 Сцены: банк-2 пэды -> Performance-слоты (ПРИОРИТЕТ 1)
- Сейчас перформансы переключаются только мышью. Мировая практика (сцены
  Ableton/Maschine, program change): смена звука без отрыва от игры.
- Пэды банка 2 -> загрузка Performance-слота (pad perf slots уже есть,
  есть ли Init см. п.3). Квантование не нужно (перф = мгновенный конфиг).
- Распределение пэдов банка 2 согласовать между 13.x и 14.x (16 пэдов всего:
  например 17-19 транспорт, 20-24 мьюты дорожек, 25-32 паттерны — тогда сцены
  на пэды... мест нет: либо разделить по «двойному нажатию», либо банк-2 = 8
  сцен + 8 драм-функций, зафиксировать раскладку в config).

### 14.2 Мьют/соло синт-частей на пэды (ПРИОРИТЕТ 2)
- GUI-мьюты есть, железного триггера нет. Пэды -> SetPartEnabled (мьют части
  A/B зон) и/или solo-логика (нажатый пэд = соло этой части).
- Индикация: GUI обязателен (LED банка 2 не управляется по MIDI — только
  конфиг-слот, см. п.13.4).

### 14.3 Фейдеры F1-F8 -> живые контролы (ПРИОРИТЕТ 3)
- Практики (Seed-to-Stage): главные параметры на физ. ручках. Маппинг
  (конфигурируемый, дефолт): F1=громкость активной части, F2=cutoff/brightness,
  F3=BPM секвенсора, F4=FX send, F5-F8=резерв. CC перезаписываются smk.py
  (записи 2647+5*(n-1), CC-байт 1), синту — обработчик CC -> ControlEvent.

### 14.4 Arp live-контролы: Latch/Hold + sync + transpose (ПРИОРИТЕТ 4)
- Практика Ableton/Logic: Latch = арп играет после отпускания клавиш; новый
  аккорд подхватывается без остановки; Hold-кнопка; rate sync к BPM секвенсора;
  transpose одной нотой/октавой. Арп и пресеты уже есть — добавить режимы.
- Latch-выключение: повторное нажатие кнопки или пустая пауза N тактов.

### 14.5 Mono Legato + Portamento (glide) для слоя (ПРИОРИТЕТ 5)
- Запрос пользователя: «нажимаешь вторую ноту — первая перестаёт, вторая
  без атаки плавно въезжает» = mono-режим (last-note priority) + legato
  (не ре-триггерить огибающую) + portamento (скольжение высоты, время N мс).
- Движок полифонический: сделать режимом слоя (per-part): mono switch,
  legato switch, glide ms. Применять до полифонического распределения войсов.
- Область действия: DSP-слой — нативно (legato+glide). SF2-слой — glide только
  через pitch bend канала (сэмпл ускоряется/замедляется — норм звучит); true
  legato на сэмплах невозможен (атака в сэмпле), эмуляция «доигрыванием»
  старого сэмпла на новой высоте — костыль, не обещаем.


