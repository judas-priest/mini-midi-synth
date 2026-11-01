# Bass/Root Note Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-01 (база = origin/master после #33, worktree `~/Projects/mini_midi_synth-verify`)
status: executed 2026-11-01

**Goal:** Режим ноты парта вместо чекбокса Root: сегмент `[Poly|Bass|Root]` — Poly (все ноты), Bass (нижняя зажатая, прежний root_only), Root (честный корень зажатого аккорда через chord-detection; фолбэк на Bass, если аккорд не распознан).

**Architecture:** `Part.note_mode: u8` (0=Poly, 1=Bass, 2=Root) вместо `root_only: bool` (миграция: root_only=true → Bass). Гейты в `Part::root_on/root_off` (re-key инфраструктура RootDecision уже есть); Root-pc — новый `chord::detect_root` (общий скоринг с detect). UI: сегмент-контрол вместо чекбокса (research: взаимоисключающие режимы = сегмент, не чекбоксы; Nord Lead POLY/LEGATO/MONO style). Персист по образцу root_only: parts_state + PartConfig, с JSON-миграцией.

**Tech Stack:** Rust, egui 0.31.1. Research: chord-root скоринг — 12 ротаций + шаблоны (уже реализовано в `src/chord.rs::detect`), амбигвивалентные наборы (sus4/quartal/<3 нот) → фолбэк на bass note (music21-практика).

---

## Test Cases (from the requirement)

- TC1 (audio-тест, tests.rs): Root-режим, парт ноты C(48)+E(52): root=C; добавить A(45): held {C,E,A} → root=A → предыдущая C отпускается (re-key), звучит A(45). Снять A → остался C → рё-key на C.
- TC1b (audio-тест): низкая октава: root-pc выше raw (raw=1, chord pcs {9,1,4}) — нет паники/обёртки, target = saturating корректен.
- TC1c (audio-тест): верхняя нота не поднимает корень: звучит A(45), нажали C(72) → Suppress, корень остался 45.
- TC2: Root-режим, некордовый набор (2 ноты C+E без A): фолбэк на Bass — играет нижняя.
- TC3: миграция: конфиг/перф со старым `root_only: true` (без note_mode) → десериализуется в Bass; `root_only: false` → Poly.
- TC4: сегмент в парт-строке переключает режим, персистится в рестарте.
- TC5: Sus-чекбокс и Sustain-логика работают во всех трёх режимах (гейты не пересекаются).

## Negative Matrix

- zero/empty — ни одной зажатой ноты: root_held пуст → detect_root None → Suppress/Silence (как сейчас root_off без held).
- некордовый набор — фолбэк на Bass (явный код + TC2).
- raw < root-pc (низкая октава, корень выше по pc) — i16-математика + saturating_sub, без паники/обёртки (ревью Critical).
- корень выше последней нажатой (вверх-пере-ключ) — якорь min_held: корень не прыгает вверх; тест.
- duplicate/concurrent — повторный note-on той же ноты: Suppress (как в bass-режиме).
- legacy-конфиг без note_mode — serde default 0 + root_only-миграция (TC3).
- clock skew / timeout — N/A.

---

### Task 1: chord.rs — detect_root

**Files:** Modify: `src/chord.rs`

Вынести скоринг: `fn best_root(pcs: [bool; 12], bass_pc: u8) -> Option<(u8, &'static str)>` — кортеж (root, suffix) (detect строит имя из обоих). `detect` переписать через него (сохранить формат имени и slash-логику — тесты существующие не должны измениться). Новый:

```rust
/// Root pitch-class of the held chord (по тому же скорингу, что detect).
/// None: <3 distinct pcs или ни один шаблон не покрыл набор.
pub fn detect_root(notes: &[u8]) -> Option<u8> {
    ...тот же подсчёт pcs/bass_pc/count...
    best_root(pcs, bass_pc).map(|(root, _)| root)
}
```

Тесты: `detect_root(&[60,64,67]) == Some(0)`; `detect_root(&[64,67,72]) == Some(0)` (C/E → C); `detect_root(&[57,60,64]) == Some(9)` (Am → A); `detect_root(&[60,64]) == None`; `detect_root(&[60,65,69]) == Some(5)` (F/C → F).

- [ ] Step 1: тесты (красные — detect_root нет) → рефакторинг → зелёные; `cargo test chord`
- [ ] Step 2: commit `refactor: chord.rs — extract detect_root from detect`

### Task 2: Движок — note_mode + Root-гейт

**Files:** Modify: `src/synth/mod.rs` (Part, root_on/root_off, route_note_on комментарий), `src/config.rs`, `src/preset.rs` — миграция root_only

1. `Part`: заменить `pub root_only: bool` на `pub note_mode: u8,` (0=Poly, 1=Bass, 2=Root; константы `pub const NOTE_MODE_POLY: u8 = 0; ..._BASS: u8 = 1; ..._ROOT: u8 = 2;` в mod.rs). `Part::new`: `note_mode: 0`. ВСЕ использования `root_only` в synth/mod.rs обновить: `root_only` → `note_mode != NOTE_MODE_POLY` (grep ~6-8 мест: gate :957/:974, perf apply :1711/:3384/:3399/:3458, поле :535, ctor :585). Handler режима (вместо SetPartRootOnly-хендлера :2434-2441) — ЧИСТИТ root_held+root_active на КАЖДОЕ изменение (вкл. повторный клик по активному режиму — семантика существующего хендлера, тест root_only_toggle_resets_tracking).

2. `root_on` — ФИНАЛЬНЫЙ скетч (ревью-фиксы: БЕЗ аллокаций в аудио-потоке — стек-буфер вместо Vec; i16-математика — `raw - pc` в u8 ПАНИКУЕТ при raw<pc; якорь корня — min_held (басовый корень), не последний нажатый):

```rust
    fn root_on(&mut self, raw: u8) -> RootDecision {
        self.root_held[raw as usize] = true;
        if self.note_mode == NOTE_MODE_POLY || self.params.play_mode as u8 == 3 {
            return RootDecision::Sound { release: None, note: raw };
        }
        // Стек-буфер вместо Vec: root_on на аудио-потоке — аллокации запрещены
        let mut buf = [0u8; 128];
        let mut n = 0usize;
        for (k, &held_flag) in self.root_held.iter().enumerate() {
            if held_flag { buf[n] = k as u8; n += 1; }
        }
        let held = &buf[..n];
        let min_held = held.iter().copied().min().unwrap_or(raw);
        let target = match self.note_mode {
            NOTE_MODE_ROOT => crate::chord::detect_root(held)
                .map(|pc| min_held - ((min_held as i16 - pc as i16).rem_euclid(12) as u8))
                .unwrap_or(min_held), // фолбэк: некордовый набор = нижняя
            _ => min_held, // Bass: нижняя зажатая
        };
        match self.root_active {
            Some(cur) if cur == target => RootDecision::Suppress, // дубликат/не-корневая нота
            Some(cur) => {
                self.root_active = Some(target);
                RootDecision::Sound { release: Some(cur), note: target }
            }
            None => {
                self.root_active = Some(target);
                RootDecision::Sound { release: None, note: target }
            }
        }
    }
```

Семантика якоря (осознанно, ревью): target привязан к **min_held** — корень звучит в басовой октаве, никогда не прыгает вверх при добавлении верхних нот. Пример Am/C: hold 48(C)→Sound 48; hold 52(E)→fallback 48 Suppress; hold 45(A)→root A → target = 45−((45−9)%12)=45 → Sound {release:48, note:45} ✓. Нижняя октава: hold 33(A), затем 37(C#)→pcs{9,1} некордово→fallback 33; hold 40(E)→Am→target = 33 → Suppress (уже звучит) ✓. Raw<pc: raw=1, pc=9 → i16: (1−9).rem_euclid(12)=4 → 1−4 → i16 negative → as u8 wrap... ⚠ НЕТ: `min_held − 4` тоже может underflow при min_held<4 → финальная форма: `let delta = (min_held as i16 - pc as i16).rem_euclid(12) as u8; min_held.saturating_sub(delta)` — saturating_sub обязателен.

`root_off` — Root-ветка: снять raw, пересчитать target по оставшимся held (тот же стек-буфер + i16/saturating_sub); вернуть `Some(new_target)` если root_active изменился (route_note_off умеет re-key на любую ноту — проверено :1699-1726).

3. Миграция (config.rs PartUiState + preset.rs PartConfig): в ОБОИХ структурах поле `root_only: bool` → `note_mode: u8` с `#[serde(default)]`; ОБЯЗАТЕЛЬНЫЙ read-side маппинг legacy (ревью): в пост-десериализации/при чтении `if legacy_root_only && note_mode == 0 { note_mode = NOTE_MODE_BASS }` — иначе старые конфиги/перфы с root_only:true молча станут Poly. Практично: оставить legacy-поле навсегда (`#[serde(default, skip_serializing)] root_only: bool`), в save писать только note_mode. TC3.

4. GUI PartState: `note_mode: u8` вместо root_only; все использования (layers Sus/R блоки, persist/restore, push_full_part_state → `SetPartNoteMode { part, mode }` — НОВОЕ событие вместо SetPartRootOnly; handler в движке: `l.note_mode = mode` + при смене на Poly отпустить suppress-состояние — сущ. root-поля чистить как в drain-lite: root_active=None). perf apply (обоих пути) + perf save (PartConfig).

- [ ] Step 1: движок (enum/event/гейты), TC1+TC2 аудио-тесты в tests.rs (по образцу root_only-тестов), clippy + cargo test + rt_safety
- [ ] Step 2: GUI + миграция, TC3, clippy + cargo test
- [ ] Step 3: commit `feat: note modes — Poly/Bass/Root per part (chord-root detection, #24-семейство)`

### Task 3: UI — сегмент вместо чекбокса

**Files:** Modify: `src/gui/layers.rs` — парт-строка, вместо чекбокса Root (после Mute)

```rust
            // Note mode: Poly | Bass | Root — сегмент 22px (взаимоисключающие
            // режимы = сегмент, не чекбоксы; Nord POLY/LEGATO/MONO style)
            let modes = [("P", NOTE_MODE_POLY, "Poly — all notes"),
                         ("B", NOTE_MODE_BASS, "Bass — only the lowest held note"),
                         ("R", NOTE_MODE_ROOT, "Root — the chord's root note")];
            ui.add_sized([110.0, 22.0], |ui: &mut egui::Ui| {
                ui.horizontal(|ui| {
                    for (label, mode, tip) in modes {
                        let is_cur = self.parts[i].note_mode == mode;
                        let r = ui.selectable_label(is_cur, label)
                            .on_hover_text(format!("{tip}"));
                        // Повторный клик по активному — no-op (handler чистит
                        // root-состояние, рвать ноты на месте нельзя)
                        if r.clicked() && !is_cur {
                            self.parts[i].note_mode = mode;
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartNoteMode { part: i, mode });
                            self.persist_parts_state();
                        }
                    }
                });
            }).response;
```

(свернуть: `ui.add_sized` с closure возвращает InnerResponse — взять `.response`; ширина сегмента ~110px — проверить, что ряд вмещает; при переполнении уменьшить Vol-слайдер 90→80.)

- [ ] Step 1: правка, clippy + cargo test
- [ ] Step 2: commit `feat: [Poly|Bass|Root] segment in the part row`

### Task 4: Верификация и экран

- [ ] clippy/test/rt_safety; golden — не должен мигнуть (Init без root)
- [ ] `cargo run --release`; экран: TC1-TC5 + сегмент
- [ ] PR → squash merge → перезапуск release

## Out of scope

- Rename чекбокса-нейминга «Root» в JSON/доках — сегмент уже называет режимы правильно.
- Per-part octave (пэды/хоткеи) — TECHDEBT #47, отдельный план.
- GUI-кнопка октавы клавиатуры через smk.py — TECHDEBT #48, не делаем.
