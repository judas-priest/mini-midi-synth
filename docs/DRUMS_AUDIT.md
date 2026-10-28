# Аудит подсистемы Drums — 2026-10-28

Ветка `worktree-drum-remap` @ 7b821b7. Якоря критических находок перепроверены
чтением кода. Вердикт: до мерджа чинить Critical 1–2.

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

## План починки (минимум до мерджа)

- П.1: 4 строки — `*v = 36 + i as u8` / `.unwrap_or((36 + i) as u8)` в
  `gui/mod.rs:1099,1606,1641`, `gui/drums.rs:221`; тест: сериализация identity.
- П.2: сохранять remap/pad_vel/choke/sf2_mode/fixed_velocity в `set_sample_rate`
  + триггер re-push из GUI (атомик-флаг SR change); techdebt #12 остаётся.
- П.3: кламп `slot` или ремап-цели ограничить 36..55 для record-пути.
- П.5–6: решения требуются (семантика записи велосити; расширить send_drum_state).
