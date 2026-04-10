//! MIDI file sequencer tab UI.

use std::sync::atomic::Ordering;
use eframe::egui;

use crate::synth::ControlEvent;
use crate::synth::midi_player::TrackInstrument;
use crate::synth::sampler::GM_PROGRAM_NAMES;
use super::App;

/// GUI-side per-track state (mirrors the audio-thread TrackState).
pub struct MidiSeqTrackGui {
    pub name: String,
    pub channel: u8,
    pub instrument: TrackInstrument,
    pub muted: bool,
}

impl App {
    pub(super) fn draw_midi_seq(&mut self, ui: &mut egui::Ui) {
        // Sync play state from audio thread
        let atom_playing = self.midi_seq_play_atom.load(Ordering::Relaxed) != 0;
        self.midi_seq_playing = atom_playing;

        // ── Transport ──────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let play_lbl = if self.midi_seq_playing { "⏹ Stop" } else { "▶ Play" };
            if ui.button(play_lbl).clicked() {
                self.midi_seq_playing = !self.midi_seq_playing;
                let _ = self.ctrl_tx.push(ControlEvent::MidiSeqPlay { playing: self.midi_seq_playing });
            }

            // Loop toggle
            let loop_col = if self.midi_seq_looping {
                egui::Color32::from_rgb(80, 210, 100)
            } else {
                egui::Color32::DARK_GRAY
            };
            if ui.button(egui::RichText::new("↻ Loop").color(loop_col)).clicked() {
                self.midi_seq_looping = !self.midi_seq_looping;
                let _ = self.ctrl_tx.push(ControlEvent::MidiSeqSetLooping {
                    looping: self.midi_seq_looping,
                });
            }

            ui.separator();

            // BPM override
            let mut bpm_on = self.midi_seq_bpm.is_some();
            if ui.checkbox(&mut bpm_on, "Override BPM").changed() {
                self.midi_seq_bpm = if bpm_on { Some(120.0) } else { None };
                let _ = self.ctrl_tx.push(ControlEvent::MidiSeqSetBpm { bpm: self.midi_seq_bpm });
            }
            if let Some(ref mut bpm) = self.midi_seq_bpm {
                if ui.add(egui::DragValue::new(bpm).range(20.0..=300.0).speed(0.5)).changed() {
                    let _ = self.ctrl_tx.push(ControlEvent::MidiSeqSetBpm { bpm: Some(*bpm) });
                }
            } else {
                ui.label(egui::RichText::new("(file tempo)").weak().italics());
            }

            ui.separator();

            // Playback progress bar
            let pos = self.midi_seq_pos_atom.load(Ordering::Relaxed);
            let frac = pos as f32 / 1000.0;
            ui.add(egui::ProgressBar::new(frac).desired_width(160.0).show_percentage());
        });

        ui.add_space(4.0);

        // ── File load ──────────────────────────────────────────────────────
        // Poll the async file picker result (zenity runs in a background thread)
        let picked = self.midi_seq_file_pick.as_ref().and_then(|slot| {
            slot.lock().ok().and_then(|g| g.clone())
        });
        if let Some(path) = picked {
            self.midi_seq_file_pick = None;
            self.midi_seq_path = path;
            self.do_load_midi();
        }

        ui.horizontal(|ui| {
            ui.label("MIDI file:");
            let te = egui::TextEdit::singleline(&mut self.midi_seq_path)
                .desired_width(300.0)
                .hint_text("/path/to/file.mid");
            let resp = ui.add(te);
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

            // File picker button — spawns zenity/kdialog in a background thread
            let picking = self.midi_seq_file_pick.is_some();
            let btn = ui.add_enabled(!picking, egui::Button::new("…"))
                .on_hover_text("Browse for .mid file");
            if btn.clicked() {
                let slot = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
                let slot2 = slot.clone();
                std::thread::spawn(move || {
                    let result = open_midi_file_dialog();
                    if let Ok(mut g) = slot2.lock() {
                        *g = result;
                    }
                });
                self.midi_seq_file_pick = Some(slot);
            }
            if picking {
                ui.spinner();
            }

            if ui.button("Load").clicked() || enter {
                self.do_load_midi();
            }
        });

        if !self.midi_seq_status.is_empty() {
            let col = if self.midi_seq_status.starts_with("Error") {
                egui::Color32::from_rgb(220, 70, 70)
            } else {
                egui::Color32::from_rgb(100, 200, 100)
            };
            ui.colored_label(col, &self.midi_seq_status);
        }

        ui.add_space(4.0);
        ui.separator();
        ui.add_space(4.0);

        if self.midi_seq_tracks.is_empty() {
            ui.label(
                egui::RichText::new(
                    "No MIDI file loaded.\n\
                     Enter the path to a .mid file above and press Enter or click Load.",
                )
                .weak(),
            );
            return;
        }

        let has_sf2 = self.sf2_keys_soundfont.is_some();

        // ── Track list ─────────────────────────────────────────────────────
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("midi_seq_tracks")
                .num_columns(6)
                .striped(true)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    // Column headers
                    ui.label(egui::RichText::new("#").strong());
                    ui.label(egui::RichText::new("M").strong().size(13.0))
                        .on_hover_text("Mute");
                    ui.label(egui::RichText::new("Ch").strong());
                    ui.label(egui::RichText::new("Name").strong());
                    ui.label(egui::RichText::new("Instrument").strong());
                    ui.label(egui::RichText::new("Program").strong());
                    ui.end_row();

                    // Collect changes to send after the loop (avoids borrow issues)
                    let mut instr_changes: Vec<(usize, TrackInstrument)> = Vec::new();
                    let mut mute_changes: Vec<(usize, bool)> = Vec::new();

                    for (i, track) in self.midi_seq_tracks.iter_mut().enumerate() {
                        // Track index
                        ui.label(egui::RichText::new(format!("{}", i + 1)).monospace().weak());

                        // Mute button
                        let m_col = if track.muted {
                            egui::Color32::YELLOW
                        } else {
                            egui::Color32::from_gray(80)
                        };
                        if ui
                            .button(egui::RichText::new("M").color(m_col).monospace())
                            .on_hover_text(if track.muted { "Unmute" } else { "Mute" })
                            .clicked()
                        {
                            track.muted = !track.muted;
                            mute_changes.push((i, track.muted));
                        }

                        // Channel label
                        let ch_lbl = if track.channel == 9 {
                            "10 (Drums)".to_string()
                        } else {
                            format!("{}", track.channel + 1)
                        };
                        ui.label(egui::RichText::new(ch_lbl).monospace());

                        // Track name (truncated)
                        let name = if track.name.len() > 24 {
                            format!("{}…", &track.name[..23])
                        } else {
                            track.name.clone()
                        };
                        ui.label(name);

                        // Instrument type selector
                        let prev_instr = track.instrument;
                        {
                            let sf2_prog = if let TrackInstrument::Sf2 { program } = track.instrument {
                                program
                            } else {
                                0
                            };
                            egui::ComboBox::from_id_salt(format!("mst_{i}"))
                                .selected_text(instr_type_name(track.instrument))
                                .width(110.0)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut track.instrument,
                                        TrackInstrument::DspLayer0,
                                        "DSP Synth",
                                    );
                                    if has_sf2 {
                                        ui.selectable_value(
                                            &mut track.instrument,
                                            TrackInstrument::Sf2 { program: sf2_prog },
                                            "SF2",
                                        );
                                    }
                                    ui.selectable_value(
                                        &mut track.instrument,
                                        TrackInstrument::Drums,
                                        "Drums",
                                    );
                                });
                        }
                        if track.instrument != prev_instr {
                            instr_changes.push((i, track.instrument));
                        }

                        // SF2 program picker (only visible when instrument == SF2)
                        if let TrackInstrument::Sf2 { ref mut program } = track.instrument {
                            let prog_name = GM_PROGRAM_NAMES.get(*program as usize).copied().unwrap_or("?");
                            let prev_prog = *program;
                            egui::ComboBox::from_id_salt(format!("msp_{i}"))
                                .selected_text(format!("{}: {}", program, prog_name))
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(300.0)
                                        .show(ui, |ui| {
                                            for (p, name) in GM_PROGRAM_NAMES.iter().enumerate() {
                                                ui.selectable_value(
                                                    program,
                                                    p as u8,
                                                    format!("{p}: {name}"),
                                                );
                                            }
                                        });
                                });
                            if *program != prev_prog {
                                instr_changes.push((i, track.instrument));
                            }
                        } else {
                            ui.label("");
                        }

                        ui.end_row();
                    }

                    // Send changes to audio thread
                    for (idx, instr) in instr_changes {
                        let _ = self.ctrl_tx.push(ControlEvent::MidiSeqSetTrackInstrument {
                            track_idx: idx,
                            instrument: instr,
                        });
                    }
                    for (idx, muted) in mute_changes {
                        let _ = self.ctrl_tx.push(ControlEvent::MidiSeqSetTrackMute {
                            track_idx: idx,
                            muted,
                        });
                    }
                });
        });
    }

    /// Load the MIDI file at `self.midi_seq_path` and send it to the engine.
    pub(super) fn do_load_midi(&mut self) {
        let path = self.midi_seq_path.trim().to_string();
        if path.is_empty() {
            return;
        }
        match std::fs::read(&path) {
            Err(e) => {
                self.midi_seq_status = format!("Error reading file: {e}");
            }
            Ok(bytes) => match crate::synth::midi_player::parse_midi(&bytes) {
                Err(e) => {
                    self.midi_seq_status = format!("Error parsing MIDI: {e}");
                }
                Ok(data) => {
                    let n = data.track_names.len();
                    let ticks = data.total_ticks;
                    // Build GUI track list (instrument defaults: ch9 → Drums, else DspLayer0)
                    self.midi_seq_tracks = data
                        .track_names
                        .iter()
                        .zip(data.track_channels.iter())
                        .map(|(name, &ch)| MidiSeqTrackGui {
                            name: name.clone(),
                            channel: ch,
                            instrument: if ch == 9 {
                                TrackInstrument::Drums
                            } else {
                                TrackInstrument::DspLayer0
                            },
                            muted: false,
                        })
                        .collect();
                    self.midi_seq_status =
                        format!("Loaded: {n} tracks, {ticks} ticks");
                    let _ = self
                        .ctrl_tx
                        .push(ControlEvent::MidiSeqLoad { data: Box::new(data) });
                }
            },
        }
    }
}

fn instr_type_name(instr: TrackInstrument) -> &'static str {
    match instr {
        TrackInstrument::DspLayer0 => "DSP Synth",
        TrackInstrument::Sf2 { .. } => "SF2",
        TrackInstrument::Drums => "Drums",
    }
}

/// Open a native file-picker dialog and return the chosen path.
/// Tries zenity, then kdialog. Returns None if nothing was selected or no tool found.
fn open_midi_file_dialog() -> Option<String> {
    // zenity (GTK, works on both X11 and Wayland via xdg-portal)
    if let Ok(out) = std::process::Command::new("zenity")
        .args([
            "--file-selection",
            "--title=Open MIDI file",
            "--file-filter=MIDI files (*.mid *.midi) | *.mid *.midi",
            "--file-filter=All files (*) | *",
        ])
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
        return None; // zenity found but user cancelled
    }

    // kdialog fallback (KDE)
    if let Ok(out) = std::process::Command::new("kdialog")
        .args([
            "--getopenfilename",
            ".",
            "*.mid *.midi|MIDI files\n*|All files",
            "--title",
            "Open MIDI file",
        ])
        .output()
    {
        if out.status.success() {
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    None
}
