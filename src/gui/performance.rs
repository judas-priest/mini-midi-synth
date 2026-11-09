// Performance apply: load-by-name + single apply point for pad/perf switching.
use super::*;

impl App {
    /// Load a performance by name (for pad perf switching).
    /// Returns true if found and loaded.
    pub(crate) fn load_performance_by_name(&mut self, name: &str) -> bool {
        let perf = if name == "(Init)" {
            preset::init_performance()
        } else {
            let path = self.perf_ui.perf_list.iter()
                .find(|(n, _)| n == name)
                .map(|(_, p)| p.clone());
            let Some(path) = path else { return false; };
            match preset::load_performance(&path) {
                Ok(p) => p,
                Err(e) => { log::warn!("apply_performance failed: {e}"); return false; }
            }
        };
        // NO AllNotesOff here: seamless perf switch — sounding tails ring out
        // naturally (SF2 voices keep the old instrument), held keys keep
        // sustaining without re-attack; new notes use the new perf.
        // Legacy 8-part migration is centralized in preset::load_performance.
        self.apply_performance(&perf);
        true
    }

    /// ЕДИНАЯ точка применения перфа (обе загрузки: по имени и панельный
    /// «Load...»). История: пути разъезжались — панельный не восстанавливал
    /// split_point и last_performance (#49: сплит «слетал» до C3/C4), by-name
    /// не применял note_mode. Сюда добавлять новые поля PartConfig.
    pub(crate) fn apply_performance(&mut self, perf: &crate::preset::Performance) {
        let parts = &perf.parts;
        // Restore the stored split point; legacy perfs derive it from the
        // first enabled zone-B part's key_low (fallback: C4=60).
        self.core_state_ui.split_point = perf.split_point.unwrap_or_else(|| {
            parts.iter()
                .skip(crate::gui::PARTS_PER_ZONE)
                .find(|p| p.enabled)
                .map(|p| p.key_low)
                .filter(|&v| v > 0)
                .unwrap_or(60)
        });
        self.perf_ui.perf_name = perf.name.clone();
        // Запомнить перф — восстановится при старте (юзер-репорт 2026-10-31)
        self.core_state_ui.config.ui.last_performance = Some(perf.name.clone());
        self.snapshot_preset_and_mark_dirty();
        // Анти-stuck: lift the pedal BEFORE applying the new parts, or notes
        // held by the pedal of the previous perf would ring forever under the
        // new one (NOT AllNotesOff — seamless tails keep ringing).
        // Side effects: latch releases, F1 pedal button shows OFF — accepted.
        let _ = self.core_state_ui.ctrl_tx.push(ControlEvent::SetSustain { on: false });
        for (i, part) in parts.iter().enumerate() {
            if i >= self.core_state_ui.parts.len() { break; }
            let patch_idx = self.core_state_ui.patches.iter()
                .position(|p| p.name == part.patch_name);
            if patch_idx.is_none() && !part.patch_name.is_empty() {
                // #49: тихий fallback на патч 0 = «партия играет чужое» без
                // единого следа в логе; поведение сохраняем (патч 0), но
                // диагностируем
                log::warn!(
                    "perf '{}': part {}: patch '{}' not found — falling back to patch 0",
                    perf.name, i, part.patch_name
                );
            }
            let patch_idx = patch_idx.unwrap_or(0);
            self.core_state_ui.parts[i].patch_idx = patch_idx;
            self.core_state_ui.parts[i].enabled = part.enabled;
            self.core_state_ui.parts[i].volume = part.volume;
            self.core_state_ui.parts[i].edited_params = part.param_overrides.clone();
            // #49: зеркало макросов следует за params, иначе GUI показывает
            // stale, а push_full_part_state (audio-reconnect) перезапишет
            // движок старыми значениями через SetMacro
            self.core_state_ui.parts[i].macro_vals = macro_vals_from_params(&self.core_state_ui.parts[i].edited_params);
            // #49: имена макросов и XY-биндинги — GUI-стейт парта.
            // Пустой список (Init/legacy-перф) = дефолты «Macro 1..8»,
            // НЕ унаследованные имена от ПРЕДЫДУЩЕГО перфа
            self.core_state_ui.parts[i].macro_names = if part.macro_names.is_empty() {
                (1..=8).map(|k| format!("Macro {k}")).collect()
            } else {
                part.macro_names.clone()
            };
            // clamp: битый JSON с индексом ≥8 не должен паниковать при
            // индексации macro_vals/макро-лейблов
            self.core_state_ui.parts[i].xy_macro_x = part.xy_macro_x.min(7);
            self.core_state_ui.parts[i].xy_macro_y = part.xy_macro_y.min(7);
            self.core_state_ui.parts[i].sf2_mode = part.sf2_mode;
            self.core_state_ui.parts[i].sf2_program = part.sf2_program;
            self.core_state_ui.parts[i].pan = part.pan;
            self.core_state_ui.parts[i].transpose = part.transpose;
            self.core_state_ui.parts[i].mute = part.mute;
            self.core_state_ui.parts[i].note_mode = part.note_mode;
            self.core_state_ui.parts[i].sustain_enabled = part.sustain_enabled;
            self.core_state_ui.parts[i].ignore_pb = part.ignore_pb;
            self.core_state_ui.parts[i].ignore_mw = part.ignore_mw;
            self.core_state_ui.parts[i].vel_min = if part.vel_min == 0 { 1 } else { part.vel_min };
            self.core_state_ui.parts[i].vel_max = if part.vel_max == 0 { 127 } else { part.vel_max };
            // ST2: один SetPartFull вместо 13-14 пушей. Источники те же, что у
            // старого блока: scalar-поля из структуры перфа (part), vel_min/max
            // — уже clamped значения self.core_state_ui.parts[i], macro_vals — зеркало
            // edited_params перфа, min/max_note — из self.core_state_ui.parts[i] (stale
            // до-сплит значения — ровно то, что пушит send_part_range;
            // apply_split_ranges в хвосте перепушит диапазоны по сплиту).
            // Порядковая инверсия ПРОВЕРЕНА: раньше LoadPatch уходил ПЕРВЫМ
            // (send_edited_params), в транзакции — ПОСЛЕДНИМ; end-state не
            // расходится, LoadPatch не трогает volume/pan/range/сэмплер-каналы.
            // bank жёстко 0 — как в старом SetPartSf2Program arm:е (bank: 0).
            let patch = ControlEvent::load_patch_from_edited(
                i, &self.core_state_ui.parts[i].edited_params,
                self.core_state_ui.patches.get(self.core_state_ui.parts[i].patch_idx),
            );
            let patch = match patch {
                ControlEvent::LoadPatch(boxed) => Some(boxed),
                _ => unreachable!(),
            };
            let state = crate::synth::PartFullState {
                enabled: part.enabled, mute: part.mute, volume: part.volume,
                min_note: self.core_state_ui.parts[i].min_note, max_note: self.core_state_ui.parts[i].max_note,
                vel_min: self.core_state_ui.parts[i].vel_min, vel_max: self.core_state_ui.parts[i].vel_max,
                pan: part.pan, transpose: part.transpose,
                note_mode: part.note_mode,
                sustain_enabled: part.sustain_enabled,
                ignore_pb: part.ignore_pb, ignore_mw: part.ignore_mw,
                sf2_mode: part.sf2_mode, sf2_program: part.sf2_program,
                macro_vals: self.core_state_ui.parts[i].macro_vals,
                patch,
            };
            let _ = self.core_state_ui.ctrl_tx.push(ControlEvent::SetPartFull { part: i, state: Box::new(state) });
        }
        // Parts absent from the performance must not keep playing
        // the previous patch with their old voices.
        for i in parts.len()..self.core_state_ui.parts.len() {
            if self.core_state_ui.parts[i].enabled {
                self.core_state_ui.parts[i].enabled = false;
                let _ = self.core_state_ui.ctrl_tx.push(ControlEvent::SetPartEnabled { part: i, enabled: false });
            }
        }
        // Диапазоны партов пересчитываются из сплита ПЕРФОМАНСЕСА (вызов в
        // КОНЦЕ: все парты уже enabled по перфу; вызов до цикла чинил только
        // парт 0 — зона B оставалась 0..127 и молчала на низких нотах,
        // #46/#49)
        self.apply_split_ranges();
    }
}
