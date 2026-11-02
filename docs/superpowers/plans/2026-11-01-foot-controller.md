# Foot Controller Features (#5, #35 tail) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox syntax.

**Status:** ОЖИДАЕТ ЖЕЛЕЗА — футконтроллер заказан 2026-11-01, придёт ~2026-11-14. Не исполнять до прихода/настройки педали.

**Goal:** Педаль = hold-модификатор записи аккордов (#5) + педаль/пэды для навигации (#35 хвост).

**Architecture:** MIDI-события педали (note/CC, настраивается MIDI-learn'ом) → GUI-флаг `chord_capture_hold`; при held — играемый аккорд (chord::detect по зажатым нотам) пишется в лупер как одна нота-корень (или все ноты — решить по факту железа). #35: если педаль шлёт CC — забиндить в cc_map на навигационные действия; ComboBox-стрелки отдельно (egui).

**Tech Stack:** Rust, MIDI-биндинги уже есть (cc_map, key_action).

---

## Test Cases

- TC1: педаль зажата + аккорд Am → в лупер пишется A (root) / или Am-событие; педаль отпущена — обычные ноты не пишутся как аккорды.
- TC2: педаль отпущена до note-off — хвост ноты корректен, нет залипания.
- TC3: CC педали маппится через cc_map/MIDI-learn без правок кода.

## Negative Matrix

-_pedalrelease до нажатия нот — hold-флаг false, обычный путь; двойное нажатие — идемпотентно; MIDI-флуд педалью (повторные CC) — дебаунс.

---

### Task 1: #5 — chord capture по педали
- [ ] Флаг `chord_capture_hold` (CC/note из MIDI-learn); при held: на каждый новый note-on комплекта — detect_root + запись root-ноты в лупер вместо/вместе с отдельными нотами (решить по факту железа)
- [ ] Аудио-тест hold-сценария; commit

### Task 2: #35 хвост — педаль как навигация
- [ ] cc_map: заведомые биндинги «следующий/предыдущий пункт» для списков (если педаль шлёт CC); egui: hover-список реагирует
- [ ] commit

### Task 3: Верификация
- [ ] Полный набор проверок + экран с реальной педалью

## Out of scope
- Всё остальное из #35 (egui ComboBox клавиатура).
