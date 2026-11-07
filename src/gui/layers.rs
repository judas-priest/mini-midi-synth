//! Part management with Split A/B zones.
//!
//! Zone A = Parts 0-7 (always active)
//! Zone B = Parts 8-15 (activated by Split checkbox)
//!
//! All enabled parts are shown as ROWS (one row per part, all fields inline):
//! label, engine toggle (DSP/SF2), instrument select, Mute, Root, key range,
//! Pan, Tr, Vel, Volume, Remove. Visual spec: docs/mockup_layers2.html.
//!
//! When Split is OFF: Zone A parts play full range 0-127
//! When Split is ON:
//!   Zone A parts → 0 .. split_point-1
//!   Zone B parts → split_point .. 127

use std::sync::atomic::Ordering;
use eframe::egui;
use crate::preset::{self, Performance, PartConfig};
use crate::synth::ControlEvent;
use super::{App, ToastKind};
use super::note_name;
use super::PARTS_PER_ZONE;
use super::theme;

const ZONE_A: std::ops::Range<usize> = 0..PARTS_PER_ZONE;
const ZONE_B: std::ops::Range<usize> = PARTS_PER_ZONE..PARTS_PER_ZONE * 2;

/// Честные диапазоны зон по РЕАЛЬНОСТИ сплита (#51): сплит активен только
/// при живой правой руке — иначе все парты 0..=127, как бы перф ни хранил
/// split_point. Единственный источник диапазонов для движка и заголовков.
pub(super) fn zone_range_pair(split_point: u8, split_active: bool) -> ([u8; 2], [u8; 2]) {
    if split_active {
        ([0, split_point.saturating_sub(1)], [split_point, 127])
    } else {
        ([0, 127], [0, 127])
    }
}

impl App {
    pub(super) fn draw_part_rows(&mut self, ui: &mut egui::Ui) {
        // ── Performance save/load (above Split) ──────────────────────────
        self.draw_perf_row(ui);
        ui.add_space(theme::SP_MD); // отделить perf от split-ряда

        // ── Split row: toggle + point + view tabs ────────────────────────
        let split_on = self.parts[ZONE_B].iter().any(|l| l.enabled);
        ui.horizontal(|ui| {
            let split_clicked = if split_on {
                // Split active: show as an on-toggle with explicit remove hint
                ui.small_button("Split: ON ×")
                    .on_hover_text("Turn split off: remove right hand (zone B)")
                    .clicked()
            } else {
                ui.button("+ Right hand (Split)")
                    .on_hover_text("Add right hand: zone B, keyboard splits at the Split point")
                    .clicked()
            };
            if split_clicked {
                if split_on {
                    // Disable all Zone B parts, restore Zone A to full range
                    for i in ZONE_B {
                        if self.parts[i].enabled {
                            self.parts[i].enabled = false;
                            let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
                        }
                    }
                    for i in ZONE_A {
                        if self.parts[i].enabled {
                            self.parts[i].min_note = 0;
                            self.parts[i].max_note = 127;
                            self.send_part_range(i);
                        }
                    }
                    // Switch active part to Zone A if needed
                    if self.active_part >= PARTS_PER_ZONE {
                        self.set_active_part(0);
                    }
                } else {
                    // Enable the first free B slot — полный сброс + пуш (#40/#42)
                    if let Some(i) = ZONE_B.into_iter().find(|&i| !self.parts[i].enabled) {
                        self.reset_and_push_part(i, true);
                    }
                    self.apply_split_ranges();
                }
            }

            if split_on {
                ui.label("Split point:");
                let sp_name = note_name(self.split_point);
                let sp_r = ui.add(egui::Slider::new(&mut self.split_point, 0..=127)
                    .suffix(format!(" ({sp_name})")))
                    .on_hover_text("Keyboard split note (zone B starts here)");
                if sp_r.changed() {
                    self.apply_split_ranges();
                }
                self.knob_feedback(&sp_r, "Split", note_name(self.split_point));
            }

            // ── View tabs (Drums / MIDI Seq / FX Chain / Pads) ───────────
            ui.separator();
            if ui.selectable_label(self.show_drums && !self.show_midi_seq, "Drums").on_hover_text("Drum sequencer").clicked() {
                self.show_drums = true; self.show_looper = false;
                self.show_midi_seq = false; self.show_fx_chain = false;
                self.seq_target_atom.store(0, Ordering::Relaxed);
            }
            if ui.selectable_label(self.show_midi_seq, "MIDI Seq").on_hover_text("MIDI file sequencer").clicked() {
                self.show_midi_seq = true; self.show_drums = false;
                self.show_looper = false; self.show_fx_chain = false;
            }
            if ui.selectable_label(self.show_fx_chain, "FX Chain").on_hover_text("16-slot serial FX chain").clicked() {
                self.show_fx_chain = true; self.show_drums = false;
                self.show_looper = false; self.show_midi_seq = false;
            }
            // NAV long-press opens this too, but NAV SysEx only arrives over
            // Bluetooth — USB users need a GUI path.
            if ui.selectable_label(self.show_pad_perf, "Pads").on_hover_text("Pad Performances (assign performances to pads 36-51)").clicked() {
                self.show_pad_perf = !self.show_pad_perf;
            }
        });

        // ── Part rows (two zones) ────────────────────────────────────────
        // Диапазон руки — один раз в заголовке зоны; карта зон внизу удалена
        // (фидбек: дублировала диапазоны по строкам и занимала место).
        let max_h = (ui.available_height() - theme::SP_LG).max(120.0);
        // #54: сбор rect'ов строк для drag hit-test
        let mut row_rects: Vec<(usize, egui::Rect)> = Vec::new();
        egui::ScrollArea::vertical()
            .id_salt("layers_parts_scroll")
            .auto_shrink(false)
            .max_height(max_h)
            .show(ui, |ui| {
            // ── Zone A ───────────────────────────────────────────────────
            let split_active = self.parts[ZONE_B].iter().any(|p| p.enabled);
            let (a_r, _) = zone_range_pair(self.split_point, split_active);
            let a_header = format!("◀ Left hand (A) — {}..={}", a_r[0], a_r[1]);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(a_header).weak().small());
            });
            for i in ZONE_A {
                if !self.parts[i].enabled { continue; }
                ui.push_id(format!("part_row_{i}"), |ui| {
                    self.draw_part_row(ui, i, &mut row_rects);
                });
            }
            ui.add_space(theme::SP_SM); // воздух над Add part (TECHDEBT #36)
            let a_count = ZONE_A.filter(|&i| self.parts[i].enabled).count();
            if a_count < PARTS_PER_ZONE
                && ui.small_button("+ Add part").on_hover_text("Add part to Zone A").clicked()
            {
                self.add_part_to_zone(false);
            }

            // ── Zone B (visible only when split enabled) ─────────────────
            if split_on {
                ui.add_space(theme::SP_XS);
                ui.horizontal(|ui| {
                    let (_, b_r) = zone_range_pair(self.split_point, split_active);
                    ui.label(egui::RichText::new(format!(
                        "▶ Right hand (B) — {}..={}", b_r[0], b_r[1])).weak().small());
                });
                for i in ZONE_B {
                    if !self.parts[i].enabled { continue; }
                    ui.push_id(format!("part_row_{i}"), |ui| {
                        self.draw_part_row(ui, i, &mut row_rects);
                    });
                }
                ui.add_space(theme::SP_SM); // воздух над Add part (TECHDEBT #36)
                let b_count = ZONE_B.filter(|&i| self.parts[i].enabled).count();
                if b_count < PARTS_PER_ZONE
                    && ui.small_button("+ Add part").on_hover_text("Add part to Zone B").clicked()
                {
                    self.add_part_to_zone(true);
                }
            }

            // #54: drag part row — hit-test + drop. РАЗМЕЩЕНИЕ: в САМОМ КОНЦЕ
            // ScrollArea-замыкания (после "+ Add part" зоны B) — return здесь
            // обрезал бы зону B и row_rects.
            if let Some(src) = self.part_drag_src {
                // Цель = строка под pointer (drag живёт на хендле — commit
                // в его drag_stopped); здесь только расчёт цели + подсветка
                let pointer = ui.ctx().pointer_interact_pos();
                let mut new_dst = None;
                if let Some(p) = pointer {
                    for (i, r) in &row_rects {
                        if r.contains(p) {
                            let dst = *i;
                            let src_zone = src / PARTS_PER_ZONE;
                            let dst_zone = dst / PARTS_PER_ZONE;
                            if src_zone == dst_zone && src != dst {
                                new_dst = Some(dst);
                            }
                            break;
                        }
                    }
                }
                self.part_drag_dst = new_dst;
                // подсветка слота-цели в той же зоне
                if let Some(dst) = self.part_drag_dst {
                    for (i, r) in &row_rects {
                        if *i == dst {
                            // 4-й аргумент StrokeKind ОБЯЗАТЕЛЕН в egui 0.31.1
                            ui.painter().rect_stroke(*r, 4.0,
                                egui::Stroke::new(2.0, egui::Color32::from_rgb(0xFF, 0x00, 0xFF)),
                                egui::StrokeKind::Inside);
                        }
                    }
                }
            }
        });

        // ── Middle-click / × removal confirmation ────────────────────────
        if let Some(p) = self.pending_remove_part {
            let zone = if p < PARTS_PER_ZONE { "A" } else { "B" };
            let num = (p % PARTS_PER_ZONE) + 1;
            let dialog = egui::Modal::new(egui::Id::new("remove_part_dialog")).show(ui.ctx(), |ui| {
                ui.set_width(280.0);
                ui.strong(format!("Remove part {zone}{num}?"));
                ui.label("Its preset edits will be lost.");
                ui.add_space(theme::SP_MD);
                ui.horizontal(|ui| {
                    if ui.button("Remove").clicked() {
                        self.remove_part(p);
                        self.pending_remove_part = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_remove_part = None;
                    }
                });
            });
            if dialog.should_close() {
                self.pending_remove_part = None;
            }
        }
    }

    /// Performance save/load row (shown above part rows).
    fn draw_perf_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Perf:");
            ui.add(egui::TextEdit::singleline(&mut self.perf_name)
                .desired_width(110.0).hint_text("name"));
            if ui.button("Save").on_hover_text("Save performance").clicked() && !self.perf_name.trim().is_empty() {
                let parts: Vec<PartConfig> = self.parts.iter().enumerate().map(|(i, l)| {
                    let patch_name = self.patches.get(l.patch_idx)
                        .map(|p| p.name.clone()).unwrap_or_default();
                    if patch_name.is_empty() {
                        // #49: пустое имя (unwrap_or_default) никогда не
                        // сматчится при load — сохранение невосстановимо.
                        // Load-сторона warn-ует только на НЕпустые промахи
                        // (guard !is_empty — legacy/disabled парты без спама),
                        // поэтому здесь сообщение самодостаточное
                        log::warn!(
                            "perf save '{}': part {} patch_idx {} out of range — saved with empty patch_name",
                            self.perf_name, i, l.patch_idx
                        );
                    }
                    PartConfig {
                        patch_name, enabled: l.enabled, volume: l.volume,
                        key_low: l.min_note, key_high: l.max_note,
                        param_overrides: l.edited_params.clone(),
                        sf2_mode: l.sf2_mode, sf2_program: l.sf2_program,
                        pan: l.pan, transpose: l.transpose, mute: l.mute,
                        vel_min: l.vel_min, vel_max: l.vel_max,
                        note_mode: l.note_mode,
                        root_only: false, // legacy: больше не пишется
                        sustain_enabled: l.sustain_enabled,
                        macro_names: l.macro_names.clone(),
                        xy_macro_x: l.xy_macro_x,
                        xy_macro_y: l.xy_macro_y,
                        ignore_pb: l.ignore_pb,
                        ignore_mw: l.ignore_mw,
                    }
                }).collect();
                match preset::save_performance(&Performance {
                    name: self.perf_name.trim().to_string(),
                    category: String::new(),
                    split_point: Some(self.split_point),
                    parts,
                }) {
                    Ok(_path) => { self.show_toast("Performance saved", ToastKind::Success);
                                   self.perf_list = preset::list_performances();
                                   self.perf_list.insert(0, ("(Init)".into(), std::path::PathBuf::new()));
                                   self.refresh_console_list(); }
                    Err(e)    => { self.show_toast(format!("Error: {e}"), ToastKind::Error); }
                }
            }
            ui.separator();
            if ui.button("Reset").on_hover_text("Load the built-in (Init) performance: single part A1 = Init preset, full keyboard, no SF2").clicked()
                && self.load_performance_by_name("(Init)")
            {
                self.show_toast("Reset to (Init)", ToastKind::Success);
            }
            ui.separator();
            let mut load_perf: Option<Performance> = None;
            egui::ComboBox::from_id_salt("perf_load")
                .selected_text(if self.perf_list.is_empty() { "No performances" } else { "Load..." })
                .width(120.0)
                .show_ui(ui, |ui| {
                    for (name, path) in &self.perf_list {
                        if ui.selectable_label(false, name).clicked() {
                            if name == "(Init)" {
                                load_perf = Some(preset::init_performance());
                            } else {
                                load_perf = preset::load_performance(path).ok();
                            }
                        }
                    }
                });
            if let Some(perf) = load_perf {
                {
                    // ЕДИНЫЙ путь применения (_split_ и last_performance
                    // восстанавливаются так же, как по имени — #49)
                    self.apply_performance(&perf);
                    self.show_toast(format!("Loaded: {}", perf.name), ToastKind::Success);
                }
            }
            // #57: активный трек + кнопка менеджера — в строке перфа,
            // после выпадающего списка (юзер 2026-11-06)
            ui.separator();
            let track_name = self.config.ui.active_folder
                .and_then(|i| self.config.ui.perf_folders.get(i))
                .map(|f| f.name.as_str())
                .unwrap_or("(all)");
            ui.label(egui::RichText::new(format!("Track: {track_name}")).weak());
            if ui.small_button("Tracks").on_hover_text(
                "Track folders (setlists) — organize perfs per song").clicked()
            {
                self.show_tracks_manager = true;
            }
            ui.separator();
        });
    }

    /// One row = all fields of a part (mockup_layers2.html).
    fn draw_part_row(&mut self, ui: &mut egui::Ui, i: usize,
        row_rects: &mut Vec<(usize, egui::Rect)>) {
        let is_active = i == self.active_part
            && !self.show_drums && !self.show_looper
            && !self.show_midi_seq && !self.show_fx_chain;
        let zone = if i < PARTS_PER_ZONE { "A" } else { "B" };
        let num = (i % PARTS_PER_ZONE) + 1;

        ui.horizontal(|ui| {
            // #54: drag-хендл — рисованный грип (3×2 точки painter'ом:
            // U+2802 в шрифте egui НЕТ — канон глиф-граблей) + Sense::drag.
            // Commit на drag_stopped ХЕНДЛА (egui гарантирует доставку
            // drag-событий виджету, начавшему драг, до release — надёжнее
            // release-frame hit-test'а); цель — part_drag_dst из drop-детекта.
            let (grip_rect, grip) = ui.allocate_exact_size(
                egui::vec2(14.0, 22.0),
                egui::Sense::drag());
            let grip = grip.on_hover_cursor(egui::CursorIcon::Grab);
            let gc = grip_rect.center();
            for (dx, dy) in [(-3.0, -4.0), (0.0, -4.0), (3.0, -4.0),
                             (-3.0, 0.0),  (0.0, 0.0),  (3.0, 0.0),
                             (-3.0, 4.0),  (0.0, 4.0),  (3.0, 4.0)] {
                ui.painter().circle_filled(
                    gc + egui::vec2(dx, dy), 1.1, theme::TEXT_MUTED);
            }
            if grip.drag_started() {
                self.part_drag_src = Some(i);
                self.part_drag_dst = None;
            }
            if grip.drag_stopped() && self.part_drag_src == Some(i) {
                // drop на хендле: свап с последней подсвеченной целью
                if let (Some(src), Some(dst)) =
                    (self.part_drag_src, self.part_drag_dst)
                {
                    if src != dst { self.swap_parts(src, dst); }
                }
                self.part_drag_src = None;
                self.part_drag_dst = None;
            }
            // Подсветка цели — в drop-детекте draw_part_rows (layers.rs)
            // по row_rects; ui.max_rect() здесь = остаток ScrollArea, НЕ строка

            // Part label — цвет парта = цвет руки на карте зон (identity);
            // активный парт — тинт фоном (selection), без egui-selection-синевы
            // и рамок (research: identity и selection — разные каналы).
            let label = format!("{zone}{num}");
            let muted = self.parts[i].mute;
            let part_color = theme::PART_COLORS[i % theme::PART_COLORS.len()];
            let name_color = if muted { egui::Color32::GRAY } else { part_color };
            let mut label_text = egui::RichText::new(label).strong().color(name_color);
            if muted { label_text = label_text.strikethrough(); }
            let name_btn = egui::Button::new(label_text)
                .fill(if is_active { part_color.gamma_multiply(0.30) } else { egui::Color32::TRANSPARENT })
                .stroke(egui::Stroke::NONE);
            if ui.add_sized([34.0, 22.0], name_btn)
                .on_hover_text(format!("Select part {zone}{num} for editing (muted = gray + strikethrough)"))
                .clicked()
            {
                self.set_active_part(i);
                self.show_drums = false; self.show_looper = false;
                self.show_midi_seq = false; self.show_fx_chain = false;
                self.seq_target_atom.store(1, Ordering::Relaxed);
            }

            // Instrument select (кнопка открывает браузер; тоггл движка — в браузере)
            let sf2 = self.parts[i].sf2_mode;
            if sf2 {
                if self.sf2_keys_soundfont.is_some() {
                    use crate::synth::sampler::GM_PROGRAM_NAMES;
                    let prog = self.parts[i].sf2_program;
                    let cur_name = GM_PROGRAM_NAMES.get(prog as usize).copied().unwrap_or("?");
                    if ui.add_sized([170.0, 22.0], egui::Button::new(cur_name))
                        .on_hover_text(format!("GM program {prog} — open instrument browser (search + categories)")).clicked()
                    {
                        self.preset_search.clear();
                        self.inst_browser_part = Some(i);
                        self.inst_browser_scroll = true; // (#37) открытие центрирует выбор
                    self.inst_browser_seeded = false; // курсор пересеется на текущий инструмент
                    }
                } else {
                    ui.add_sized([170.0, 22.0], egui::Label::new(egui::RichText::new("---").weak()))
                        .on_hover_text("No SoundFont loaded (load one in Settings)");
                }
            } else {
                // DSP patch select: нужно только имя текущего патча.
                // self.patches (immutable) + self.parts (mutable) не пересекаются.
                let cur = self.patches.get(self.parts[i].patch_idx)
                    .map(|p| p.name.clone())
                    .unwrap_or_else(|| "---".into());
                if ui.add_sized([170.0, 22.0], egui::Button::new(&cur))
                    .on_hover_text("Open patch browser (search + categories)").clicked()
                {
                    self.preset_search.clear();
                    self.inst_browser_part = Some(i);
                    self.inst_browser_scroll = true; // (#37) открытие центрирует выбор
                    self.inst_browser_seeded = false; // курсор пересеется на текущий инструмент
                }
            }

            // Mute / Root — фиксированные ширины: колонки всех строк совпадают
            let mut m = self.parts[i].mute;
            if ui.add_sized([52.0, 22.0], egui::Checkbox::new(&mut m, "Mute")).on_hover_text("Mute this part").changed() {
                self.push_undo_snapshot_for(i); // #31: снимок ДО изменения
                self.parts[i].mute = m;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartMute { part: i, mute: m });
                self.persist_parts_state();
            }
            // Note mode: одна кнопка-цикл в стиле тумблера DSP/SF2 — клик
            // переключает Poly → Bass → Root → Poly, на кнопке текущий режим.
            let (mode_label, mode_tip) = match self.parts[i].note_mode {
                crate::synth::NOTE_MODE_BASS => ("Bass", "Bass — only the lowest held note. Click: Root"),
                crate::synth::NOTE_MODE_ROOT => ("Root", "Root — the chord's root note. Click: Poly"),
                _ => ("Poly", "Poly — all notes. Click: Bass"),
            };
            let next_mode = match self.parts[i].note_mode {
                crate::synth::NOTE_MODE_BASS => crate::synth::NOTE_MODE_ROOT,
                crate::synth::NOTE_MODE_ROOT => crate::synth::NOTE_MODE_POLY,
                _ => crate::synth::NOTE_MODE_BASS,
            };
            if ui.add_sized([52.0, 22.0], egui::Button::new(mode_label).small())
                .on_hover_text(mode_tip.to_string())
                .clicked()
            {
                self.parts[i].note_mode = next_mode;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartNoteMode { part: i, mode: next_mode });
                self.persist_parts_state();
            }
            // Play Mode (Poly/Mono/Mono-ST/Latch/...): live-переключение в
            // строке (юзер 2026-11-05: моно/легато/лэтч просились в строку,
            // как Sus/PB/MW). Источник правды — edited_params (как в дереве
            // params.rs), undo-снапшот на смену; портаменто остаётся в дереве
            // (глубина звука, не live-свитч).
            let mut pmode = self.parts[i].edited_params.get("play_mode").copied().unwrap_or(0.0) as usize;
            let pm_resp = egui::ComboBox::from_id_salt(format!("pm_row_{i}"))
                .width(56.0)
                .selected_text((*crate::gui::PLAY_MODE_NAMES.get(pmode).unwrap_or(&"Poly")).to_string())
                .show_ui(ui, |ui| {
                    for (mi, name) in crate::gui::PLAY_MODE_NAMES.iter().enumerate() {
                        if ui.selectable_value(&mut pmode, mi, *name).changed() {
                            self.push_undo_snapshot_for(i);
                            self.parts[i].edited_params.insert("play_mode".into(), pmode as f32);
                            self.send_edited_params(i);
                        }
                    }
                });
            pm_resp.response.on_hover_text(
                "Play mode: Poly / Mono / Mono-ST / Latch / ... (portamento — в дереве параметров)");
            let mut s = self.parts[i].sustain_enabled;
            if ui.add_sized([52.0, 22.0], egui::Checkbox::new(&mut s, "Sus")).on_hover_text(
                "Sustain pedal affects this part").changed()
            {
                self.parts[i].sustain_enabled = s;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSustain { part: i, on: s });
                self.persist_parts_state();
            }
            // Единая семантика чекбоксов (юзер 2026-11-05): ГАЛКА = РЕАГИРУЕТ
            // (как Sus). Внутри — ignore-флаги (инверсия на границе GUI).
            let mut pb = !self.parts[i].ignore_pb;
            if ui.add_sized([52.0, 22.0], egui::Checkbox::new(&mut pb, "PB")).on_hover_text(
                "Part responds to pitch bend (unchecked = ignore)").changed()
            {
                self.parts[i].ignore_pb = !pb;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartIgnorePb { part: i, on: !pb });
                self.persist_parts_state();
            }
            let mut mw = !self.parts[i].ignore_mw;
            if ui.add_sized([52.0, 22.0], egui::Checkbox::new(&mut mw, "MW")).on_hover_text(
                "Part responds to mod wheel (unchecked = ignore)").changed()
            {
                self.parts[i].ignore_mw = !mw;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartIgnoreMw { part: i, on: !mw });
                self.persist_parts_state();
            }

            // Key range убран из строки: диапазон руки — один раз в заголовке
            // зоны (фидбек: «зачем к каждому парту, он один на руку»).

            // Pan — значение в hover, не на полосе: текст значения переменной
            // ширины вылезал из фикс-бокса и сдвигал хвост строки («криво»).
            ui.label("Pan:");
            let mut pan = self.parts[i].pan;
            let pan_r = ui.add_sized([90.0, 22.0], egui::Slider::new(&mut pan, -1.0..=1.0).show_value(false))
                .on_hover_text(format!("Pan: {pan:+.2} — double-click to reset"));
            // #31: снимок ДО изменения — один на drag (drag_started), до
            // clamp'а у колеса и до сброса двойным кликом.
            if pan_r.drag_started() { self.push_undo_snapshot_for(i); }
            if pan_r.double_clicked() {
                self.push_undo_snapshot_for(i);
                pan = 0.0;
            }
            if pan_r.changed() || pan_r.double_clicked() {
                self.parts[i].pan = pan;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan });
            }
            self.knob_feedback(&pan_r, "Pan", format!("{pan:+.2}"));
            // (#45) колесо над слайдером = шаг значения; глотаем smooth-дельту,
            // чтобы ScrollArea не скроллил список под слайдером
            if pan_r.hovered() {
                let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    self.push_undo_snapshot_for(i); // #31: до clamp'а
                    pan = (pan + dy * 0.0025).clamp(-1.0, 1.0);
                    ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                    self.parts[i].pan = pan;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part: i, pan });
                    self.knob_feedback(&pan_r, "Pan", format!("{pan:+.2}"));
                }
            }

            // Tr
            ui.label("Tr:");
            let mut tr = self.parts[i].transpose as i32;
            let tr_r = ui.add_sized([120.0, 22.0], egui::Slider::new(&mut tr, -24..=24).show_value(false))
                .on_hover_text(format!("Transpose: {tr:+} st — double-click to reset"));
            // #31: оба пути слайдера — drag_started и до clamp'а у колеса,
            // плюс сброс двойным кликом.
            if tr_r.drag_started() { self.push_undo_snapshot_for(i); }
            if tr_r.double_clicked() {
                self.push_undo_snapshot_for(i);
                tr = 0;
            }
            if tr_r.changed() || tr_r.double_clicked() {
                self.parts[i].transpose = tr as i8;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: tr as i8 });
            }
            self.knob_feedback(&tr_r, "Tr", format!("{tr:+} st"));
            // (#45) колесо: ~1 полутон на ~20px прокрутки
            if tr_r.hovered() {
                let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    self.push_undo_snapshot_for(i); // #31: до clamp'а
                    tr = (tr + (dy * 0.05).round() as i32).clamp(-24, 24);
                    ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                    self.parts[i].transpose = tr as i8;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part: i, semitones: tr as i8 });
                    self.knob_feedback(&tr_r, "Tr", format!("{tr:+} st"));
                }
            }

            // Vel
            ui.label("Vel:");
            let mut vmin = self.parts[i].vel_min as i32;
            let mut vmax = self.parts[i].vel_max as i32;
            let c1 = ui.add_sized([46.0, 22.0], egui::DragValue::new(&mut vmin).range(1..=127).speed(1)).changed();
            ui.label("-");
            let c2 = ui.add_sized([46.0, 22.0], egui::DragValue::new(&mut vmax).range(1..=127).speed(1)).changed();
            if c1 || c2 {
                let vmin = (vmin as u8).max(1);
                let vmax = (vmax as u8).max(vmin);
                self.parts[i].vel_min = vmin;
                self.parts[i].vel_max = vmax;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartVelRange { part: i, vel_min: vmin, vel_max: vmax });
            }

            // Volume
            let mut vol = self.parts[i].volume;
            let vol_r = ui.add_sized([90.0, 22.0], egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false))
                .on_hover_text(format!("Volume: {vol:.2} — double-click to reset"));
            if vol_r.double_clicked() { vol = 0.8; }
            if vol_r.changed() || vol_r.double_clicked() {
                self.parts[i].volume = vol;
                self.send_part_volume(i);
            }
            self.knob_feedback(&vol_r, "Vol", format!("{vol:.2}"));
            // (#45) колесо над слайдером = шаг значения; см. Pan
            if vol_r.hovered() {
                let dy = ui.ctx().input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    vol = (vol + dy * 0.0025).clamp(0.0, 1.0);
                    ui.ctx().input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
                    self.parts[i].volume = vol;
                    self.send_part_volume(i);
                    self.knob_feedback(&vol_r, "Vol", format!("{vol:.2}"));
                }
            }

            // Remove (never for the first part of a zone)
            let is_zone_first = i.is_multiple_of(PARTS_PER_ZONE);
            if !is_zone_first && ui.small_button("×").on_hover_text("Remove this part").clicked() {
                self.pending_remove_part = Some(i);
            }
        });
        // #54: rect строки для drag hit-test (после horizontal — включает всю строку)
        row_rects.push((i, ui.min_rect()));
    }

    fn add_part_to_zone(&mut self, zone_b: bool) {
        let range = if zone_b { ZONE_B } else { ZONE_A };
        if let Some(i) = range.into_iter().find(|&i| !self.parts[i].enabled) {
            self.reset_and_push_part(i, true);
            let split_on = self.parts[ZONE_B].iter().any(|p| p.enabled);
            if split_on {
                self.apply_split_ranges();
            } else {
                // No split — new part gets full range
                self.parts[i].min_note = 0;
                self.parts[i].max_note = 127;
                self.send_part_range(i);
            }
            self.set_active_part(i);
            self.show_drums = false; self.show_midi_seq = false; self.show_fx_chain = false;
        }
    }

    fn remove_part(&mut self, part: usize) {
        self.parts[part].enabled = false;
        self.parts[part].mute = false;
        self.parts[part].pan = 0.0;
        self.parts[part].transpose = 0;
        self.parts[part].sf2_mode = false;
        self.parts[part].note_mode = crate::synth::NOTE_MODE_POLY;
        let _ = self.ctrl_tx.push(ControlEvent::SetPartEnabled { part, enabled: false });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: false });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartPan { part, pan: 0.0 });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartTranspose { part, semitones: 0 });
        let _ = self.ctrl_tx.push(ControlEvent::SetPartNoteMode { part, mode: crate::synth::NOTE_MODE_POLY });
        self.set_active_part(if part >= PARTS_PER_ZONE { PARTS_PER_ZONE } else { 0 });
        // find first enabled in same zone
        let zone = if part >= PARTS_PER_ZONE { ZONE_B } else { ZONE_A };
        if let Some(i) = zone.into_iter().find(|&i| self.parts[i].enabled) {
            self.set_active_part(i);
        }
        // #51: удаление последнего B-парта = сплит исчез; предикат внутри
        // сам развернёт зону A в 0..=127 (раньше движок оставался
        // порезанным до следующего триггера apply_split_ranges)
        if part >= PARTS_PER_ZONE {
            self.apply_split_ranges();
        }
    }

    /// Set key ranges for all parts based on split_point.
    /// Zone A: 0 .. split_point-1
    /// Zone B: split_point .. 127
    pub(super) fn apply_split_ranges(&mut self) {
        // TECHDEBT #51: сплит реален только при живой правой руке. Перф
        // хранит split_point даже при выключенном сплите и безусловный
        // сплит резал зону A до sp-1 («Organ играет до C3» при выключенном
        // сплите; Reset и рестарт — те же грабли). Юзер-модель: нет B —
        // нет сплита.
        let split_active = self.parts[ZONE_B].iter().any(|p| p.enabled);
        let (a, b) = zone_range_pair(self.split_point, split_active);
        // Всем партам (вкл. выключенным): диапазон — свойство зоны; иначе
        // пере-включенный парт несёт stale-диапазон (0..127 по умолчанию) и
        // молчит на нотах своей зоны (#46-семейство)
        for i in ZONE_A {
            self.parts[i].min_note = a[0];
            self.parts[i].max_note = a[1];
            self.send_part_range(i);
        }
        for i in ZONE_B {
            self.parts[i].min_note = b[0];
            self.parts[i].max_note = b[1];
            self.send_part_range(i);
        }
    }

}

#[cfg(test)]
mod split_tests {
    use super::*;

    #[test]
    fn tc1_active_split_pair() {
        assert_eq!(zone_range_pair(60, true), ([0, 59], [60, 127]));
    }

    #[test]
    fn tc2_no_zone_b_means_no_split() {
        // Регрессия «Organ играет до C3»: перф с split_point=60 и без
        // включённых B-партов обязан давать полный диапазон
        assert_eq!(zone_range_pair(60, false), ([0, 127], [0, 127]));
    }

    #[test]
    fn tc3_split_at_zero() {
        assert_eq!(zone_range_pair(0, true), ([0, 0], [0, 127]));
    }

    #[test]
    fn tc4_split_at_top() {
        assert_eq!(zone_range_pair(127, true), ([0, 126], [127, 127]));
    }
}
