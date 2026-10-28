# Deep review: `audit-remediation` (158b612..38f2674, 2026-08-23 → HEAD)

Дата: 2026-10-28. Метод: 3 чанка × 2 ревьюера (regression hunter + change reviewer),
все якоря перепроверены главной сессией. Дополнительная независимая проверка якорей
отдельной сессией того же дня: все находки подтверждены по сути; сместившиеся
якоря (#1 doc-комментарий, #5 dispatch, #8) поправлены в тексте ниже.
2026-10-28 (позже): независимый переаудит каждого пункта субагентами — все
находки CONFIRMED/PARTIAL, галлюцинаций нет; коррективы к планам фикса (#1, #2,
#3, #6) внесены в текст.
Чит-лог чанков:

- A — RT-safety / потоковая инфраструктура (trash.rs, audio.rs, metrics.rs, lib/main split, amidi.rs, settings.rs, looper display)
- B — движок: sustain/MIDI-роутинг/жизненный цикл голосов (synth/mod.rs, voice.rs, midi.rs, cc_map.rs, midi_player.rs, sampler.rs, vendor/rustysynth)
- C — слой приложения: пресеты/config/chord/GUI (preset.rs, config.rs, chord.rs, gui/*, key_action.rs)

Scope: ветка vs master включает июньско-июльскую историю Android/UI (не ревьюилась —
предшествующая работа). Цель ревью — окно аудита с f6d0977^ (2026-08-23) по HEAD.

## Verdict: DO NOT SHIP

Одно подтверждённое High-регрессия в профильной для ветки области (pedal-up рубит
ноты с зажатыми клавишами) плюс два Medium-замечания по гигиене.

## Blockers (Critical/High)

### 1. Pedal-up отпускает ноты с зажатыми клавишами — `src/synth/mod.rs:1374-1379` [CONFIRMED]

`set_sustain` при отпускании педали делает note_off **всем** активным не-releasing
голосам, включая голоса, чьи клавиши всё ещё зажаты. База (158b612, старый обработчик
CC64, ~строка 1610-1619) отпускала только голоса с флагом `part.sustained_notes` —
то есть те, чей key-off уже пришёл.

Регрессия: зажать C4 → нажать F1/CC64 → отпустить → C4 обрывается под зажатой
клавишей и не звучит до повторного нажатия. Стандартная артикуляция re-pedal сломана.

Не покрыто тестами: `tests/sustain_audit.rs:73` (`pedal_up_releases_held_notes`)
отпускает клавишу до pedal-up (NoteOff на :78, pedal-up на :80).
План не санкционирует этот кейс; doc-комментарий в коде (mod.rs:1369-1372) признаёт
компромисс, но цель ремедиации была «залипшие ноты», а не «обрыв под клавишей».

Переаудит якорей (2026-10-28, вторая сессия): подтверждены. Точное положение —
`set_sustain` на mod.rs:1373, безусловный `note_off` на mod.rs:1380-1385;
базовый обработчик CC64 — 158b612 `src/synth/mod.rs:1608-1619`.

Направление фикса: per-part `keys_down: [bool; 128]`, обновлять в note_on/note_off;
при pedal-up отпускать только голоса с выставленным `sustained_notes` (сохранить
очистку stale-флагов для невладельческих голосов).
Детали фикса (переаудит субагентом 2026-10-28): индексировать raw-нотой и матчить
голос по `src_note` (конвенция mod.rs:799,824); `keys_down` нужно чистить во всех
panic/reset-путях, где чистятся `note_stack`/`latch_held`/`sustained_notes`
(mod.rs:1722-1723, 1775-1778, 1865-1866, 1929-1931, 1952-1954, 2126-2129);
сохранить существующий sweep `sustained_notes`/`note_stack`/`latch_held` на
pedal-up (нужен ради mono/latch). Альтернатива в стиле rustysynth (per-voice
RELEASE_REQUESTED, release гейтится педалью — vendor/rustysynth/src/voice.rs:285-300)
для этой архитектуры сложнее из-за mono/latch-холдов.

## Findings (Medium/Low)

### 2. Отладочный `eprintln!` — `src/midi.rs:118,121` [CONFIRMED]

`eprintln!("[midi-in] ...")` на каждый CC/note, блокировка stderr на каждое
сообщение в потоках midir/JNI, заспамливает logcat. Остаток отладки → `log::debug!`.
Уточнение переаудита: RT-поток не задет (AMidi-путь идёт через `parse_midi_message`,
audio.rs:216, без eprintln). Но на desktop логгер не инициализирован вовсе
(env_logger в Cargo.toml нет, init_common/desktop_main не ставят реализацию) —
замена на `log::debug!` без init логгера в `desktop_main`/`run_headless` просто
спрячет диагностику.

### 3. Санитизация имён ломает совместимость — `src/preset.rs:20-39,44` [CONFIRMED]

Новая `sanitize_preset_filename` схлопывает все не-алфанумерические символы в один
`_`; старое правило сохраняло `- + . ( )`. Два следствия:

- (a) повторное сохранение старого пресета создаёт копию файла
  (`bass_lead_(user).json` → `bass_lead_user.json`), правки никогда не попадают
  в оригинал;
- (b) имена, различающиеся только пунктуацией, молча перезаписывают друг друга —
  проверки существования перед `write_atomic` нет. Противоречит инварианту
  «не перезаписывать пресеты».

Фикс (скорректирован переаудитом): «fallback на старую деривацию» из исходного
плана **опасен** — старое правило (до b89a6f3: `name.to_lowercase().replace(' ', "_")`)
сохраняло всё, включая `/` и `..`; b89a6f3 — это security-фикс path traversal,
его откатывать нельзя. Корректные варианты: (1) миграция при листинге — файлы,
чьё имя не совпадает с новой деривацией от `name` из JSON, переименовать в новый
формат (суффикс `-2` при коллизии); (2) collision-aware save — перед `write_atomic`
сравнить `name` в существующем JSON: равен → легитимный upsert своего файла,
не равен → ошибка или авто-суффикс. Голая проверка `path.exists()` не годится —
сломает повторное сохранение своего пресета.

### 4. Reconnect теряет контент лупера — `src/gui/settings.rs:275-278` [CONFIRMED]

Reconnect восстанавливает пресеты/состояние частей/CC-мапу/драм-кит, но не контент
лупера; записанный луп молча теряется при любом reconnect аудио
(`settings.rs:223-224` забирает пустой display из свежего движка). Пробел унаследован
от плана (в его Step 1 лупер не упомянут).

### 5. Транспортные пэды ch10 конфликтуют с GM2 percussion — `src/synth/mod.rs:1546-1567` [PARTIAL, High→Medium]

Пэды (64/81/82/83) перехватываются безусловно до драм-пути. 64/81/82/83 — звуки
GM2 percussion (Low Conga, Open Triangle, Shaker, Jingle Bell); живой ch10-вход с
настоящей percussion-контроллер переключает транспорт вместо звука. План фиксирует
дизайн, но аргументирует только диапазоном драм-слотов 36–51, не GM2-мапой.
Воспроизведение MIDI-файлов не задето (свой dispatch, mod.rs:2586-2627; в
исходном тексте стоял mod.rs:2552 — там LFO-код, якорь поправлен при переаудите).
Для пэдов SMK-37 — намеренно; занести в TECHDEBT, не блокер.

### 6. Детект xrun мёртв на Linux/ALSA — `src/audio.rs:276` [CONFIRMED]

Детект — string match, но vendored cpal ALSA-host восстанавливает underrun'ы внутри
(`vendor/cpal/src/host/alsa/mod.rs:836-840`, `try_recover`; второй невидимый путь —
poll-descriptors, alsa/mod.rs:759) и не форвардит xrun в error callback — фикс
«xrun не роняет движок» — мёртвый код на ALSA/PipeWire-через-pipewire-alsa.
Уточнение переаудита: **не мёртв на JACK** — vendored JACK-host шлёт
`"xrun (buffer over or under run)"` в error callback (`jack/stream.rs:459-460`),
а cpal собирается с `features = ["jack"]`. Оценка Low: observability, не
корректность (неудачный `try_recover` всё равно приводит к error-callback →
reconnect). AAudio — не проверено.

### 7. Paused-путь вне rt_guard — `src/audio.rs:176-187` [CONFIRMED]

Drain в paused-ветке выполняется до `rt_guard` (:188); полный 32-слотовый trash
делает drop на месте — free() в аудиопотоке, который `rt-assert` никогда не увидит.
Low (задокументированный fallback; строго лучше базы).

### 8. Out-of-range ProgramChange глушит часть 0 — `src/synth/mod.rs:1712-1724` [CONFIRMED, принято; якорь поправлен — было 1700-1713]

Очистка поднята выше bail по диапазону — несуществующий program теперь делает
note_off всем голосам части 0 (в базе путь был инертен). Осознанный
hang-prevention; Low.

### 9. Мёртвый API от reverted-фичи — `src/synth/voice.rs:515-517`, `envelope.rs:162-176` [CONFIRMED]

`note_off_for` без внешних вызовов (остаток отката непрерывной sustain-фичи 59ed413).
Удалить.

### 10. Прочее (Low, принято как доложено)

- CC121/124-127 теперь доходят до движка и могут попадать в MIDI-Learn биндинги
  (расширен фильтр в midi.rs);
- `write_atomic` без fsync — потеря при пропадании питания на SD-карте (Orange Pi);
- пользовательский перформанс, буквально названный «(Init)», становится
  незагружаемым из UI;
- `TogglePadPerf` без дефолтного кейбинда для пользователей с сохранёнными
  кейбиндами;
- whole-perf запись лупера упирается в потолок ~680 нот для 3-частей
  (4096 событий / per-part запись).

## Refuted by re-audit

Нет — галлюцинированных якорей не найдено; одна коррекция серьёзности (#5, High→Medium).

## Acceptance criteria (ключевые заявки)

| Заявка | Статус | Доказательство |
|---|---|---|
| Бинарная sustain + отвязка CC64 | MET | mod.rs:1367-1373; cc_map.rs:137-170; engine guard mod.rs:1799; GUI learn-reject |
| Сопоставление note-off по raw note | MET | voice.rs `src_note`; 22/22 sustain_audit |
| SF2 release scale | MET | sampler.rs:247,290 (re-apply при rebuild); vendor patch проверен; путь rebuild без теста |
| MIDI player note-off routing | MET | midi_player.rs:202-208; vel-0 нормализован |
| Пэд-транспорт / (Init) perf / whole-perf запись / chord display | MET | покрыты тестами |
| RT-safety | MET | rt_safety 4/4; paused-путь не покрыт (#7) |

Гейты (прогнаны ревьюерами): `cargo test` (86 pass), `rt-assert` suite,
`clippy -D warnings` — зелёные.

## Backward compatible

Частично. Чтение старых пресетов/перформансов/config: да (serde-дефолты, listing
директории). Повторное сохранение: нет (#3). Живое поведение sustain с зажатыми
клавишами: нет (#1). Desktop MIDI/Android пути: только compile-check, Android в
ревью не собирался.

## Out-of-scope additions (без блокировки)

`.bad`-backup конфига, DSP-load индикатор, релейбл F1–F4, перенос delay-mix
quick-контроля, guard `check_smf_header`, `PitchSequencer::adopt` — корректно,
никто не просил.

## Residual risks

- SF2-only sustain/panic пути верифицированы только чтением кода (нет SF2 sustain fixture);
- классификация xrun на JACK/PipeWire/AAudio;
- Android `#[cfg]`-пути и физический USB unplug;
- один flaky `--lib` failure, наблюдённый ревьюером один раз, не воспроизвёлся в 3 прогонах.

## Порядок фикса

1. #1 — keys_down + регрессионный тест «клавиша зажата через pedal-up» (чистить
   во всех panic/reset-путях, матчить по `src_note`);
2. #2 — eprintln → log::debug! + init логгера на desktop (env_logger в
   `desktop_main`/`run_headless`);
3. #3 — миграция-переименование при листинге + collision-aware save (fallback
   на старую деривацию запрещён — path traversal).



# Аудит подсистемы Drums — 2026-10-28

Ветка `worktree-drum-remap` @ 7b821b7. Якоря критических находок перепроверены
чтением кода. Вердикт: до мерджа чинить Critical 1–2.
2026-10-28: независимый переаудит всех пунктов субагентами — все находки
CONFIRMED, кроме П.11 (PARTIAL, см. уточнение); якоря дословно, смещений нет;
коррективы к плану починки внесены ниже.

## Critical

1. **Identity-remap в GUI закодирован как 0..15 вместо нот 36..51** — движок хранит
   полные ноты (`src/synth/drum.rs:910`, `apply_remap_vec` клампит 30..55 на
   `drum.rs:1026`), но GUI пишет индексы:
   - `src/gui/mod.rs:1606`, `:1641` (`*v = i as u8`) — загрузка кита без сохранённой
     таблицы / unload → все пэды съезжают на 6 полутонов (ноты 30..45); после unload
     ноты 30–35 молчат (`drum.rs:1031` режет `< 36`).
   - `src/gui/mod.rs:1099` — internal-kit restore: `.min(15)` схлопывает сохранённые
     ноты 36..51 → кламп до 30 → все пэды играют ноту 30.
   - `src/gui/drums.rs:221` — detach дубликата в однопэдной модалке.
   Эталон: per-kit load (`gui/mod.rs:1603`, clamp 30..55 + миграция `<16 → +36`) и
   Reset в полной модалке (`drums.rs:150`, `36 + i`).

2. **`DrumEngine::set_sample_rate` стирает всё состояние** (`drum.rs:953-962`,
   `*self = Self::new(...)`), вызывается из RT-callback (`audio.rs:190-194`) —
   remap/pad_vel/choke/sf2_mode/паттерны сбрасываются, GUI не перепушивает.
   Плюс аллокация в callback (techdebt #12). Десинк: sampler держит `drums_sf2`,
   движок сбрасывает `sf2_mode=false` → живые пэды SF2, step-секвенсор DSP.

## Important

3. Underflow `note - 36` в `src/synth/mod.rs:1581` при ремапе на цель 30–35
   (panic в debug, тихий дроп в release).
4. Step-секвенсор не играет ноты 30–35: `mod.rs:2715` строит ноту как `36 + slot`.
5. V127 и pad-vel оверрайд запекаются в запись паттерна (`mod.rs:1576-1584`) —
   теряется пер-степовская велосити.
6. `send_drum_state` (`gui/mod.rs:1490-1505`) не пушит choke-группы — покрытие
   reconnect только через хрупкую связку `send_initial_presets` → `load_sf2_drums`.

## Minor

7. Stale `group_last_note` после note-off — лишний note-off по мёртвой ноте
   (`drum.rs:999-1008`; note-off пути state не чистят).
8. Общий choke-стейт у живых пэдов, MIDI-файла и step-секвенсора
   (`mod.rs:1594/2597/2717`) — файл глушится живым пэдом; недокументировано.
9. MIDI-файловый путь не применяет ремап/pad-vel/V127 (`mod.rs:2593-2602`).
10. `SetDrumsSf2Mode` не сбрасывает choke-стейт и не глушит voices (`mod.rs:2190-2193`).
11. Два источника choke-таблицы: хардкод `CHOKE_GROUPS` (`drum.rs:18-35`, DSP) vs
    GUI-derived из имён сэмплов (`gui/mod.rs:1411-1428`, SF2). Расхождение:
    `china`/`ride` красятся как тарелки (`keyboard.rs:147`), группу получают только
    `crash`/`cymbal` (`gui/mod.rs:1417`).
    Уточнение переаудита (PARTIAL): ride/china-без-choke **совпадает** с
    GM-таблицей движка (ride в CHOKE_GROUPS тоже группа 0, china вне GM 36..51) —
    реальное расхождение только «раскраска обещает choke, которого нет» (UX,
    не функциональный баг). Нюанс: сэмпл с «china-crash» в имени группу получит.
12. Ключ конфига = stem SF2-файла (`gui/mod.rs:1430-1433`) — одноимённые киты из
    разных каталогов делят `drums_remaps`/`drums_vels`.

## Проверено, ок

Trash-паттерн DrumSetRemap/ChokeGroups/PadVel корректен (`mod.rs:2102-2114`);
`reset_choke_state` в обоих panic-путях (`mod.rs:1781`, `:2132`); sustain не гейтит
drum note-off (`sampler.rs:405-415`); bounds-guard'ы 26/16/30..55/36..51 согласованы
(кроме п.3); обе remap-модалки пишут одно состояние.

## Тесты

`tests/drum_remap.rs` (7), `drum_choke.rs` (7), `drum_pad_vel.rs` (7) покрывают движок.
Непокрыто: GUI-кодирование remap (п.1), `set_sample_rate` (п.2), record-путь (п.3-5).

## План починки (минимум до мерджа; скорректирован переаудитом субагентами 2026-10-28)

- П.1: 4 строки — `*v = 36 + i as u8` / `.unwrap_or((36 + i) as u8)` в
  `gui/mod.rs:1099,1606,1641`, `gui/drums.rs:221`; в 1099 дополнительно
  `.min(15)` → `.min(55)` (кламп вниз не нужен, верх нужен); тест: сериализация
  identity. Замечание: баг 1606/1641 сразу после unload триггерит underflow П.3.
- П.2: сохранять remap/pad_vel/choke/sf2_mode/fixed_volume в `set_sample_rate`;
  список исходного плана **неполон** — добавить `sequencer` (паттерны/bpm/swing/
  playing/recording — самое дорогое из стираний), `volume` и `group_last_note`
  (или `reset_choke_state`). Затронут только JACK mid-stream путь
  (audio.rs:190-194); reconnect-путь делает полный re-push и не задет.
  Альтернатива без re-push: in-place update `set_sample_rate` без `Self::new`
  (заодно закроет аллокации, techdebt #12). Триггер re-push из GUI (атомик-флаг
  SR change); rt_safety SR-путь не покрыт тестом — добавить.
- П.3: кламп `slot` из исходного плана **опасен** — кламп 30→36 записал бы хит
  в чужой слот 0 (тихая порча паттерна). Правильно: `note as i16 - 36` и дроп
  записи при выходе из 0..16 (зеркалит guard `record_hit`, drum.rs:805).
  «Ограничить ремап-цели 36..55» не годится — ломает легитимный SF2-кейс
  (`KIT_NOTE_LO=30` ради полной зоны сэмплов).
- П.5–6: решения требуются (семантика записи велосити — наивное «применять
  fixed_velocity на playback» затрёт ручные пер-степовские велосити;
  расширить send_drum_state: + `DrumSetChokeGroups` + `SetDrumsSf2Mode`,
  опц. `LoadDrumsSoundFont` из кэшированного Arc — заодно снимет зависимость
  reconnect от наличия SF2-файла на диске).
