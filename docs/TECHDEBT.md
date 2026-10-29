# Tech Debt / Backlog

Recorded 2026-10-27. Ordered roughly by user-perceived impact.

## 1. Metronome tied to sequencer BPM
- Audible click (toggleable) synced to the drum-sequencer BPM; plus a visual click indicator in the GUI (step highlight already exists — add a downbeat accent + GUI flash).
- Click sound: synthesize a short tick in the drum engine (accent on beat 1) — no sample needed.
- Should follow the same BPM source as the sequencer + looper sync (`looper_sync_bpm`).

## 2. Sequencer records only one layer, not the whole performance
- Current: drum step sequencer records hits into the active drum pattern; part/looper recording targets a single layer (`active_part`). Expected: "record" captures the whole performance (all enabled parts), not just one layer.
- Needs design: multi-layer capture is what the looper already does (audio). Decide: extend looper to capture all enabled layers, or make the sequencer/PitchSeq record across parts.

## 3. Default empty init performance — РЕАЛИЗОВАНО (уточнено 2026-10-29)
Заявление было частично устаревшим: `preset::init_performance()` (одна часть
A1 = Init-пресет, полный диапазон, без SF2, split_point None) встроен в бинарь,
подаётся виртуальной записью "(Init)" в perf_list и грузится из комбо Layers,
из pad-perf-слотов и LED-консоли. Файл на диске не нужен: фабричные пресеты
тоже in-memory (load_all_patches). Добавлена кнопка Reset в perf-строке
Layers (план docs/superpowers/plans/2026-10-29-metronome-init-perf.md).

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

## 11. Диалоги ремапа пэдов живут на вкладке Drums — РЕАЛИЗОВАНО (2026-10-29)
Обе модалки вынесены в `gui/mod.rs::update` (рисуются на любой вкладке, ctx
корневой). План: docs/superpowers/plans/2026-10-29-console-gui-polish.md Task 1.

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

## 16. Компаньон LED-консоли: автозапуск из синта (заявка 2026-10-29)
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

## 17. LED-консоль: автовосстановление при потере каналов (BT клавы, BT/USB синта)
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

## 18. Аудио-вывод: автореспавн при hotplug устройств (наушники ↔ колонки)
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

## 19. Аудит производительности: DSP клипует, CPU до ~80% при игре synth-частью
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
(в) метроном: проверка — не считается ли sin/floor когда click_env == 0 (guard),
(г) denormals flush-to-zero (MMFTZ) — включён ли.

## 20. Ждёт пользовательской проверки (батч PR #5, 2026-10-29)
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

## 21. Размерность: лупер и секвенсор не должны быть прибиты к 4/4
Заявка пользователя (2026-10-29): «лупер и секвенсор не должны быть обделены
размерностью — например, чтобы 3/4 можно было делать».

Текущее состояние (проверить при планировании):
- Секвенсор: pattern.length существует (до 16 шагов), но GUI-сетка жёстко 4×4
  (16 шестнадцатых = один такт 4/4); 12 шагов = 3/4 технически достижимо длиной,
  но метроном акцентирует `step % 4 == 0` (усечёт 3/4 неверно), swing/quantize
  и record-путь привязаны к 16-шаговой сетке.
- Лупер: длина петли фиксируется при записи (в тиках), looper_sync_bpm и
  LED-консоль предположительно считают bars в 4/4.
Что сделать (нужен дизайн перед реализацией):
1. Метроном/акцент: настраиваемая длина такта (beats per bar) вместо `%4`.
2. Секвенсор: выбор размерности (3/4, 6/8, 5/4...) → length + шаг сетки в GUI.
3. Лупер: bars-индикация/синхронизация с учётом size.
4. Record-путь/квантование: привязка к длине такта, не к 16.

## 22. Запись с клавиатуры: метроном глохнет, банк ударных не пишется (репорт 2026-10-29)
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
- (a) метроном: при Rec авто-старте проверь set_playing/set_recording → не
  сбрасывает ли click_env/state; или drum-хиты маскируют клик по громкости
  (клик масштабируется drum_volume) — сравнить громкость до/во время записи.

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

## 24. Сустейн-педаль игнорирует root-only режим части (репорт 2026-10-29)
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
