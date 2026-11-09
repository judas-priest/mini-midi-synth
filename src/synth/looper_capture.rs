//! Looper-capture машина: quantized arm→record→close, overdub, cycle
//! add/remove, clock-death fallback. Вынесено из tick_block verbatim
//! (techdebt Strategic 3) — behavior-preserving move.
use super::*;

impl SynthEngine {
    pub(super) fn looper_capture_tick(&mut self, step: u8) {
        // Quantized master recording: pattern boundary = sequencer wrapped to
        // step 0 (edge-detected). Arm -> start capture (петля = 1 цикл
        // паттерна — размеры лупера и секвенсора совпадают); каждая граница
        // считает целый цикл, второй Rec закрывает на СЛЕДУЮЩЕЙ границе —
        // петля = РОВНО N целых циклов.
        let prev = self.looper_prev_step;
        self.looper_prev_step = step;
        let playing = self.drum_engine.sequencer.playing;
        let boundary = playing && step == 0 && prev != 0;
        let quantized_looping = self.looper_capture_quantized
            && matches!(
                self.looper.state,
                crate::synth::looper::LooperState::Recording
                    | crate::synth::looper::LooperState::Overdubbing
                    | crate::synth::looper::LooperState::Playing
            );
        // #32 Phase 5: pattern switch at a boundary (quantized GUI queue or
        // immediate console switch) resizes the looper cycle to the new
        // pattern length — while capturing, or while a 1-cycle-of-old-pattern
        // loop plays. Free (non-quantized) loops keep their length. Drift
        // correction below keeps its own guard and needs no change here.
        let cur_pat = self.drum_engine.sequencer.current_pattern;
        let pat_switched = cur_pat != self.looper_prev_pattern;
        if pat_switched { self.looper_prev_pattern = cur_pat; }
        if boundary && pat_switched {
            let one_cycle_of_old = self.looper_cycle_ticks > 0
                && self.looper.loop_len_ticks() == self.looper_cycle_ticks;
            if quantized_looping || one_cycle_of_old {
                let pattern_len =
                    self.drum_engine.sequencer.patterns[cur_pat as usize].length.max(1);
                let bpm = self.drum_engine.sequencer.bpm.max(1.0);
                let samples_per_step =
                    (self.sample_rate as f64 * 60.0) / (bpm as f64 * 4.0);
                let new_cycle = (pattern_len as f64 * samples_per_step) as u32;
                self.looper_cycle_ticks = new_cycle;
                self.keep_capture_capacity();
                if self.looper_rec_cycles > 0 {
                    self.looper.set_loop_len_ticks(
                        self.looper_rec_cycles.saturating_mul(new_cycle));
                } else {
                    self.looper.set_loop_len_ticks(new_cycle);
                }
            }
        }
        if self.looper_arm_pending
            || self.looper_finish_pending
            || quantized_looping
        {
            let lstate = self.looper.state;
            // Квантованный захват может УЖЕ самообернуться в Overdubbing
            // (looper.tick() переключает Recording->Overdubbing по своей длине
            // петли на несколько сэмплов РАНЬШЕ, чем детект границы такта) —
            // закрывать надо и его.
            let capturing = lstate == crate::synth::looper::LooperState::Recording
                || (lstate == crate::synth::looper::LooperState::Overdubbing
                    && self.looper_capture_quantized
                    && !self.looper_overdub_active);
            if boundary && self.looper_overdub_pending && lstate == crate::synth::looper::LooperState::Playing {
                // Слой: старт поверх петли на границе (длина петли не меняется)
                self.looper_overdub_pending = false;
                self.looper_overdub_active = true;
                self.looper.toggle_record(); // Playing -> Overdubbing
            } else if boundary
                && lstate == crate::synth::looper::LooperState::Overdubbing
                && self.looper_overdub_active
            {
                if self.looper_finish_pending {
                    self.looper_finish_pending = false;
                    self.looper_overdub_active = false;
                    self.looper.toggle_record(); // Overdubbing -> Playing
                }
            } else if boundary && self.looper_arm_pending {
                self.looper_arm_pending = false;
                self.looper.toggle_record(); // Idle -> Recording (length = bars * bar)
                // Boss RC-семантика: петля = Bars × цикл паттерна; авто-закрытие
                // после Bars циклов; тики записи НЕПРЕРЫВНЫ (wrap только на
                // конце всей петли) — релокация не нужна.
                let pat = self.drum_engine.sequencer.current_pattern as usize;
                let pattern_len = self.drum_engine.sequencer.patterns[pat].length.max(1);
                let bpm = self.drum_engine.sequencer.bpm.max(1.0);
                let samples_per_step = (self.sample_rate as f64 * 60.0) / (bpm as f64 * 4.0);
                let cycle = (pattern_len as f64 * samples_per_step) as u32;
                // Безостановочный захват: лупер с запасом (64 цикла максимум) —
                // playback_tick НЕ оборачивается, тики событий глобальные.
                // Реальная длина = looper_rec_cycles циклов; Rec закрывает.
                self.looper.set_loop_len_ticks(cycle.saturating_mul(64));
                // GUI-позиция = прогресс текущего цикла (см. set_display_cycle_ticks)
                self.looper.set_display_cycle_ticks(Some(cycle));
                self.looper_cycle_ticks = cycle;
                self.looper_rec_cycles = 1;
                self.set_master_rec_state(2, 1); // capturing, cycle 1
            } else if boundary && capturing {
                // Закрытие — ТОЛЬКО по Rec (finish_pending). Безостановочный режим.
                if self.looper_finish_pending {
                    self.looper_finish_pending = false;
                    let kept = self.looper_rec_cycles.saturating_mul(self.looper_cycle_ticks);
                    self.looper.trim_after(kept);
                    self.looper.set_loop_len_ticks(kept);
                    self.looper.set_display_cycle_ticks(None);
                    self.looper_rec_cycles = 0;
                    // REC/CLOSING-атом надо гасить ЗДЕСЬ: единственные сбросы
                    // были в master_stop/disarm — GUI висел в «REC n тактов»/
                    // «CLOSING» с мёртвыми +1/−1 (audit rhythm #1).
                    self.set_master_rec_state(0, 0);
                    self.drum_engine.set_recording(false); // сетка тоже разармирована
                    self.looper.toggle_record(); // Recording/Overdubbing -> Playing
                } else {
                    // Новый цикл начался: логическая длина +1, ёмкость с запасом
                    self.looper_rec_cycles += 1;
                    self.set_master_rec_state(2,
                        self.looper_rec_cycles.min(255) as u8);
                    self.keep_capture_capacity();
                }
            } else if boundary
                && lstate == crate::synth::looper::LooperState::Playing
                && self.looper.loop_len_ticks() == self.looper_cycle_ticks
            {
                // Drift correction (Mobius-style): только для петли = 1 цикл —
                // каждый круг выравнивает петлю в ноль. Для N>1-цикловых петель
                // seek убивал бы вторую половину (длина != cycle) — не трогаем.
                self.looper.seek_start();
            } else if !playing && capturing {
                // fallback: часы умерли посреди записи — закрыть сейчас
                // (нечётный кусок; длину петли не перекраиваем)
                self.looper_finish_pending = false;
                self.looper.set_display_cycle_ticks(None);
                self.looper_rec_cycles = 0;
                // REC/CLOSING-атом надо гасить ЗДЕСЬ: единственные сбросы
                // были в master_stop/disarm — GUI висел в «REC n тактов»/
                // «CLOSING» с мёртвыми +1/−1 (audit rhythm #1).
                self.set_master_rec_state(0, 0);
                self.drum_engine.set_recording(false);
                self.looper.toggle_record();
            }
        }
    }
}
