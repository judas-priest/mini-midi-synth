// Instrument browser modal: search + category tree over DSP/GM engines.
use super::*;

impl App {
    /// Instrument browser modal: search + category tree, applies to `inst_browser_part`.
    pub(crate) fn draw_instrument_browser_modal(&mut self, ctx: &egui::Context) {
        let Some(part) = self.inst_browser_part else { return };
        // #браузер-фидбек: браузер показывает ОБОИХ дерева (DSP + GM). is_sf2 —
        // ТЕКУЩИЙ движок парта: он задаёт только порядок секций (своя — первой)
        // и источник seed-курсора; переключение движка следует за выбором.
        let is_sf2 = self.parts[part].sf2_mode;
        // Свежее состояние движка парта: после тоггла в том же кадре порядок
        // секций и seed следуют за НОВЫМ движком
        let engine_now = self.parts[part].sf2_mode;
        // Зона/номер вместо LAYER_NAMES (8 имён): парты зоны B (8-15) давали «?»
        let layer_label = if part < PARTS_PER_ZONE {
            format!("A{}", part + 1)
        } else {
            format!("B{}", part - PARTS_PER_ZONE + 1)
        };

        let mut close = false;
        let mut picked_patch: Option<usize> = None;
        let mut picked_program: Option<u8> = None;
        let mut toggled_cat: Option<String> = None;
        // Some(true) = клик в GM-секции, Some(false) = клик в DSP-секции —
        // движок парта следует за выбранным звуком (post-closure ниже).
        let mut picked_switch_to_sf2: Option<bool> = None;
        // Клавиатурная навигация (VS Code Quick Pick): плоский список видимых
        // элементов (имя, индекс, из GM-секции?), виртуальный выделенный
        // индекс (поля inst_browser_*).
        let mut flat: Vec<(String, usize, bool)> = Vec::new();

        // Навигационный ввод — ДО окна: single-line поиск не использует ↑/↓,
        // поэтому перехват безопасен при любом фокусе (egui 0.31: TextEdit
        // не консьюмит стрелки/Enter/Esc).
        let moved = ctx.input(|i| {
            let mut d: i32 = 0;
            if i.key_pressed(egui::Key::ArrowDown) { d += 1; }
            if i.key_pressed(egui::Key::ArrowUp) { d -= 1; }
            d
        });
        if moved != 0 {
            let new_idx = self.inst_browser_index as i32 + moved;
            self.inst_browser_index = new_idx.max(0) as usize;
            self.inst_browser_scroll = true; // (#37) скролл только по смене индекса
        }
        if self.inst_browser_last_search != self.preset_search {
            // Фильтр изменился — выделение на первый результат
            self.inst_browser_index = 0;
            self.inst_browser_last_search = self.preset_search.clone();
            self.inst_browser_scroll = true; // (#37) сброс фильтра — к началу списка
        }
        let enter_pressed = ctx.input(|i| i.key_pressed(egui::Key::Enter));

        // Немодальное окно: выбор инструмента обратим (клик по другому патчу
        // перекроет), полноэкранная модалка избыточна (audit Medium п.8).
        // Обёртка modal::ModalWindow: Esc-стек, константный Id окна (память
        // позиции при динамическом заголовке — TECHDEBT #34), футер Done.
        let mut win_open = true;
        let title = if is_sf2 {
            format!("GM Instrument — Part {layer_label}")
        } else {
            format!("Patch — Part {layer_label}")
        };
        let mut builder = modal::ModalWindow::new(
            ctx, "inst_browser", format!("Instrument Browser — {layer_label}"),
            &mut win_open, 700.0,
        )
        .default_height(540.0)
        // #38-регресс: пин ширины — храповик egui растит окно до контента
        .fixed_width(700.0);
        // #34: restore the position saved in a previous session (within a
        // session egui remembers drags itself, keyed by the constant Id).
        if let Some([x, y]) = self.config.ui.inst_browser_pos {
            builder = builder.default_pos(egui::pos2(x, y));
        }
        let win = builder.show(|ui| {
            ui.label(egui::RichText::new(title).strong());
            // Тоггл движка DSP ↔ SF2 — живое переключение прямо в браузере
            ui.horizontal(|ui| {
                let cur_sf2 = self.parts[part].sf2_mode;
                let (label, tip) = if cur_sf2 {
                    ("\u{27A1} DSP", "Switch this part to the DSP engine")
                } else {
                    ("\u{27A1} SF2", "Switch this part to the SF2 (soundfont) engine")
                };
                if ui.button(label).on_hover_text(tip).clicked() {
                    let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                    self.parts[part].sf2_mode = !cur_sf2;
                    let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: !cur_sf2 });
                    if part == 0 { self.config.sf2.layer_a_sf2 = !cur_sf2; }
                    else if part == 1 { self.config.sf2.layer_b_sf2 = !cur_sf2; }
                    self.persist_parts_state();
                    // Курсор — на первую строку новой текущей секции
                    self.inst_browser_index = 0;
                    self.inst_browser_seeded = true;
                    self.inst_browser_scroll = true;
                }
                ui.label(egui::RichText::new(if cur_sf2 {
                    "engine: SF2 (soundfont)"
                } else {
                    "engine: DSP"
                }).weak());
            });
            // (#38) фикс-минимальная ширина: длинные строки поиска/результатов
            // не распирают окно
            ui.set_min_width(660.0);

            // Двухколонный браузер (юзер 2026-11-08): слева поиск+список,
            // справа — параметры пресета. Явные ширины колонок: SidePanel
            // внутри Window разносил окно на весь экран.
            // Fallback-курсоры секций (читаются post-closure — hoisted)
            let mut first_gm_row: Option<usize> = None;
            let mut first_dsp_row: Option<usize> = None;
            ui.horizontal(|ui| {
                // ── Левая колонка: поиск + список ──
                ui.allocate_ui(egui::vec2(268.0, ui.available_height()), |ui| {
                    ui.set_min_width(252.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let w = ui.available_width();
                let r = ui.add(egui::TextEdit::singleline(&mut self.preset_search)
                    .hint_text("\u{1F50D} Search...")
                    .desired_width((w - 30.0).max(60.0)));
                if !self.preset_search.is_empty()
                    && ui.small_button("\u{00D7}").clicked()
                {
                    self.preset_search.clear();
                    r.request_focus();
                }
            });
            ui.separator();

            let query = self.preset_search.to_lowercase();
            let searching = !query.is_empty();

            egui::ScrollArea::vertical().auto_shrink(false)
                .show(ui, |ui| {
                    // Оба дерева всегда видны; секция ТЕКУЩЕГО движка парта —
                    // первой (план: DSP сверху по умолчанию, osc-парт).
                    let sections = if engine_now { [true, false] } else { [false, true] };
                    // Первые строки секций (fallback-курсор, см. post-closure)
                    for (si, section_sf2) in sections.into_iter().enumerate() {
                        if si > 0 {
                            ui.add_space(theme::SP_XS);
                            ui.separator();
                            ui.add_space(theme::SP_XS);
                        }
                    if section_sf2 {
                        if self.sf2_keys_soundfont.is_some() {
                        // ── GM categories (перенос mod.rs:1141-1205: клик пишет
                        //    picked_program, коллапс — toggled_cat) ──
                        use crate::synth::sampler::GM_PROGRAM_NAMES;
                        // Заголовок секции; маркер — у текущего движка парта
                        // (U+26AB: шрифт без U+25CF — см. memory glyph-грабли)
                        let gm_hdr = if section_sf2 == engine_now {
                            "\u{26AB} SF2 — GM-инструменты"
                        } else {
                            "SF2 — GM-инструменты"
                        };
                        ui.strong(gm_hdr);
                        let current_program = self.parts[part].sf2_program;
                        let gm_categories: &[(&str, usize, usize)] = &[
                            ("Piano", 0, 8), ("Chromatic Perc", 8, 16),
                            ("Organ", 16, 24), ("Guitar", 24, 32),
                            ("Bass", 32, 40), ("Strings", 40, 48),
                            ("Ensemble", 48, 56), ("Brass", 56, 64),
                            ("Reed", 64, 72), ("Pipe", 72, 80),
                            ("Synth Lead", 80, 88), ("Synth Pad", 88, 96),
                            ("Synth FX", 96, 104), ("Ethnic", 104, 112),
                            ("Percussive", 112, 120), ("Sound FX", 120, 128),
                        ];
                        // Seed-курсор садится на текущий инструмент: если его
                        // категория свёрнута — строка skipped и fallback
                        // уводит курсор наверх. Раскрываем категорию.
                        if !self.inst_browser_seeded && engine_now {
                            for &(cat_name, start, end) in gm_categories.iter() {
                                if (current_program as usize) >= start
                                    && (current_program as usize) < end
                                {
                                    if self.collapsed_categories.remove(cat_name) {
                                        self.save_collapsed_categories();
                                    }
                                    break;
                                }
                            }
                        }
                        for &(cat_name, start, end) in gm_categories {
                            let matches: Vec<usize> = if searching {
                                (start..end)
                                    .filter(|&i| GM_PROGRAM_NAMES[i].to_lowercase().contains(&query)
                                        || cat_name.to_lowercase().contains(&query))
                                    .collect()
                            } else {
                                (start..end).collect()
                            };
                            if searching && matches.is_empty() { continue; }
                            let is_collapsed =
                                !searching && self.collapsed_categories.contains(cat_name);
                            let arrow = if is_collapsed { "\u{25B6}" } else { "\u{2B07}" };
                            if !searching {
                                if ui.selectable_label(false, format!("{arrow} {cat_name}")).clicked() {
                                    toggled_cat = Some(cat_name.to_string());
                                }
                            } else {
                                ui.label(egui::RichText::new(cat_name).strong().small());
                            }
                            if !is_collapsed {
                                for i in matches {
                                    let name = GM_PROGRAM_NAMES[i];
                                    if first_gm_row.is_none() { first_gm_row = Some(flat.len()); }
                                    flat.push((format!("  {name}"), i, true));
                                    let selected = current_program == i as u8;
                                    // При открытии курсор садится на текущий
                                    // инструмент парта — seed только из секции
                                    // ТЕКУЩЕГО движка (#сож-class: «предлагает не то»)
                                    // Seed курсора: первая строка секции текущего
                                    // движка (не selected-строка — она может быть
                                    // свёрнута/скрыта; фидбек 2026-11-02)
                                    // Seed — на ТЕКУЩЕМ инструменте парта
                                    if !self.inst_browser_seeded && engine_now && selected {
                                        self.inst_browser_index = flat.len() - 1;
                                        self.inst_browser_seeded = true;
                                        // Флаг мог быть съеден ранней строкой
                                        // со «старым» индексом — перевзводим,
                                        // чтобы ТЕКУЩИЙ курсор прокрутился
                                        self.inst_browser_scroll = true;
                                    }
                                    let is_hi = self.inst_browser_index == flat.len() - 1;
                                    // Одна подсветка — курсор клавиатуры (#37-класс);
                                    // активный инструмент помечен точкой, не заливкой
                                    let mut row_text = egui::RichText::new(format!("  {}", name.chars().take(30).collect::<String>()));
                                    if selected { row_text = row_text.strong(); }
                                    let r = ui.selectable_label(is_hi, row_text)
                                        .on_hover_text(name);
                                    if is_hi && self.inst_browser_scroll {
                                        r.scroll_to_me(Some(egui::Align::Center));
                                        self.inst_browser_scroll = false; // (#37) один раз
                                    }
                                    if r.clicked() && !selected {
                                        self.inst_browser_index = flat.len() - 1;
                                        picked_program = Some(i as u8);
                                        picked_switch_to_sf2 = Some(true);
                                    }
                                    // Двойной клик = применить + закрыть
                                    // (Windows select-and-OK)
                                    if r.double_clicked() {
                                        picked_program = Some(i as u8);
                                        picked_switch_to_sf2 = Some(true);
                                        close = true;
                                    }
                                }
                            }
                        }
                        } else {
                            // SF2 не загружен — GM-секция скрыта целиком, hint
                            ui.label(egui::RichText::new("SF2: load a soundfont in Settings").weak());
                        }
                    } else {
                        // ── DSP patch tree (перенос mod.rs:1216-1271: клик пишет
                        //    picked_patch) ──
                        let dsp_hdr = if section_sf2 == engine_now {
                            "\u{26AB} DSP — патчи"
                        } else {
                            "DSP — патчи"
                        };
                        ui.strong(dsp_hdr);
                        let current_preset_idx = self.parts[part].patch_idx;
                        // Seed на текущий патч: раскрываем его свёрнутую
                        // категорию (иначе строка skipped, курсор — наверху)
                        if !self.inst_browser_seeded && !engine_now {
                            if let Some(p) = self.patches.get(current_preset_idx) {
                                if self.collapsed_categories.remove(&p.category) {
                                    self.save_collapsed_categories();
                                }
                            }
                        }
                        let categories: Vec<(String, usize)> = {
                            let mut cats = Vec::new();
                            let mut last = String::new();
                            for (i, p) in self.patches.iter().enumerate() {
                                if p.category != last {
                                    cats.push((p.category.clone(), i));
                                    last = p.category.clone();
                                }
                            }
                            cats
                        };
                        let mut cat_idx = 0;
                        for (i, patch) in self.patches.iter().enumerate() {
                            if cat_idx < categories.len() && categories[cat_idx].1 == i {
                                cat_idx += 1;
                            }
                            if searching {
                                let name_match = patch.name.to_lowercase().contains(&query);
                                let cat_match = patch.category.to_lowercase().contains(&query);
                                if !name_match && !cat_match { continue; }
                            }
                            if !searching {
                                if cat_idx > 0 && categories[cat_idx - 1].1 == i {
                                    let cat = &categories[cat_idx - 1].0;
                                    let is_collapsed = self.collapsed_categories.contains(cat);
                                    let arrow = if is_collapsed { "\u{25B6}" } else { "\u{2B07}" };
                                    if ui.selectable_label(false, format!("{arrow} {cat}")).clicked() {
                                        toggled_cat = Some(cat.clone());
                                    }
                                }
                                if self.collapsed_categories.contains(&patch.category) {
                                    continue;
                                }
                            }
                            if first_dsp_row.is_none() { first_dsp_row = Some(flat.len()); }
                            let selected = i == current_preset_idx;
                            flat.push((format!("  {}", patch.name), i, false));
                            // Seed курсора на текущий патч — только из секции
                            // ТЕКУЩЕГО движка (первый кадр после открытия)
                            // Seed курсора: первая строка секции текущего
                            // движка (не selected-строка — она может быть
                            // свёрнута/скрыта; фидбек 2026-11-02)
                            // Seed — на текущем патче парта
                            if !self.inst_browser_seeded && !engine_now && selected {
                                self.inst_browser_index = flat.len() - 1;
                                self.inst_browser_seeded = true;
                                // Перевзвод scroll — см. GM-ветку выше
                                self.inst_browser_scroll = true;
                            }
                            let is_hi = self.inst_browser_index == flat.len() - 1;
                            // Одна подсветка — курсор; активный патч — точкой
                            let mut row_text = egui::RichText::new(format!("  {}", patch.name.chars().take(30).collect::<String>()));
                            if selected { row_text = row_text.strong(); }
                            let r = ui.selectable_label(is_hi, row_text) // (#38)
                                .on_hover_text(patch.name.clone());
                            if is_hi && self.inst_browser_scroll {
                                r.scroll_to_me(Some(egui::Align::Center));
                                self.inst_browser_scroll = false; // (#37) один раз
                            }
                            if r.clicked() && !selected {
                                self.inst_browser_index = flat.len() - 1;
                                picked_patch = Some(i);
                                picked_switch_to_sf2 = Some(false);
                            }
                            if r.double_clicked() {
                                picked_patch = Some(i);
                                picked_switch_to_sf2 = Some(false);
                                close = true;
                            }
                        }
                    }
                    // Fallback-курсор: seed не сработал (selected-строка
                    // свёрнута/скрыта) — первая строка секции текущего движка;
                    // scroll — чтобы курсор был виден и в следующем кадре
                    if !self.inst_browser_seeded {
                        self.inst_browser_index = match engine_now {
                            true => first_gm_row.unwrap_or(0),
                            false => first_dsp_row.unwrap_or(0),
                        };
                        self.inst_browser_seeded = true;
                        self.inst_browser_scroll = true;
                    }
                    }
                });
            // (#44-фидбек) Клик по инструменту = ПРИМЕНИТЬ и играть дальше —
            // окно остаётся открытым; закрытие — Done из shell, двойной клик
            // по инструменту (Windows select-and-OK), Esc, ✕.
                });

                // ── Правая колонка: параметры пресета ──
                ui.allocate_ui(egui::vec2(392.0, ui.available_height()), |ui| {
                    ui.add_space(theme::SP_XS);
                    if !self.parts[part].sf2_mode {
                        ui.strong("Параметры пресета");
                        egui::ScrollArea::vertical().auto_shrink(false)
                            .show(ui, |ui| {
                                self.draw_params_editable_for(ui, part);
                            });
                    } else {
                        ui.add_space(theme::SP_MD);
                        ui.label(egui::RichText::new(
                            "SF2: у семплов нет DSP-параметров").weak());
                    }
                });
            });
        });

        // ── Post-closure handling (after .show, reads locals + modal) ────
        // #34: запоминаем позицию КАЖДЫЙ кадр, пока окно открыто — надёжнее,
        // чем хук «при закрытии»: drag + quit позицию не теряет. egui 0.31:
        // `Window::show` → Option<InnerResponse>; позиция = inner.response.rect.
        if let Some(inner) = &win {
            let min = inner.response.rect.min;
            self.config.ui.inst_browser_pos = Some([min.x, min.y]);
            self.mark_config_dirty();
        }
        // Зажим индекса: список мог пересобраться (фильтр/смена парта)
        let flat_len = flat.len();
        if self.inst_browser_index >= flat_len {
            self.inst_browser_index = flat_len.saturating_sub(1);
        }
        if enter_pressed && flat_len > 0 {
            let (_, idx, entry_sf2) = &flat[self.inst_browser_index.min(flat_len - 1)];
            if *entry_sf2 {
                picked_program = Some(*idx as u8);
                picked_switch_to_sf2 = Some(true);
            } else {
                picked_patch = Some(*idx);
                picked_switch_to_sf2 = Some(false);
            }
        }
        if let Some(cat) = toggled_cat {
            if self.collapsed_categories.contains(&cat) {
                self.collapsed_categories.remove(&cat);
            } else {
                self.collapsed_categories.insert(cat);
            }
            self.save_collapsed_categories();
        }
        // Движок следует за выбранным звуком (#браузер-фидбек)
        if let Some(sf2) = picked_switch_to_sf2 {
            if self.parts[part].sf2_mode != sf2 {
                let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
                self.parts[part].sf2_mode = sf2;
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part, enabled: sf2 });
                if part == 0 { self.config.sf2.layer_a_sf2 = sf2; }
                else if part == 1 { self.config.sf2.layer_b_sf2 = sf2; }
                self.persist_parts_state();
            }
        }
        if let Some(prog) = picked_program {
            self.parts[part].sf2_program = prog;
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                part, program: prog, bank: 0 });
            // Persist for parts 0/1 only (startup seeds). part >= 2: no persist —
            // прежний else затирал layer_b_program с любого парта (QW2).
            if part == 0 { self.config.sf2.layer_a_program = prog; }
            else if part == 1 { self.config.sf2.layer_b_program = prog; }
            self.mark_config_dirty();
            // Применили и играем дальше — окно не закрываем (фидбек 2026-11-01)
        }
        if let Some(idx) = picked_patch {
            self.push_undo_snapshot_for(part); // #31: снимок ДО применения патча
            let _ = self.ctrl_tx.push(ControlEvent::AllNotesOff);
            self.parts[part].patch_idx = idx;
            self.load_edited_params(part);
            self.send_edited_params(part);
            self.snapshot_preset_and_mark_dirty();
            // Применили и играем дальше — окно не закрываем
        }
        if !win_open || win.is_none() {
            close = true;
        }
        if close {
            self.inst_browser_part = None;
            // #34: позиция уже в config.ui — persisted вместе с остальным.
            self.save_global_to_config();
        }
    }
}
