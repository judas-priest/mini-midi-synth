// Tracks manager modal: setlists (perf folders) editor.
use super::*;

impl App {
    /// #57: менеджер папок-треков (сетлистов). egui-замыкания не могут держать
    /// &mut self — решения собираются в ops во время отрисовки (copy-then-act),
    /// применяются после закрытия окна; КАЖДАЯ мутация — mark_config_dirty()
    /// (F3: запись на диск отложена до flush_config, форс на выходе) +
    /// refresh_console_list.
    pub(crate) fn draw_tracks_manager(&mut self, ctx: &egui::Context) {
        enum Op {
            SetActive(usize),
            Rename(usize, String),
            MoveUp(usize),
            MoveDown(usize),
            MoveAliasUp(usize, usize),
            MoveAliasDown(usize, usize),
            DelFolder(usize),
            DelPerf(usize, usize),
            AddCur(usize),
            AddPerf(usize, String),
            New,
        }
        if !self.perf_ui.show_tracks_manager {
            // Guard-флаг не должен залипать после закрытия окна.
            self.perf_ui.tracks_rename_focus = false;
            return;
        }
        let mut open = true;
        let mut ops: Vec<Op> = Vec::new();
        // #35: фокус rename-TextEdit прошлого кадра — консервативный nav-guard
        // для update() (draw идёт после update, флаг на кадр позже).
        let mut rename_focus = false;
        let cur_perf = self.perf_ui.perf_name.clone();
        let win = modal::ModalWindow::new(
            ctx, "tracks_manager", "Track Folders (Setlists)", &mut open, 520.0)
            .fixed_width(520.0);
        win.show(|ui| {
            ui.label(egui::RichText::new(
                "Setlists are ordered lists of aliases — the original perf is edited in one place.")
                .weak());
            let perf_names: Vec<String> =
                self.perf_ui.perf_list.iter().map(|(n, _)| n.clone()).collect();
            let sel = self.config.ui.active_folder;
            let armed = self.perf_ui.tracks_del_armed;

            // ── Left pane: Setlists ──
            ui.columns(2, |cols| {
                // ── Left: Setlists ──
                cols[0].horizontal(|ui| {
                    ui.label(egui::RichText::new("Setlists").strong());
                    if ui.small_button("+ New").clicked() {
                        ops.push(Op::New);
                    }
                });
                if self.config.ui.perf_folders.is_empty() {
                    cols[0].add_space(theme::SP_MD);
                    cols[0].label(egui::RichText::new("No setlists yet.").weak());
                    cols[0].label(egui::RichText::new("Click + New to create one.").weak());
                }
                for fi in 0..self.config.ui.perf_folders.len() {
                    let name = self.config.ui.perf_folders[fi].name.clone();
                    let active = sel == Some(fi);
                    let sel_here = self.perf_ui.tracks_sel == Some(fi);
                    cols[0].horizontal(|ui| {
                        if ui.selectable_label(sel_here || active, &name)
                            .on_hover_text(if active { "Active — click to select for editing" } else { "Click/Enter to set active" }).clicked()
                        {
                            ops.push(Op::SetActive(fi));
                            self.perf_ui.tracks_sel = Some(fi);
                        }
                        if active {
                            ui.label(egui::RichText::new("ACTIVE")
                                .color(egui::Color32::from_rgb(0xFF, 0x00, 0xFF)).small());
                        }
                    });
                }
                // ── Right: Contents of the selected setlist ──
                cols[1].horizontal(|ui| {
                    ui.label(egui::RichText::new(match sel {
                        Some(fi) => format!(
                            "Contents: {}", self.config.ui.perf_folders[fi].name),
                        None => "Contents".to_string(),
                    }).strong());
                });
            });

            match sel {
                Some(fi) if fi < self.config.ui.perf_folders.len() => {
                    // ── Toolbar (non-destructive left, destructive right) ──
                    ui.horizontal(|ui| {
                        if ui.small_button("+ Current Perf").clicked() {
                            ops.push(Op::AddCur(fi));
                        }
                        egui::ComboBox::from_id_salt("tm_add_perf")
                            .selected_text("+ Perf…")
                            .show_ui(ui, |ui| {
                                for n in &perf_names {
                                    if ui.selectable_label(false, n).clicked() {
                                        ops.push(Op::AddPerf(fi, n.clone()));
                                    }
                                }
                            });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("\u{2B07}").on_hover_text("Move setlist down").clicked() {
                                ops.push(Op::MoveDown(fi));
                            }
                            if ui.small_button("\u{2B06}").on_hover_text("Move setlist up").clicked() {
                                ops.push(Op::MoveUp(fi));
                            }
                            let mut armed_now = armed;
                            if confirm::confirm_button(ui, "Delete Setlist",
                                &mut armed_now, true,
                                "Click again to delete this setlist")
                            {
                                ops.push(Op::DelFolder(fi));
                            }
                            self.perf_ui.tracks_del_armed = armed_now;
                        });
                    });
                    // ── Alias rows: label … reorder ↑↓ … × (right edge) ──
                    if self.config.ui.perf_folders[fi].perfs.is_empty() {
                        ui.add_space(theme::SP_SM);
                        ui.label(egui::RichText::new(
                            "Empty setlist — add performances above.").weak());
                    }
                    for pi in (0..self.config.ui.perf_folders[fi].perfs.len()).rev() {
                        let pname = self.config.ui.perf_folders[fi].perfs[pi].clone();
                        ui.horizontal(|ui| {
                            ui.label(format!("  {}", pname));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("\u{00D7}")
                                    .on_hover_text("Remove from setlist (the perf itself is kept)")
                                    .clicked()
                                {
                                    ops.push(Op::DelPerf(fi, pi));
                                }
                                if pi + 1 < self.config.ui.perf_folders[fi].perfs.len()
                                    && ui.small_button("\u{2B07}").clicked()
                                {
                                    ops.push(Op::MoveAliasDown(fi, pi));
                                }
                                if pi > 0 && ui.small_button("\u{2B06}").clicked() {
                                    ops.push(Op::MoveAliasUp(fi, pi));
                                }
                            });
                        });
                    }
                    // ── Rename: inline, commits on Enter ──
                    ui.add_space(theme::SP_SM);
                    ui.horizontal(|ui| {
                        ui.label("Rename:");
                        let mut renamed =
                            self.config.ui.perf_folders[fi].name.clone();
                        let resp = ui.add(egui::TextEdit::singleline(&mut renamed)
                            .desired_width(160.0)
                            .hint_text("Enter to apply"));
                        if resp.has_focus() {
                            rename_focus = true;
                        }
                        if resp.lost_focus()
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                            && !renamed.trim().is_empty()
                            && renamed.trim()
                                != self.config.ui.perf_folders[fi].name.as_str()
                        {
                            ops.push(Op::Rename(fi, renamed.trim().to_string()));
                        }
                    });
                }
                Some(_) | None => {
                    ui.add_space(theme::SP_MD);
                    ui.label(egui::RichText::new(
                        "Select a setlist on the left to edit its contents.").weak());
                }
            }
        });
        // Прошлый кадр для nav-гейта в update(): фокус живёт, пока TextEdit
        // активен — дребезга нет (канон консервативного guard'а).
        self.perf_ui.tracks_rename_focus = rename_focus;
        // Close propagation: Done/Esc/✕ set *win_open=false — propagate to
        // the flag (was dropped in the UX rewrite — Done did nothing)
        if !open {
            self.perf_ui.show_tracks_manager = false;
        }
        for op in ops {
            match op {
                Op::SetActive(fi) => {
                    if fi < self.config.ui.perf_folders.len() {
                        self.config.ui.active_folder = Some(fi);
                        self.perf_ui.perf_window = 0;
                        self.mark_config_dirty();
                        self.refresh_console_list();
                    }
                }
                Op::Rename(fi, nm) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) { f.name = nm; }
                    self.mark_config_dirty();
                }
                Op::MoveUp(fi) => {
                    if fi > 0 && fi < self.config.ui.perf_folders.len() {
                        self.config.ui.perf_folders.swap(fi - 1, fi);
                        if self.config.ui.active_folder == Some(fi) {
                            self.config.ui.active_folder = Some(fi - 1);
                        }
                        if self.perf_ui.tracks_sel == Some(fi) {
                            self.perf_ui.tracks_sel = Some(fi - 1);
                        }
                        self.mark_config_dirty();
                        self.refresh_console_list();
                    }
                }
                Op::MoveDown(fi) => {
                    if fi + 1 < self.config.ui.perf_folders.len() {
                        self.config.ui.perf_folders.swap(fi, fi + 1);
                        if self.config.ui.active_folder == Some(fi) {
                            self.config.ui.active_folder = Some(fi + 1);
                        }
                        if self.perf_ui.tracks_sel == Some(fi) {
                            self.perf_ui.tracks_sel = Some(fi + 1);
                        }
                        self.mark_config_dirty();
                        self.refresh_console_list();
                    }
                }
                Op::DelFolder(fi) => {
                    if fi < self.config.ui.perf_folders.len() {
                        self.config.ui.perf_folders.remove(fi);
                        match self.config.ui.active_folder {
                            Some(a) if a == fi => self.config.ui.active_folder = None,
                            Some(a) if a > fi => self.config.ui.active_folder = Some(a - 1),
                            _ => {}
                        }
                        // sel за пределами после удаления — clamp (канон tracks_window)
                        if let Some(s) = self.perf_ui.tracks_sel {
                            if s >= self.config.ui.perf_folders.len() {
                                self.perf_ui.tracks_sel =
                                    self.config.ui.perf_folders.len().checked_sub(1);
                            }
                        }
                        self.mark_config_dirty();
                        self.refresh_console_list();
                    }
                }
                Op::MoveAliasUp(fi, pi) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        if pi > 0 && pi < f.perfs.len() {
                            f.perfs.swap(pi - 1, pi);
                            self.mark_config_dirty();
                        }
                    }
                }
                Op::MoveAliasDown(fi, pi) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        if pi + 1 < f.perfs.len() {
                            f.perfs.swap(pi, pi + 1);
                            self.mark_config_dirty();
                        }
                    }
                }
                Op::DelPerf(fi, pi) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        if pi < f.perfs.len() { f.perfs.remove(pi); }
                    }
                    self.mark_config_dirty();
                    self.refresh_console_list();
                }
                Op::AddCur(fi) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        f.perfs.push(cur_perf.clone());
                    }
                    self.mark_config_dirty();
                    self.refresh_console_list();
                }
                Op::AddPerf(fi, n) => {
                    if let Some(f) = self.config.ui.perf_folders.get_mut(fi) {
                        f.perfs.push(n);
                    }
                    self.mark_config_dirty();
                    self.refresh_console_list();
                }
                Op::New => {
                    let n = self.config.ui.perf_folders.len() + 1;
                    self.config.ui.perf_folders.push(crate::config::PerfFolder {
                        name: format!("Track {n}"),
                        perfs: Vec::new(),
                    });
                    self.mark_config_dirty();
                }
            }
        }
    }
}
