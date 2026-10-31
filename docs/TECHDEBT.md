# Tech Debt / Backlog

Recorded 2026-10-27. Ordered roughly by user-perceived impact.

## 1. Metronome tied to sequencer BPM — ГОТОВО (аудит 2026-10-31: клик из такта секвенсора, drum.rs:1238)
- Audible click (toggleable) synced to the drum-sequencer BPM; plus a visual click indicator in the GUI (step highlight already exists — add a downbeat accent + GUI flash).
- Click sound: synthesize a short tick in the drum engine (accent on beat 1) — no sample needed.
- Should follow the same BPM source as the sequencer + looper sync (`looper_sync_bpm`).

## 2. Sequencer records only one layer — ЗАКРЫТО решением юзера 2026-10-31 (аудит): драмы → сетка, остальное → лупер
- Current: drum step sequencer records hits into the active drum pattern; part/looper recording targets a single layer (`active_part`). Expected: "record" captures the whole performance (all enabled parts), not just one layer.
- Needs design: multi-layer capture is what the looper already does (audio). Decide: extend looper to capture all enabled layers, or make the sequencer/PitchSeq record across parts.

## 3. Default empty init performance — РЕАЛИЗОВАНО (уточнено 2026-10-29)
Заявление было частично устаревшим: `preset::init_performance()` (одна часть
A1 = Init-пресет, полный диапазон, без SF2, split_point None) встроен в бинарь,
подаётся виртуальной записью "(Init)" в perf_list и грузится из комбо Layers,
из pad-perf-слотов и LED-консоли. Файл на диске не нужен: фабричные пресеты
тоже in-memory (load_all_patches). Добавлена кнопка Reset в perf-строке
Layers (план docs/superpowers/plans/2026-10-29-metronome-init-perf.md).

## 4. Panorama: works for DSP parts, not for SF2 parts — ГОТОВО (аудит: SetPartPan → CC10 через set_channel_pan, применяется per-voice)
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

## 5. Chord detection for recording — НЕ СДЕЛАНО (аудит 2026-10-31: detect только на дисплее keyboard.rs:51)
- Problem: sequencer/looper records every played note; user wants to play a chord shape and have a single chord recorded (smart chord capture), not N separate notes.
- Research when picked up: chord detection algorithms (pitch-class-set → chord name/midi chord), how Smart Chord on M-VAVE / DAW chord tracks capture; likely: detect simultaneously-pressed notes within a window, collapse to one chord event.

## 6. Root-note-only mode for a layer (bass split) — РЕАЛИЗОВАНО (2026-10-29)
Per-part root-only gate (lowest-held-key priority, без chord-detection #5 —
не потребовался), SetPartRootOnly + PartConfig-персистенция + Root-чекбокс в
routing-строке; ключ-ап под педалью убивает бас (force-release). План:
docs/superpowers/plans/2026-10-29-console-gui-polish.md Tasks 2-3.

## 7. Custom sustain release (SR) — вернуть как статичную настройку — РЕАЛИЗОВАНО (2026-10-29)
- Research done 2026-10-27 (best practices: Ravenscroft 275, Ivory II, NI Claire).
- Industry standard: TWO-VALUE model — release while pedal held (native/long) vs
  release after pedal-up (user-configurable). NOT coupled to fader/pedal
  position live: scaling the tail by pedal position makes fast pedal taps chop
  tails and is unplayable (this exact failure was reverted in 59ed413).
- Half-pedal (continuous pedal scaling release+resonance) needs a continuous
  pedal with configurable Min/Max CC range — out of scope until such a pedal.
- Re-pedal (quick off→on re-catch) already works.
- Сделано: DSP `Envelope::note_off_for` + `SetSustainRelease` (сек, None =
  патчовый релиз), SF2 `SetSf2ReleaseScale` → sampler, оба проброшены в GUI
  (кнопка SR + слайдер, DragValue «SF2 rel»), персистенция в config.ui.
  План: docs/superpowers/plans/2026-10-29-console-gui-polish.md Tasks 4-5.

## 8. Bank 2 (knobs/pads) config — PARTIAL (аудит: закрыто обходным путём — python-конфиг ~/Projects/smk37_config; Wine-путь мёртв)
- Fader bank 2 передаёт события из коробки; knob bank 2 и pad bank 2 молчат
  (проверено aseqdump: клава не шлёт ничего на этих банках — пустые назначения).
- Назначения делаются только в MidiSuite (Win/Mac/Android). Под Wine приложение
  запускается, но Flutter-окно не создаётся (BLE/WinUSB-плагины) — тупик.
- Пути: телефон + приложение MidiSuite по BT, или Windows-машина/VM.
- После назначения CC на кнобы банка 2 — забиндить через MIDI Learn; пэды банка 2
  (фактически шлют ноты 74/77/81/83/84 на канале 1) — нацелить транспорт секвенсора.

## 8.1 Bank 2 config via SysEx — НЕ АКТУАЛЬНО (аудит: конфиг не rides MIDI — vendor GATT; SysEx-writer в Rust не нужен) (repos cloned to ~/Projects/mvave-research)
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

## 9. F2 (Tone) и колёса — ЗАКРЫТО (2026-10-29, не воспроизводится)
Проверка юзера: F2 работает с тоном (DSP и SF2), pitch bend -> высота,
mod wheel -> вибрато (на большинстве SF2). Проблемы нет — пункт устарел.

## 10. Хай-хет choke — ГОТОВО (аудит: DSP choke-группы drum.rs:1181 + SF2 sf2_choke_take, тесты drum_choke)
- Сценарий: сыграл открытый хет → сыграл закрытый → открытый должен
  заглохнуть (естественное поведение живой ударки). Сейчас — не заглохает.
- В drum.rs есть CHOKE_GROUPS (crash chokes on retrigger) — проверить группы
  хетов (open HH slot 46, closed HH 42, pedal HH 44): вероятно, closed/pedal
  должны к choke-группе open HH, но с учётом того, что ЗАКРЫТЫЙ глушит
  ОТКРЫТЫЙ (обратное — нет).

## 11. Диалоги ремапа пэдов живут на вкладке Drums — РЕАЛИЗОВАНО (2026-10-29)
Обе модалки вынесены в `gui/mod.rs::update` (рисуются на любой вкладке, ctx
корневой). План: docs/superpowers/plans/2026-10-29-console-gui-polish.md Task 1.

## 12. set_sample_rate аллоцирует в аудио-колбэке (rt-assert abort под JACK) — РЕАЛИЗОВАНО (2026-10-30, S1)
Вся цепочка `set_sample_rate` из колбэка (audio.rs:199-205) стала allocation-free:
FX ресайзят preallocated-буферы в capacity (SR_CAP=192 kHz), drum/voice обновляют
SR-поля явно, без `*self = Self::new`. Планы: docs/superpowers/plans/2026-10-30-refactor-s1-rt-safe-sr.md.
Принятое исключение: пересборка SF2-сэмплера в колбэке (sampler.rs:310-322, Displaced → trash) — отдельный TECHDEBT.

История (до фикса): при старте под JACK/PipeWire stream отдаёт 48000 после старта → колбэк
вызывает `synth.set_sample_rate()` → `Part/Voice::set_sample_rate`
аллоцирует (RawVec). В debug-сборках с `rt-assert` это мгновенный abort
("memory allocation of N bytes failed"), в release — скрытое RT-нарушение.
Воспроизводится каждый запуск, если запрошенный SR != SR устройства.

USB-конфиг (2026-10-28, проверено эмпирически): конфиг-протокол (00 59-фреймы)
по USB НЕ существует. Порты: Private (hw:2,0,0) = OTA/identity — на handshake
F0 00 32 45 00 00 00 40 7F F7 отвечает статичной identity-строкой, конфиг-фреймы
в любых обёртках (raw / F0 / 00 32 45-обёртка / после echo-back) молчат;
Master (hw:2,0,1) — обычный MIDI; hw:2,0,2 — мёртв. Конфиг только GATT AE40
по BLE (smk.py).

## 13. Банк-2 пэды как пульт секвенсора — PARTIAL (аудит: транспорт ГОТОВО; паттерны без квантования; chain НЕТ; слот-мьюты не на пэдах)
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

## 14. Сцены/мьюты/фейдеры с железа — PARTIAL (аудит: сцены+мьюты ГОТОВО через LED-консоль; solo НЕТ; фейдеры F5-F8 не забиндены)
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
— РЕАЛИЗОВАНА (частично) 2026-10-29: latch + panic-очистка + arp_latch парам; rate sync уже был безусловным (set_bpm из sequencer.bpm каждый блок, mod.rs tick_block); transpose арпа — отдельный пункт.
- Практика Ableton/Logic: Latch = арп играет после отпускания клавиш; новый
  аккорд подхватывается без остановки; Hold-кнопка; rate sync к BPM секвенсора;
  transpose одной нотой/октавой. Арп и пресеты уже есть — добавить режимы.
- Latch-выключение: повторное нажатие кнопки или пустая пауза N тактов.

### 14.5 Mono Legato + Portamento (glide) для слоя (ПРИОРИТЕТ 5)
— DSP-часть РЕАЛИЗОВАНА 2026-10-29: mono/legato/glide жили в patch params (play_mode 1/2, portamento_time/mode); добавлены SetPartMono/Legato/Glide live-события; исправлены баги (mono-путь игнорировал portamento_mode; retrigger_note не задавал porta_coeff — glide щёлкал мгновенно). SF2 legato/glide (skip-attack через startAddrsOffset, glide через pitch bend) — осталось, отдельный пункт.
- Запрос пользователя: «нажимаешь вторую ноту — первая перестаёт, вторая
  без атаки плавно въезжает» = mono-режим (last-note priority) + legato
  (не ре-триггерить огибающую) + portamento (скольжение высоты, время N мс).
- Движок полифонический: сделать режимом слоя (per-part): mono switch,
  legato switch, glide ms. Применять до полифонического распределения войсов.
- Область действия: DSP-слой — нативно (legato+glide). SF2-слой — glide через
  pitch bend канала (сэмпл ускоряется/замедляется — норм звучит); legato на
  сэмплах — ИНДУСТРИАЛЬНЫЙ приём «skip attack» (SFZ offset-опкод, Yamaha «_of»
  сэмплы, Kontakt sample start offset): legato-нота стартует не с нуля, а с
  offset за атакой + короткий кроссфейд со старой нотой (SFZ: off_time+attack
  blend). rustysynth умеет startAddrsOffset (vendor/rustysynth/src/
  instrument_region.rs:148) — legato-нота = voice со стартовой позицией
  start+offset; offset конфигурируемый per-кит/слой (дефолт ~10-30 мс, искать
  zero-crossing не обязательно, кроссфейд маскирует).



## 15. Банк-2 пэдов как динамическая LED-панель — РЕАЛИЗОВАНА v1 (2026-10-29)
Спека: docs/superpowers/specs/2026-10-29-bank2-led-console-design.md; план:
docs/superpowers/plans/2026-10-29-bank2-led-console.md (executed). Компоненты:
автомат src/gui/led_console.rs (мультитач, tap/hold), swallow нот в parse_and_push
(CONSOLE_TX), socket-клиент в GUI (led_companion toggle, alive/retry/heartbeat 2s),
компаньон ~/Projects/smk37_config/pad_console.py (троттлинг 60ms — прошивка дропает
быстрые записи). Ноты банка 2 перепрошиты (PADS_NOTES.md). v2-идеи (заявки 2026-10-29):
- ТРАНСПОРТ-ПЕРЕКЛЮЧАТЕЛЬ: 4-й пункт меню = toggle «нижняя строка слушает:
  драм-секвенсор / лупер» (это существующий seq_target — физические Seq-кнопки
  уже переключают его; LED-индикация цели на самом пэде: синий=драмы, зелёный=лупер;
  Undo/Play/Stop/Rec следуют цели)
- PAGE на пэд 29 (⏩): страницы перформансов/паттернов по 7 (28 = страница назад;
  лупер-rewind на холд Play или остаётся), индикация «есть ещё страница» = горит
- Мьюты зоны B, квантованное переключение паттернов, SCENE-режим, Redo

Реализовано v2 (план docs/superpowers/plans/2026-10-29-console-v2.md, 2026-10-29):
- ТРАНСПОРТ-ПЕРЕКЛЮЧАТЕЛЬ: меню-слот 3 = toggle seq_target (синий=драмы,
  зелёный=лупер, режим Menu сохраняется); пэд-транспорт 25/26/27 (ноты 82/83/81
  ch9) унифицирован с Seq-кнопками 123/124 и следует цели; нижняя LED-строка
  показывает состояние активной цели (лупер: Recording/Playing/Overdubbing).
- Мьюты зоны B: MUTE-режим = страницы по 7 частей (PAGE 28/29), окна 0-6 /
  7-13 / 14-15 с clamp — все 16 частей доступны с пэдов (закрыт гэп «part 8
  via GUI» и «части 14-16 GUI-only»). Отклонение от заявки: окна вместо
  zone-toggle на пэде 24.
- Redo: холд пэда 32 (>= 500 ms) = redo, тап = undo; пэд 32 белый (FFFFFF)
  когда redo доступен, розовый (FF5080) когда только undo. Отклонение от
  заявки: undo/redo срабатывают на РЕЛИЗЕ пэда (тап/холд различимы только
  по отпусканию); один уровень redo, деструктивные операции и DrumLoadKit
  инвалидируют stale redo.
Остались нереализованными: квантованное переключение паттернов, SCENE-режим.
Концепция (2026-10-28): банк-2 = Launchpad-style «мягкие клавиши»: пэд = вход в режим,
остальные пэды перекрашиваются в действия режима (BLE GATT), второй пэд = выход/меню.
Синт — источник правды о состоянии; ноты пэдов уже идут в синт (канал 10).

Разведка (сделано):
- LED красится записью color-слота БЕЗ commit — LED из RAM, flash не изнашивается,
  power-cycle откатывает к сохранённому (эксперимент на пэде 32, 2026-10-28).
  Карта адресов: см. ~/Projects/smk37_config/PADS_NOTES.md (1467/2059 + 74*(n-...)).
- MIDI-SysEx LED-канала в stock FW нет: Chocolate-style F0 00 32 ... по USB Private
  и BLE молчит (USB требует «enclosed URB» при живом официальном коннекте —
  github.com/cbix/mvave-chocolate-sysex). Краска только через GATT AE40.
- Community/официальные FW (genkigenki/Snoop37, FM1 Open SDK) — путь к кастомной
  прошивке с настоящим MIDI-LED, если понадобится мгновенная индикация; пока не нужно.

Осталось сделать:
1. ЛАТЕНТНОСТЬ ИЗМЕРЕНА (2026-10-28): 8 LED-записей подряд = 4-5 ms суммарно
   (~0.5 ms/write, write-without-response через BlueZ DBus) — перекраска банка
   мгновенная, человеческий глаз не различает. Пункт закрыт.
2. Архитектура красильщика: (а) компаньон-процесс с GATT-клиентом (синт шлёт ему
   «режим X» по IPC) — чище, но +процесс; (б) GATT-клиент потоком в синте (Rust,
   bluer crate) — без нового процесса, но BLE-стек внутри аудио-приложения.
   Решение (2026-10-28): компаньон-процесс (Rust + bluer) — синт остаётся лёгким,
   GATT/BLE-стек изолирован; IPC — простая строка «режим»/«действие» (unix socket).
   Синт — источник правды, компаньон — исполнитель краски.
3. Конечный автомат режимов + словарь раскладок (NONE/TRANSPORT/PATTERNS/SCENES/
   MUTES — см. 13.x/14.x), конфиг mode→пэды→действия в config.json.
4. Конфликт BLE-централизма: GATT-клиент красильщика и телефонный MidiSuite —
   один центр за раз; документировать, что телефон при этом отключён.

Дополнение #15 (2026-10-29, измерено юзером aseqdump): физические кнопки корпуса
шлют MIDI по BLE: Seq Play = NoteOn note 123 ch1 vel127, Seq Rec = note 124 ch1;
стрелки < > = вендор-SysEx F0 35 59 10 00 {7F|00} F7 (press/release, семья кнобов
банка 2, id=10). Кнопки Octave/Bank MIDI не шлют (локальные). Следствие: 123/124
надо отмаршрутизировать из «ноты канала 1» в мастер-транспорт (ControlEvent).

## 16. Автозапуск LED-консоли — ГОТОВО (аудит: spawn/kill/respawn c backoff, gui/mod.rs:664-713)
РЕШЕНО (2026-10-29): автоспавн реализован, см. план
docs/superpowers/plans/2026-10-29-companion-autospawn.md. Галка led_companion on →
синт сам спавнит компаньона (respawn с backoff 5с), off → убивает только своего
child'а; дубликаты разруливает probe-guard в pad_console.py.

Сейчас компаньон запускается вручную (`python3 ~/Projects/smk37_config/pad_console.py`).
Хочется: галка led_companion on → синт сам spawned'ит процесс компаньона; off → убивает.

Дизайн-заметки:
- spawn: std::process::Command::new("python3").arg(pad_console.py) — путь в config
  (led_companion_cmd), дефолт ~/Projects/smk37_config/pad_console.py.
- kill при off: child.kill() + wait (зомби); осторожно с SIGTERM → компаньон чистит
  сокет-файл по KeyboardInterrupt только — добавить обработку SIGTERM в компаньон.
- Двойной запуск: компаньон при старте должен проверить «уже запущен?» (pid-файл
  или попытка bind занятого сокета — EADDRINUSE → exit 0 тихо).
- Life-cycle (РЕШЕНО 2026-10-29, по заявке пользователя): автореспавн с backoff
  5с — если компаньон умер (краш/убит), синт поднимает его снова, пока галка on.
  Пульт должен «просто работать». Использовать уже существующий alive/retry
  механизм socket-клиента как триггер респавна.
- Не в v1: сейчас запуск вручную, это документировано.

## 17. LED автовосстановление — PARTIAL (аудит: наивный reconnect-цикл работает; DBus-watch/backoff из спеки — нет, помечено «позже»)
Сценарии разрыва (2026-10-29, заявка пользователя): отвалился BT клавиатуры (GATT
для компаньона и/или BLE-MIDI для синта); отвалился BT/USB аудио-синта (движок
пересобирается, ControlEvent-кольца и console_rx пересоздаются). Требуется:
консоль самовосстанавливается во всех комбинациях без действий пользователя
(автореспавн/переподключение, консоль продолжает работать слепо пока каналов нет).

ЧТО ДЕЛАТЬ ПОЗЖЕ. Сначала — погуглить лучшие практики восстановления BLE-MIDI +
GATT-коннектов в длинноживущих процессах (BlueZ reconnect patterns: коалесцcing
reconnect, exponential backoff, watchdog по notify-таймауту, Device1.Connected
properties-watch через DBus-signals вместо polling) и как это делают готовые
инструменты (bluetoothd поведение, companion-процессы в проектах типа MidiBridge,
Snoop37 editor). Затем спека: (a) компаньон: DBus-watch на Connected клавы,
reconnect-backoff, различать «клава off» vs «bluez stale cache» (см. PROTOCOL.md);
(б) синт: пересоздание console_rx/socket при reconnect_audio (сейчас console_rx
из CommonInit — проверить, что переподключение аудио не рвёт канал); (в) LED
после восстановления: heartbeat 2s уже перекрашивает всё — сверить.

## 18. Аудио автореспавн при hotplug — ГОТОВО (аудит: dead-stream detect + default-device poller + ReconnectPolicy backoff, тесты audio_hotplug)
Сценарий (2026-10-29, заявка пользователя): отключил наушники → включил обратно —
звука нет, помогает только рестарт приложения.

Что есть: разрыв СТРИМА ловится (audio.rs:292-293 → disconnected-атом → GUI
вызывает reconnect_audio, gui/mod.rs:462). дыра: ВСТАВЛЕНИЕ/смена default-device
не рождает ошибку стрима — cpal стрим продолжает «играть» в мёртвое устройство,
flag не ставится, reconnect не происходит. Плюс: если reconnect_audio падает
(устройства ещё нет), ретраев нет — один тост и тишина до рестарта.

ЧТО ДЕЛАТЬ ПОЗЖЕ. Сначала погуглить практики: cpal hotplug/device-changed
(dcpial нет единого API — проверять issue-трекер cpal), Linux: watch default
device через PipeWire/WirePlumber (pw-cli/pactl subscribe; DBus), кроссплат
poll-подход: раз в 1-2с сравнивать имя default_output_device + state стрима,
при изменении — reconnect_audio с retry-циклом (backoff, пока устройство не
появится). Затем спека: (a) poller в GUI-треде (не RT); (b) reconnect_audio
безопасно вызывать повторно (проверить идемпотентность — looper restore уже
умеет, см. PR #2); (c) выбор устройства: дефолт системы или из config.

РЕАЛИЗОВАНО 2026-10-29 (план docs/superpowers/plans/2026-10-29-metronome-hotplug.md):
GUI-поллер default-output-device (1.5 c, desktop; Android off — AAudio-роутинг),
ReconnectPolicy (Connected | Retrying{at, attempts}, backoff 2 c, тост только на
первой неудаче серии), AudioBackend.device_name. Подпункт (б) снят чтением:
reconnect_audio не пересоздаёт console_rx/LED-socket. Осталось: ручная
верификация hotplug по чеклисту плана; известное ограничение — ALSA-хост
(константное имя «default») поллером не отслеживается.

## 19. DSP клипует / CPU 80% — частично диагностировано REFACTOR_AUDIT (QW1/QW2 устранены — аллокации в колбэке были кандидатом); финальная проверка клиппинга не сделана
Репорт пользователя (2026-10-29): DSP загрузка ~80% и клиппинг при игре
synth-частью. УТОЧНЕНИЕ юзера: клиппинг был ЕЩЁ ДО метронома — это НЕ регресс
батча PR #4/#5, а накопленная нагрузка движка (подозреваемые в порядке
правдоподобия: per-sample математика голосов на 16 частях, denormals без
flush-to-zero, SF2/DSP суммирование, per-sample деления; батчевые фичи —
вторичный фактор, проверять во вторую очередь).

ЧТО ДЕЛАТЬ ПОЗЖЕ. Сначала гугл лучших практик профилирования RT-audio (perf,
flamegraph, criterion, RT-safety vs load; типовые причины клиппинга в
полифонических синтхах: denormals, per-sample div/sin, cache-miss voice layout),
затем: (a) измерить базу до/после батча на тех же пресетах (git bisect по
коммитам PR #4/#5 с perf-замером), (b) flamegraph горячего пути tick_block,
(в) метроном: ПРОВЕРЕНО 2026-10-29 — guard уже есть: sin/floor считаются только
при click_env > 0.0, env зануляется при < 0.001 (drum.rs:1218-1226 @ ecf67e7);
в покое cost = один f32-компарер на сэмпл. Оптимизация не требуется; полный
аудит (#19: flamegraph/denormals/bisect) остаётся отдельной работой,
(г) denormals flush-to-zero (MMFTZ) — включён ли.

## 20. Батч PR #5 (SR/Mono/Legato/ArpLatch) — всё реализовано и влито; зависло на юзер-проверке репортов с 2026-10-29
- **SR (pedal-up release)**: F1 Sustain ON → нота → клавишу отпустить → SR вкл +
  слайдер 0.10 → F1 OFF → хвост ~0.1 с. Юзер: «вроде не работает» — проверить и
  разобрать (возможно: F1 фейдер/кнопка не шлёт CC64 на нужный канал, или
  override не применяется к SF2 — SF2 хвостом правит «SF2 rel» отдельно).
- **Legato/Glide (DSP-only)**: play_mode Mono-ST + Portamento 0.2 — ноты
  перетекают без переудара. Юзер: «странно работает, не до конца правильно» —
  выяснить конкретный симптом.
- **Arp Latch**: чекбокс Latch в arp-секции параметров части. Юзер: «не нашёл» —
  указать точное место (params части → арп-секция), проверить сценарий
  аккорд→отпустил→играет сам.

## 21. Не прибиты к 4/4 — PARTIAL (аудит: сигнатуры+bars ГОТОВО; таймлайн лупера рисует бар-линии жёстко 4/4, drums.rs:1166)
Заявка пользователя (2026-10-29): «лупер и секвенсор не должны быть обделены
размерностью — например, чтобы 3/4 можно было делать».

**СЕКВЕНСОР — СДЕЛАНО (2026-10-29, план docs/superpowers/plans/2026-10-29-time-signatures.md):**
- `DrumPattern.beats_per_bar: u8` (2..=4, default 4, `#[serde(default)]` — старые
  kit-файлы читаются без миграции) + `ControlEvent::DrumSeqTimeSig` (кламп 2..=4,
  пишет оба поля: bpb и `length = bpb * 4`; MAX_STEPS=16, поэтому 5/4 и 6/4
  невозможны без расширения массива шагов — вне скоупа).
- GUI: кнопки 2/4 3/4 4/4 рядом с Steps; свободная длина (DragValue 1..=16) —
  ни одна кнопка не подсвечена.
- Причём сетка/record/undo/метроном УЖЕ были length-aware: сетка рендерит
  `0..pattern_length`, `record_hit` делит по `% len`, undo-снапшот копирует
  паттерны целиком, акцент метронома — `fired == 0` (начало такта = шаг 0 при
  любой длине). Попутно: `DrumSeqLength` в обработчике клампится `clamp(1,16)`
  (нулевой length из кольца = div-by-zero в `record_hit`).
- Известные ограничения (вне скоупа): импорт MIDI бьёт паттерны по 16 шагов
  4/4 (preset.rs, `DrumPattern::default()`); swing работает по чётным шагам
  и от длины не зависит; уменьшение length на лету может дать один переход
  без клика на некратном шаге (задокументированное поведение).

**ОСТАЛОСЬ — только лупер** (UX-вопрос, не синк): `loop_length_ticks` —
свободная длина в тиках, `looper_sync_bpm` гоняет только BPM, синк не ломается.
Не сделано: bars-индикация таймлайна (`looper_bars * 4` битовых линий — чисто
визуальный 4/4) и (опционально) bars-запрос длины при записи в не-4/4.

## 22. Метроном при записи / банк ударных в сетку — ГОТОВО (аудит: тесты metronome tc6-10, drum_record_remap)
Репорт пользователя: (a) включил секвенсор с клавы (Seq Rec) — начал писать,
метроном затихает; (b) переключил банк ударных — звуки играют, но в секвенсор
в реальном времени НЕ пишутся.

Гипотезы для диагностики (проверять при планировании):
- (b) главный подозреваемый — конфликт банков: банк-2 пэдов прошит нотами
  60–67/75–80 (консоль LED), эти ноты СЪЕДАЮТСЯ в parse_and_push (CONSOLE_TX)
  и до ударки не доходят — если «переключение банка» на клаве переводит пэды
  на консольную карту, играть ими в драмы/писать в секвенсор невозможно BY
  DESIGN. Решение — дизайн: либо переключаемый «режим пэдов» (консоль/драмы,
  см. транспорт-переключатель #15 v2), либо вернуть банку 2 драм-ноты и меню
  на другие пэды. Также проверить: не papered ли velocity-0 путь и route_note_on
  обёртки (root-only) в record-ветке для банк-1.
- (a) метроном: РАССЛЕДОВАНО 2026-10-29 (план
  docs/superpowers/plans/2026-10-29-metronome-hotplug.md) — движок чист:
  клик продолжает звучать при старте записи во всех путях (GUI DrumSeqRecord,
  клавиатурный note-83, авто-play из стопа, двойной toggle), rec-путь не трогает
  click_*/volume/phase (set_playing без reset, save_snapshot без click-поля).
  Покрытие: tests/metronome.rs tc6-tc10. Вероятные причины репорта: (1) соседние
  пэды 82 (play/pause) / 81 (stop) — без playing клик молчит by design;
  (2) маскировка: клик 0.55/0.9 × drumVol² против полноразмерных записанных
  хитов (отдельный «Metro volume» — возможный будущий UX-Item, не баг).
  П. (b) (#22) остаётся design-Item, не трогается.

РАССЛЕДОВАНО И ЗАКРЫТО (2026-10-30, план 2026-10-30-drum-grid-record-remap.md):
корень (b) — НЕ конфликт банков, а гейт записи в сетку по РЕМАПНУТОЙ ноте
`(36..=51).contains(&remapped)`: у Metal_Drums_v2 таргеты [30,38,51,48,…] вне
диапазона → record_hit молча скипался, хотя звук играл. Фикс: слот сетки =
исходная нота пэда (36-51), ремап = только звук. Тесты: tests/drum_record_remap.rs.
Лупер при этом пишет удары с тегом 254 (решение юзера: пишем и в сетку, и в
лупер, «ничего не убирать» — см. память master_transport.md).

## 23. GUI-пэды (нижняя строка) зеркалят LED-консоль — ПОКА НЕ ДЕЛАТЬ
Причина (юзер, 2026-10-29): физический переключатель банка MIDI не шлёт —
GUI не узнаёт о ручном банке, зеркало будет врать. Вернуться, когда появится
способ отслеживать банк (кастомная FW / SysEx-протокол).
Заявка пользователя (2026-10-29): при включённом компаньоне нижняя строка пэдов
в GUI должна отображать те же цвета и режим банка (HOME/PATTERN/MUTE-меню),
что и физические пэды клавы — чтобы GUI был зеркалом консоли.

Дизайн-заметки:
- Источник: `App.led_snapshot` (16×RGB, уже считается каждый кадр) + 
  `console.mode()` — рисовать bank-2 пэды в `keyboard.rs` этими цветами вместо
  статичной раскладки, подписать активный режим.
- ОГРАНИЧЕНИЕ: физический переключатель банка на клаве MIDI не шлёт (Octave/Bank
  немые кнопки) — GUI не узнаёт о ручном банке. Варианты: (a) считать банк
  синт-сайд: консоль активна ⇒ считаем банк 2; юзер переключил физически —
  рассинкрон, помечен как известный; (b) стикер на корпус. Документировать.
- Без компаньона — текущее поведение (статичные цвета по драм-категориям).

## 24. Сустейн × root-only — PARTIAL (аудит: DSP ГОТОВО с тестом; SF2 — под педалью re-key держит старый рут до pedal-up)
Репорт пользователя: при включённом root-only на части сустейн-педаль обходит
режим — подавленные ноты всё равно звучат (или старый рут висит).

Диагностика при планировании (все места, где педаль касается нот, должны идти
через route_note_on/off или учитывать root_active/root_held):
1. `set_sustain` pedal-up (mod.rs ~1503-1510): needs-bitmap группирует по
   `voice.note` (транспонированному) и Part::note_off_for/releases — педальный
   релиз НЕ проходит через route_note_off → саппресс-маски root не применяются.
2. SF2-часть: сустейн живёт в rustysynth (sampler.sustain_pedal) — root-only
   подавление на route_note_on распространяется на SF2? (note_on идёт через
   route ✓, но pedal-held off уходит в sampler.note_off напрямую).
3. Арп/лупер диспетчи через route ✓ (проверено в #6), но pedal-held
   переползание рута (re-key) при педали — см. force-release в route_note_off.
Тест: root_only + pedal: аккорд → отпустить всё при педали → звучит только рут
до pedal-up; pedal-up → тишина. Обе ветки (DSP/SF2).

## 25. Панель секвенсора ударки — прижать к низу окна (над GUI-клавишами и пэдами) — РЕАЛИЗОВАНО (2026-10-29)
Заявка пользователя (2026-10-29): панель секвенсора должна ВСЕГДА быть снизу,
над on-screen клавиатурой и пэдами, независимо от вкладки/скролла.
Реализовано: `TopBottomPanel::bottom("drum_seq_panel")` регистрируется ПОСЛЕ
keyboard_panel и ДО CentralPanel (mod.rs) — egui карвит панели по порядку
регистрации, поэтому секвенсор ложится ровно между основным полем и
клавиатурой/пэдами, на всех вкладках. План:
docs/superpowers/plans/2026-10-29-ui-batch.md Task 1 (гипотеза «клавиатура в
CentralPanel» не подтвердилась — она уже нижняя панель). Контракт зафиксирован
headless-тестом `tests/ui_layout_contract.rs`.

## 26. GM Instruments панель справа: ресайз должен сжимать основное поле, а не перекрывать его — РЕАЛИЗОВАНО (2026-10-29)
Заявка пользователя (2026-10-29): при изменении размера правой панели GM
Instruments основное поле должно пере-позиционироваться, а не перекрываться.

Находка при реализации (2026-10-29): панель УЖЕ была
`SidePanel::right(...).resizable(true)`, создана ДО CentralPanel — гипотеза
про `egui::Window` не подтвердилась; в этой конструкции перекрытие панель-
поверх-поля исключено геометрией egui (панели режут контент по своему rect,
«Always add any CentralPanel last»). Сделано: ширина переведена на современный
`.width_range(120.0..=260.0)`, контракт раскладки зафиксирован headless-тестом
`tests/ui_layout_contract.rs` (`gm_panel_resize_reflows_central`: +100px панели
= −100px центрального поля, 1:1). Если перекрытие воспроизводится на живом
столе вопреки контракту — это вне egui-геометрии: нужен скриншот и отдельный
разбор. План: docs/superpowers/plans/2026-10-29-ui-batch.md Task 2.

## 27. Нормальное имя приложения в title окна — РЕАЛИЗОВАНО (2026-10-29)
`ViewportBuilder::with_title("Mini MIDI Synth")` в run_gui (lib.rs, desktop-
ветка; egui-winit без него ставит «egui window»). app_id не тронут
("mini_midi_synth" — WM_CLASS/persist eframe). План:
docs/superpowers/plans/2026-10-29-ui-batch.md Task 3.

## 28. Seamless смена патча — ГОТОВО (аудит: LoadPatch без AllNotesOff, хвосты доигрывают; PR #6)
Отменяет требование e91a37b «после смены патча/программы ни одна нота не
звучит». Новая политика: хвосты доигрывают СТАРЫМ патчем (параметры скопированы
в голос на note_on), зажатые клавиши сустейнятся без re-attack; pedal-up /
key-off / CC120/123 / паника — прежние механизмы освобождения. Латч-mismatch
(latch→не-latch) отпускает только latch-голоса. Вор голосов предпочитает
release-фазные. Известные компромиссы: DSP-хвост перекрашивается mod-матрицей
нового патча; mono note_stack и SF2 latch-ноты переносятся через смену патча
без чистки (самозаживают); педаль-висяк при никогда не отпущенной педали — по
вине педали. Сценарий юзера: Carol of the Bells — смена перфа на границе такта,
колокольчики доигрывают, левая прыгает на октаву. План:
docs/superpowers/plans/2026-10-29-seamless-patch-switch.md; аудит семантики —
в диалоге 2026-10-29.

## 29. Смена перфа обрывает хвосты — PARTIAL по дизайну (совпадающие части seamless; SF2↔DSP дренит; crossfade — обсудить позже)
Семантика: если в новом перфе часть на месте зажатой ноты другая (SF2→DSP/Init) —
честный переход движка, дренаж по дизайну (обрыв). Если части совпадают — seamless.
Возможное будущее: «переходные хвосты для несовпадающих частей» (crossfade/SSS-lite) —
сложно, обсуждение с юзером, когда актуально. Записано 2026-10-29 после кейса
Carol→Halloween (SF2-колокольчики на part 5 → в Halloween part 5 = Init).

## 30. Дог лупера — PARTIAL (аудит: единый dock ГОТОВО; тоггл docked/in-tab НЕТ; полный юнит не собран — Swing/Vol/Remap/Kit I/O на вкладке)
СТАТУС (2026-10-31): объединённая docked-панель РЕАЛИЗОВАНА (драмы слева +
лупер справа, ui.columns, ScrollArea::both); мастер-транспорт в панели.
ОСТАЛОСЬ: тоггл docked/in-tab (config.ui), полный юнит (Swing/Vol/V127/Kit I/O
всё ещё на вкладке Drums — Metro/BPM переехали),Cycle-индикация сделана.
Заявка (2026-10-29, уточнение к #25): docked-панель снизу нужна была для ЛУПЕРА
(трек-визуализация, позиции слоёв), не только секвенсора. Секвенсор уже прибит
(TECHDEBT #25, PR #7). Сделать: looper-панель либо вторым TopBottomPanel::bottom
над клавиатурой (порядок регистрации: контент → лупер → секвенсор → клавиатура?),
либо объединить лупер+секвенсор в одну docked-панель с переключателем — решить
при планировании по фактической высоте экрана.
Дополнение (2026-10-29): docked-панели должны быть ПЕРЕКЛЮЧАЕМЫМИ — юзер хочет
вариативность: показывать лупер/секвенсор docked снизу ИЛИ как раньше внутри
вкладок. Настройка (config.ui + тоггл в GUI): «docked transport» on/off; при off
секвенсор возвращается на вкладку Drums, лупер — на свою вкладку. Дополнение 2 (2026-10-29): если docked — то ПОЛНЫЙ секвенсор в одном окне:
BPM, swing, громкость, метроном, размерность (2/4 3/4 4/4), паттерны, сетка,
транспорт, undo/redo — ВСЁ принадлежащее секвенсору в docked-панели, а не
размазано между вкладкой Drums и низом. Сейчас после #25-сплита: BPM/Swing/Vol/
V127/Metro/Kit I/O остались на вкладке Drums — это разрыв единого юнита.
ПЛАН ТРЕБУЕТ: глубокий аудит текущей разметки (какие контролы где после #25,
grep draw_drum_seq_docked / draw_drum_kit_controls в gui/drums.rs) + гугл
практик компоновки драм-машин (TR-8S layout, Elektron, Hydrogen GUI, egui
bottom-panel patterns) → дизайн самодостаточной панели. Связано с toggle
(docked/in-tab) из дополнения выше: обе раскладки показывают ПОЛНЫЙ набор.

## 31. UX-консолидация — PARTIAL (аудит: Layers-redesign ГОТОВО; единое хранилище/undo живых действий/discoverability — нет)
Пункт 1 ПРИНЯТ: единое хранилище состояния частей (mute/root/volume/transpose —
один источник правды), Layers/консоль/перф = окна к нему + быстрые кнопки.
Чинит класс конфликтов «перф затёр живой мьют». Технически — рефакторинг
состояния, пользовательски — починка UX (один «кто главный»).

Остальные пункты концепта (кратко, каждый раскрывается при очереди):
2. Сокрытие фич: функции спрятаны за модалками/деревом (latch юзер «не нашёл»).
   Названия юзера устраивают. Решение — гуглить UI/UX-практики discoverability
   (инвентарь функций, поиск по функциям, progressive disclosure, hover-подсказки,
   паттерны DAW-интерфейсов) при планировании пункта.
3. Состояние размазано по 5 поверхностям с разными соглашениями. Принцип
   LED-консоли («цвет = состояние, одна поверхность статуса») распространить
   на GUI-пэды/сетку/панели (#23 — отложен из-за невидимого банка, вернуться).
4. Жаргон в UI — см. п.2.
5. Три редактора с пересекающимися ролями (GM-панель/Layers/дерево параметров).
   ЯКОРЬ (юзер, 2026-10-29): страница перфов и партов — САМОЕ ПОНЯТНОЕ, что есть,
   её не трогать и строить консолидацию вокруг неё. Клеймо: что не относится к
   раскладке — убрать из перф-страницы в «звук» (GM-панель+дерево объединить:
   пресет+огибающие+фильтры+эффекты одной части на одном экране).
   УТОЧНЕНИЕ (юзер, 2026-10-29): выбор инструмента части (SF2 program / DSP пресет)
   должен быть В СТРОКЕ ЧАСТИ в Layers (где инструменты и есть) — «Mode: SF2:
   SC-55 | программа» в дереве параметров неинтуитивно (кто догадается, что это
   переключение движка). GM-панель остаётся браузером библиотеки; применение —
   в Layers; контрол «Mode SF2» в дереве отмирает (входит в выбор инструмента).
   ДВИЖОК ЧАСТИ (DSP/SF2): кнопка-тумблер В СТРОКЕ ЧАСТИ (вариант А, выбран юзером
   2026-10-30), не через список инструментов. Рядом теги состояния
   («SF2: SC-55 prog 1» / «DSP»). Макет: docs/mockup_layers.html.
   СТРУКТУРА ЭКРАНА: Layers = зона левой руки (части A1–A8) + зона правой руки
   (B1–B8) на одном экране, парты внутри зон. Ноль потерь от перехода на
   инспектор-паттерн (все инструменты видны в строках, прослушка и сравнение
   были недоступны и раньше).
   Layers redesign РЕАЛИЗОВАН (2026-10-30): все части строками (две зоны,
   макет docs/mockup_layers2.html), выбор инструмента (SF2 program / DSP
   пресет) и кнопка DSP/SF2 — в строке части; вкладки удалены
   (commit af904e8).
6. Undo только у секвенсора. Живые действия (патч/транспон/мьют) не откатываются —
   добавить undo для раскладки/живого.
Целевая структура: экран «живое» + экран «раскладка» + экран «звук» + LED-консоль
как зеркало «живого». Объём ≈ большой батч.

## 32. Мастер-транспорт — почти ГОТОВО (аудит: Phases 1-4 сделаны; остался Phase 5 — смена паттернов на границе + чейнинг; отложены: вариант Б, MIDI-clock slave, half/×2)
Юзер (ключевой инсайт): запись в СЕКВЕНСОРЫ почти бесплатна — пустота не пишется,
всё ходит по сетке, выравнивание бесплатно. Аудио-квантование лупера НЕ НУЖНО.
База: у нас уже есть драм-паттерны (16 слотов) И питч-секвенсор на каждую часть
(SeqSetStep/Length/Rate/Scale). Инфраструктура есть, дописать логику.

Фазы:
1. GUI: ОДНО поле BPM (Sync-логика всегда on; раздельный BPM — убрать).
   Мастер-транспорт: Play/Rec/Stop (пэды 25/26/27) управляют драмами + лупером
   + записью питч-секв вместе.
2. ЗАПИСЬ ПЕРФОРМАНСА: Rec — армит ВМЕСТЕ: драм-паттерн + питч-секв всех
   включённых частей; Play — всё воспроизводит. По партам с мьютами.
   Инструмент части меняется задним числом (события переигрываются текущим
   пресетом — кусочек «Б» бесплатно). Компромисс: квант в 16-е, моды/хвосты
   не пишутся (это остаётся за аудио-лупером).
3. КВАНТОВАНИЕ ЛУПЕРА: уже существует (looper_quantize Off/1/4/1/8/1/16 в
   gui/drums.rs) — проверить работу в связке с мастер-транспортом, вывести
   в docked-панель.
4. УПРОЩЕНИЕ PAD-МЕНЮ: раз логика лупер/секвенсор объединяется — pad-меню
   (транспорт-переключатель из #15 v2) УПРОСТИТЬ/пересмотреть.
5. ПЕРЕХОДЫ ТАКТОВ (квантированные переходы паттернов): переключение паттернов
   НА ГРАНИЦЕ ТАКТА (нажал Pat 3 — вступит с нового такта) + возможный чейнинг
   (A→B→A). Названия в драм-секвенсоре: pattern length/last step — при
   планировании сверить терминологию.

Аудио-лупер остаётся для НЕ-нотного контента: свободные фразы мимо сетки,
внешнее аудио, хвосты-пады. Free-режим сохраняется.
Связано: #30 (docked-панель лупера+секвенсора — туда мастер-транспорт),
#21 (размер), #18 (hotplug — сделано).

Phase 1 (clock): ВЫПОЛНЕНО (S5, 2026-10-30) — один канонический BPM
(drum sequencer), лупер синхронизируется per-block (bpm + beats_per_bar),
смена темпа mid-loop рескейлит луп (regrid), LooperSetBpm /
looper_sync_bpm / второе BPM-поле удалены. MIDI-clock (0xF8/0xFA) не
реализован вовсе (0 вхождений) — осознанно вне S5: внешний clock-slave
ввёл бы второй источник темпа; решать внутри #32 при дизайне
мастер-транспорта.

Phase 2 (master transport, вариант А): ВЫПОЛНЕНО (2026-10-30) — MasterPlay/
MasterStop/MasterRecord; seq_target=2 MASTER (новый дефолт); пэды 82/83/81 и
клавиши 123/124 при target=2 управляют драмами+лупером вместе. Запись
перформанса = лупер (Mix — все части, квант 16-е, реплей текущим пресетом,
парт-мьюты) — юзер выбрал вариант А 2026-10-30: питч-секв остаётся живым
модулятором.

Phase 4 (упрощение pad-меню): ВЫПОЛНЕНО (2026-10-30) — transport-ноты
(пэды 81/82/83 ch9, клавиши 123/124) ВСЕГДА управляют master-транспортом;
переключатель seq_target (LED menu slot 3, цикл 0→1→2) удалён. seq_target
сужен до drum-note mode (0 = пэды 36-51 триггерят ударку на любом канале),
переключение — вкладка Drums / headless ToggleDrumsSynth (flip 0↔1).
GUI: одна транспортная строка в docked-юните (Master Play/Rec + Undo/Clear
секв + L-Undo лупера + BPM + Metro + beat-flash); строки Play/Rec драмов и
лупера удалены; state/progress/Clear лупера сохранены.

Phase sync (квантованная запись лупера, best practice SooperLooper/Boss-RC):
ВЫПОЛНЕНО (2026-10-30, план 2026-10-30-quantized-master-rec.md) — MasterRecord
при играющем секвенсоре: старт захвата лупера на следующей границе паттерна
(current_step==0), авто-закрытие после Bars тактов, досрочное второе нажатие
закрывает тоже на границе. Free-режим (секвенсор стоит) — прежнее немедленное
поведение. GUI: ARMED-статус. Отвергнутая альтернатива: Bars = длина паттерна
(Bars остались ручными).

Multi-cycle → Continuous capture (2026-10-31, ФИНАЛ — дважды пересмотрен
юзером; финал = гибрид DAW-практики, НЕ Roland): квантованный захват
БЕЗОСТАНОВОЧНЫЙ — Rec → ARMED → старт на границе такта → пишется без
авто-закрытия; лупер держится с запасом (cycle*64), тики записи глобальные
непрерывные (wrap/релокация не нужны). Кнопки «+1 bar»/«−1 bar» во время
записи: растянуть/урезать петлю (−1 откатывает и курсор записи — цикл можно
переиграть). Rec → закрытие на ближайшей границе, петля = целые циклы.
Rec после закрытия = НОВАЯ запись (looper.clear + свежий арм — Ableton
Session practice), наслоение = кнопка «Layer» (MasterOverdub, границей
выровненный overdub, длина петли не меняется). Индикация: master_rec_state/
cycle атомы → ARMED / REC k / END k + прогресс внутри цикла; Loop: N st —
честная длина (loop_steps атом). Мультитрек: пишется только реально
звучащая часть (route_note_on -> bool), offs фильтруются по active_notes.
Q = квантование нот по сетке только во время записи. Принято (не чинить):
BPM-смена посреди захвата устаревает cycle_ticks; drift-correction (seek на
границе) только для петель = 1 цикл; Bars — только free-режим (секвенсор
стоит, петля = bars тактов).

ОТЛОЖЕНО (не терять):
- Идея «длина = сколько сыграл» (Ableton-style) — РЕАЛИЗОВАНА в continuous
  capture (закрытие по Rec, целые циклы) — пункт снят 2026-10-31.
- Расширение/урезание ГОТОВОЙ петли (½/×2 длины, Ableton Looper half/double) —
  не путать с +1/−1 bar во время записи (уже есть).
- Вариант Б (#32 фаза 2 буквально): питч-секв как standalone-плеер нот
  (anchor-нота, триггер голосов из tick_block, синхронизация позиции с
  мастер-транспортом). Большая движковая работа, риск для живого мод-режима.
- Фаза 5: переходы паттернов на границе такта + чейнинг.
- #30: docked-панель лупер+секвенсор с тогглом docked/in-tab и полным юнитом
  (BPM/Swing/Vol/Metro/Kit I/O из вкладки Drums) — отдельный план; Master-строку
  из Task 2 туда перенести.

## 33. Слайдеры L/T/D/P в драм-сетке не работают в SF2-режиме — ЗАКРЫТО (UX-fix, 2026-10-30)
Расследование: слайдеры исправно пушат DrumSetParam, движок применяет их в
drum_engine.params — но это DSP-only параметры по дизайну. В sf2_mode тик
секвенсора (drum.rs DrumEngine::tick) не вызывает trigger_slot и не делает
level/pan-микс, а возвращает хит вызывающему (DrumTriggers) →
sampler.drum_note_on = обычный MIDI note-on в SF2; ни level, ни tune, ни
decay, ни pan туда не доходит (drum_note_on принимает только note+velocity).
Сломано не было никогда — SF2-драмы добавлены позже слайдеров и эти параметры
не потребляли. Live-пэды в sf2_mode идут тем же путём (synth/mod.rs
sampler_drums-ветка). Фикс UX: в docked-панели (gui/drums.rs) слайдеры
серые с тултипом «DSP only, no effect in SF2 mode», когда включён
SF2 Drums (sf2_drums_enabled + загруженный кит). Тесты: tests/drum_slot_params.rs.
Если когда-нибудь захочется L/T для SF2 — это per-note tuning/velocity-гейн
в sampler.drum_note_on, отдельная работа.
Дополнение (2026-10-29, юзер не понял контрол): выпадашка «Part: A1 …» в
строке Bars = цель записи лупера (какую часть пишем в петлю; при записи — красный
«Rec: …»). UX-мусор: выглядит как селектор части, а является «Rec to».
Пересмотреть в редизайне лупер-панели: (а) переименовать в «Rec to: …»,
(б) судьба зависит от #32 — если будет запись всего перформанса (#2), селектор
отмирает; если по-частям — оставить с явным именем.
### #30/#34 — согласовано (2026-10-29): лупер — юзер хочет глушить и ПО СЛОЯМ (уже есть), и ПО ПАРТАМ (доделать, легко).
Делаем: part-mute (фильтр воспроизведения по part_id) + GUI/LED (мьют по парту).
Дальше стоп (редактирование нот = DAW-граница, не делаем). Overdub = отдельный
слой, глушится отдельно (уже есть). Одно поле BPM при Sync (мелкий GUI-фикс,
вместе с этим).

### #14.3 фейдеры F1–F8 — отложено (2026-10-29, юзер: «сложно, оставим»)
Маппинг фейдеров на функции (кандидаты: F3=громкость лупера, F4=громкость
ударки, F5–F8=эффекты/части) — вернуться при актуальности. F1 (сустейн) и
F2 (тон) уже работают.ДИЗАЙН docked-панели (2026-10-29, нагуглено+согласовано направление, пока ТОЛЬКО записано):
Принципы: TR-8S «один knob — одна функция» (живое = мгновенное, без меню); progressive
disclosure (Nielsen): показывать только нужное СЕЙЧАС; разделение ролей — LIVE
(инструмент) vs SETUP (настроил и забыл); Elektron menu-diving = антипример.
Уровень LIVE (docked-панель, всегда видна): транспорт Play/Stop/Rec/Undo/Redo,
BPM (одно поле), Metro, паттерны 1–8 + сигнатура 4/4 3/4 + Steps, лупер: Bars/
Quantize/Rec-state/Source, мьюты.
Уровень SETUP (вкладка Drums, остаётся): Kit save/load, MIDI import, Remap,
Swing, слайдеры L/T/D/P (DSP-only), vel ranges, pan.
Без потери функциональности: ничего не удаляется, раскладка по частоте
использования. Дополнения #30 выше (looper docked, тоггл docked/in-tab, полный
юнит) — входят в этот дизайн.
## REFACTOR_AUDIT (2026-10-30) — статусы по аудиту 2026-10-31: ГОТОВО QW1/QW2/QW3/QW6/QW8/QW9/QW10(код), S0, S1, S5; НЕ НАЧАТО QW4/QW5/QW7, S2/S3/S4/S6/S7/S8; QW10 хвост — carried-список не вычищен
6 аудиторов + сверка 2-й итерацией. Quick wins (RT/GUI perf + латентные баги):
QW1 wavetable alloc на каждый note-on (8KB+13k sin, CONFIRMED), QW2 LFO LUT
ленивая аллокация на аудиопотоке, QW3 drain-хелпер + root_held ghost-rekey,
QW6 looper атомы на каждый сэмпл (48k div/s), QW8 dead code sweep, QW9 config
save на каждый кадр при драге. Strategic: S0 characterization-тесты (блокирует
S1-S5), S1 RT-safe set_sample_rate (=старый #12), S2 реестр параметров (7-8
мест на параметр), S3 консолидация FX, S4 god-файлы, S5 clock-sync лупера
(ПЕРЕД #32), S6 dsp_utils dedup, S7 engine handles, S8 pitch-bend routing.
Файл: REFACTOR_AUDIT.md (корень). Быстрые QW1/QW2 — кандидаты на причину
клиппинга #19 (аллокации в колбэке = dropouts при игре wavetable-патчей).

## 34. Окно instrument browser: одна позиция + запоминание (2026-10-31, юзер)

Модалка выбора инструмента открывается «каждый раз как бог на душу положит» —
позиция/размер окна каждый раз новые, непрактично. Хотелка: одно место
открытия + запоминание позиции/размера (per-window egui state уже сохраняется
автоматически, если не сбрасывать id; вероятно ломается тем, что окно
пересоздаётся с другим title per-part — «GM Instrument — Part A1» vs «A2» —
у каждого свой state). Сделать: стабильный Id окна (не зависящий от парта),
одна общая позиция; опционально — персист позиции в конфиг. Позже, не в
раунде 3.

## 35. Клавиатурная навигация в модалках/настройках: стрелки + Enter (2026-10-31, юзер)

В «Patch — Part 1» (и во ВСЕХ модалках и в настройках) выбор должен работать
со стрелок и Enter: ↑/↓ — перемещение по списку, Enter — выбрать, Esc —
закрыть. Сейчас только мышь. egui: memory().focus + Event::Key — сфокусировать
первый selectable после открытия, двигать индекс, Enter → picked. Касается:
instrument browser (DSP + GM деревья),
 Pad Remap, combo-popups перфов, TextEdit-форм (Perf name, kit save).

## 36. Отступ над кнопкой «+ Add part» (2026-10-31, юзер)

В списке партов кнопке «+ Add part» не хватает воздуха сверху — прижата к
последней парт-строке зоны. Сделать: `ui.add_space(theme::SP_SM)` перед
кнопкой (обе зоны, layers.rs draw_part_rows). Мелочь, в ближайший UI-раунд.
