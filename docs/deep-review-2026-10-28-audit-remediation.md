# Deep review: `audit-remediation` (158b612..38f2674, 2026-08-23 → HEAD)

Дата: 2026-10-28. Метод: 3 чанка × 2 ревьюера (regression hunter + change reviewer),
все якоря перепроверены главной сессией. Чит-лог чанков:

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

Не покрыто тестами: `tests/sustain_audit.rs:73` отпускает клавишу до pedal-up.
План не санкционирует этот кейс; комментарий в коде (mod.rs:1363-1366) признаёт
компромисс, но цель ремедиации была «залипшие ноты», а не «обрыв под клавишей».

Направление фикса: per-part `keys_down: [bool; 128]`, обновлять в note_on/note_off;
при pedal-up отпускать только голоса с выставленным `sustained_notes` (сохранить
очистку stale-флагов для невладельческих голосов).

## Findings (Medium/Low)

### 2. Отладочный `eprintln!` — `src/midi.rs:118,121` [CONFIRMED]

`eprintln!("[midi-in] ...")` на каждый CC/note, блокировка stderr на каждое
сообщение в потоках midir/JNI, заспамливает logcat. Остаток отладки → `log::debug!`.

### 3. Санитизация имён ломает совместимость — `src/preset.rs:20-39,44` [CONFIRMED]

Новая `sanitize_preset_filename` схлопывает все не-алфанумерические символы в один
`_`; старое правило сохраняло `- + . ( )`. Два следствия:

- (a) повторное сохранение старого пресета создаёт копию файла
  (`bass_lead_(user).json` → `bass_lead_user.json`), правки никогда не попадают
  в оригинал;
- (b) имена, различающиеся только пунктуацией, молча перезаписывают друг друга —
  проверки существования перед `write_atomic` нет. Противоречит инварианту
  «не перезаписывать пресеты».

Фикс: при сохранении — fallback на старую деривацию, если такой файл существует;
либо проверка коллизий перед записью.

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
Воспроизведение MIDI-файлов не задето (свой dispatch, mod.rs:2552+). Для пэдов
SMK-37 — намеренно; занести в TECHDEBT, не блокер.

### 6. Детект xrun мёртв на Linux/ALSA — `src/audio.rs:276` [CONFIRMED]

Детект — string match, но vendored cpal ALSA-host восстанавливает underrun'ы внутри
(`vendor/cpal/src/host/alsa/mod.rs:839`, `try_recover`) и не форвардит xrun в error
callback — фикс «xrun не роняет движок» — мёртвый код на основном Linux-backend.
JACK/PipeWire/AAudio — не проверено.

### 7. Paused-путь вне rt_guard — `src/audio.rs:176-187` [CONFIRMED]

Drain в paused-ветке выполняется до `rt_guard` (:188); полный 32-слотовый trash
делает drop на месте — free() в аудиопотоке, который `rt-assert` никогда не увидит.
Low (задокументированный fallback; строго лучше базы).

### 8. Out-of-range ProgramChange глушит часть 0 — `src/synth/mod.rs:1700-1713` [CONFIRMED, принято]

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

1. #1 — keys_down + регрессионный тест «клавиша зажата через pedal-up»;
2. #2 — eprintln → log::debug!;
3. #3 — совместимость деривации имён / проверка коллизий.
