// SF2 plumbing: soundfont scan, parse, load/unload for keys and drums.
use super::*;

/// Result of scanning for .sf2 files, including directories that could not be
/// read — so the UI can explain an empty list instead of just showing one.
pub struct Sf2Scan {
    pub files: Vec<(String, std::path::PathBuf)>,
    pub unreadable: Vec<(std::path::PathBuf, String)>,
}

/// Scan for .sf2 files in the standard data directory.
pub fn scan_sf2_files() -> Sf2Scan {
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut unreadable = Vec::new();

    // On Android /sdcard/Download needs storage permissions this app does not
    // hold, so the scan usually fails there. That has to be reported rather
    // than silently producing an empty list.
    #[cfg(target_os = "android")]
    let dirs_to_scan = vec![sf2_dir(), std::path::PathBuf::from("/sdcard/Download")];
    #[cfg(not(target_os = "android"))]
    let dirs_to_scan = vec![sf2_dir()];

    for dir in &dirs_to_scan {
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("sf2") {
                        let name = path.file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("?")
                            .to_string();
                        if !files.iter().any(|(n, _)| n == &name) {
                            files.push((name, path));
                        }
                    }
                }
            }
            Err(e) => {
                // A missing private sf2 dir is normal on first run. Anything
                // else — permission denied in particular — is worth surfacing.
                if e.kind() != std::io::ErrorKind::NotFound {
                    unreadable.push((dir.clone(), e.to_string()));
                }
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    Sf2Scan { files, unreadable }
}

pub(crate) fn sf2_dir() -> std::path::PathBuf {
    crate::config::app_data_dir().join("sf2")
}

impl App {
    pub(crate) fn find_or_add_sf2(&mut self, sf2_path: &str) -> Option<usize> {
        let idx = self.sf2_ui.sf2_file_list.iter().position(|(_, p)| p.to_string_lossy() == sf2_path);
        if let Some(idx) = idx {
            return Some(idx);
        }
        let path = std::path::PathBuf::from(sf2_path);
        if path.exists() {
            let name = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();
            self.sf2_ui.sf2_file_list.push((name, path));
            Some(self.sf2_ui.sf2_file_list.len() - 1)
        } else {
            None
        }
    }

    /// Extract the kit's own drum map (note -> SAMPLE name) for the remap UI.
    /// All instruments/presets are scanned; per-key sample names are collected.
    fn extract_kit_names(&mut self, sf: &rustysynth::SoundFont) {
        self.reset_kit_names();
        let headers = sf.get_sample_headers();
        let mut seen = [false; 26];
        for inst in sf.get_instruments() {
            for r in inst.get_regions() {
                let (lo, hi) = (r.get_key_range_start(), r.get_key_range_end());
                if hi < 30 || lo > 55 { continue; }
                let sid = r.get_sample_id();
                if sid >= headers.len() { continue; }
                let name = headers[sid].get_name().to_string();
                for k in lo.max(30)..=hi.min(55) {
                    let i = (k - 30) as usize;
                    if !seen[i] { self.drum_kit_names[i] = name.clone(); seen[i] = true; }
                }
            }
        }
    }

    pub(crate) fn reset_kit_names(&mut self) {
        for (i, n) in self.drum_kit_names.iter_mut().enumerate() {
            let note = 30 + i as u8;
            *n = if (36..=51).contains(&note) {
                crate::synth::drum::DRUM_NAMES[(note - 36) as usize].to_string()
            } else { String::new() };
        }
    }

    /// Parse an SF2 file and return the Arc<SoundFont>, or set sf2_status on error.
    fn parse_sf2(&mut self, path: &std::path::Path) -> Option<std::sync::Arc<rustysynth::SoundFont>> {
        match std::fs::File::open(path) {
            Ok(mut file) => {
                use std::io::BufReader;
                let mut reader = BufReader::new(&mut file);
                match rustysynth::SoundFont::new(&mut reader) {
                    Ok(sf) => Some(std::sync::Arc::new(sf)),
                    Err(e) => { self.show_toast(format!("SF2 parse error: {e}"), ToastKind::Error); None }
                }
            }
            Err(e) => { self.show_toast(format!("SF2 open error: {e}"), ToastKind::Error); None }
        }
    }

    pub(crate) fn load_sf2_keys(&mut self, idx: usize) {
        let Some((name, path)) = self.sf2_ui.sf2_file_list.get(idx).cloned() else {
            self.show_toast("SF2 file not found", ToastKind::Error);
            return;
        };
        // Reuse existing drums soundfont if same file
        let sf = if self.sf2_ui.sf2_drums_loaded_name == name {
            self.sf2_ui.sf2_drums_soundfont.clone()
        } else {
            None
        };
        let sf = match sf {
            Some(s) => s,
            None => match self.parse_sf2(&path) {
                Some(s) => s,
                None => return,
            },
        };
        self.sf2_ui.sf2_keys_soundfont = Some(sf.clone());
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2BlockSize { size: self.sf2_ui.sf2_block_size });
        let _ = self.ctrl_tx.push(ControlEvent::LoadKeysSoundFont { soundfont: sf });
        self.sf2_ui.sf2_keys_loaded_name = name;
        self.show_toast("Keys SF2 loaded", ToastKind::Success);
        // Apply per-part SF2 modes
        for i in 0..2 {
            if self.parts[i].sf2_mode {
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: true });
                let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Program {
                    part: i, program: self.parts[i].sf2_program, bank: 0,
                });
            }
        }
        self.config.sf2.keys_file_path = Some(path.to_string_lossy().to_string());
        self.config.sf2.file_path = self.config.sf2.keys_file_path.clone(); // compat
        self.mark_config_dirty();
    }

    pub(crate) fn load_sf2_drums(&mut self, idx: usize) {
        let Some((name, path)) = self.sf2_ui.sf2_file_list.get(idx).cloned() else {
            self.show_toast("SF2 file not found", ToastKind::Error);
            return;
        };
        // Reuse existing keys soundfont if same file
        let sf = if self.sf2_ui.sf2_keys_loaded_name == name {
            self.sf2_ui.sf2_keys_soundfont.clone()
        } else {
            None
        };
        let sf = match sf {
            Some(s) => s,
            None => match self.parse_sf2(&path) {
                Some(s) => s,
                None => return,
            },
        };
        self.sf2_ui.sf2_drums_soundfont = Some(sf.clone());
        let _ = self.ctrl_tx.push(ControlEvent::SetSf2BlockSize { size: self.sf2_ui.sf2_block_size });
        let _ = self.ctrl_tx.push(ControlEvent::LoadDrumsSoundFont { soundfont: sf.clone() });
        self.sf2_ui.sf2_drums_loaded_name = name;
        self.extract_kit_names(&sf);
        self.push_drum_choke_groups();
        self.show_toast("Drums SF2 loaded", ToastKind::Success);
        if self.sf2_ui.sf2_drums_enabled {
            let _ = self.ctrl_tx.push(ControlEvent::SetDrumsSf2Mode { enabled: true });
        }
        // Load the per-kit remap table (identity if none saved for this kit).
        if let Some(v) = self.config.sf2.drums_remaps.get(&self.drum_remap_key()) {
            // migrate legacy slot-encoded values (0..16) to kit notes (+36)
            let v: Vec<u8> = v.iter().map(|&x| if x < 16 { x + 36 } else { x }).collect();
            let mut t = [0u8; 16];
            for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(36 + i as u8).clamp(30, 55); }
            self.drum_remap = t;
        } else {
            for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = (36 + i) as u8; }
        }
        self.push_drum_remap();
        if let Some(v) = self.config.sf2.drums_vels.get(&self.drum_remap_key()) {
            let mut t = [0u8; 16];
            for (i, s) in t.iter_mut().enumerate() { *s = v.get(i).copied().unwrap_or(0).min(127); }
            self.drum_pad_vel = t;
        } else {
            self.drum_pad_vel = [0u8; 16];
        }
        self.push_drum_pad_vel();
        self.config.sf2.drums_file_path = Some(path.to_string_lossy().to_string());
        self.mark_config_dirty();
    }

    pub(crate) fn unload_sf2_keys(&mut self) {
        self.sf2_ui.sf2_keys_soundfont = None;
        self.sf2_ui.sf2_keys_loaded_name.clear();
        for i in 0..2 {
            self.parts[i].sf2_mode = false;
            let _ = self.ctrl_tx.push(ControlEvent::SetPartSf2Mode { part: i, enabled: false });
        }
        let _ = self.ctrl_tx.push(ControlEvent::UnloadKeysSoundFont);
        self.config.sf2.keys_file_path = None;
        self.config.sf2.file_path = None;
        self.mark_config_dirty();
    }

    pub(crate) fn unload_sf2_drums(&mut self) {
        self.sf2_ui.sf2_drums_soundfont = None;
        self.sf2_ui.sf2_drums_loaded_name.clear();
        self.sf2_ui.sf2_drums_enabled = false;
        self.reset_kit_names();
        self.push_drum_choke_groups(); // back to GM choke layout
        // Internal kit: reset to identity remap.
        for (i, v) in self.drum_remap.iter_mut().enumerate() { *v = (36 + i) as u8; }
        self.push_drum_remap();
        self.drum_pad_vel = [0u8; 16];
        self.push_drum_pad_vel();
        let _ = self.ctrl_tx.push(ControlEvent::SetDrumsSf2Mode { enabled: false });
        let _ = self.ctrl_tx.push(ControlEvent::UnloadDrumsSoundFont);
        self.config.sf2.drums_file_path = None;
        self.mark_config_dirty();
    }
}
