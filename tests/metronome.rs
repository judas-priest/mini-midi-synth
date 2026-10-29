//! Metronome click synced to the drum step sequencer.
//! Plan: docs/superpowers/plans/2026-10-29-metronome-init-perf.md

use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};
use std::sync::atomic::Ordering;

const SR: f32 = 48_000.0;
const CLICK_THRESHOLD: f32 = 0.05;

fn metronome_engine(bpm: f32, on: bool) -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on });
    let _ = e.handle_control(ControlEvent::DrumSeqBpm { bpm });
    e
}

/// Per-block peaks; consecutive loud blocks merge into one click event.
/// Returns (first_block_index, peak) per event.
fn click_events(e: &mut SynthEngine, blocks: usize) -> Vec<(usize, f32)> {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut events: Vec<(usize, f32)> = Vec::new();
    let mut loud_run = false;
    for b in 0..blocks {
        e.tick_block(&mut l, &mut r);
        let peak = l.iter().fold(0.0f32, |m, &s| m.max(s.abs()))
            .max(r.iter().fold(0.0f32, |m, &s| m.max(s.abs())));
        if peak > CLICK_THRESHOLD {
            if loud_run {
                let last = events.last_mut().unwrap();
                last.1 = last.1.max(peak);
            } else {
                events.push((b, peak));
                loud_run = true;
            }
        } else {
            loud_run = false;
        }
    }
    events
}

#[test]
fn tc1_click_on_every_beat_at_120bpm() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    // 120 BPM: beat = 0.5 s = 24000 samples = 750 blocks; first boundary fires at sample 6000 (block ~187) after DrumSeqPlay resets step 0 of 32.
    // 2260 blocks (~3 s) → beats land at blocks ~187 / 937 / 1687 (после DrumSeqPlay первый беат приходит рано — старт с границы шага 0).
    let events = click_events(&mut e, 2260);
    assert!(events.len() >= 2, "expected >= 2 beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750");
    }
}

#[test]
fn tc2_accent_on_step_zero_is_louder() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let events = click_events(&mut e, 2260);
    assert!(events.len() >= 3, "expected >= 3 beats, got {events:?}");
    // Play resets step to 0 → first event is the step-0 accent (gain 0.9),
    // second is step 4 (gain 0.55).
    assert!(events[0].1 > events[1].1 * 1.2,
        "accent peak {} not louder than beat peak {}", events[0].1, events[1].1);
}

#[test]
fn tc3_metronome_silent_when_not_playing() {
    let mut e = metronome_engine(120.0, true);
    // ~1 beat worth of blocks; transport never started.
    assert!(click_events(&mut e, 1600).is_empty(), "click without playback");
}

#[test]
fn tc4_toggle_off_stops_clicks() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    assert!(!click_events(&mut e, 800).is_empty(), "no clicks while enabled+playing");
    let _ = e.handle_control(ControlEvent::DrumSetMetronome { on: false });
    assert!(click_events(&mut e, 800).is_empty(), "clicks persist after toggle-off");
}

#[test]
fn tc5_config_missing_drum_metronome_defaults_off() {
    // Legacy config.json without the new UiSettings field must parse to off.
    let json = r#"{"audio":{"backend":"JACK","sample_rate":48000,"buffer_size":256},
"midi":{"port_name":null},"ui":{"window_width":720.0,"window_height":400.0,"last_preset":null}}"#;
    let cfg: mini_midi_synth::config::Config =
        serde_json::from_str(json).expect("legacy config must parse");
    assert!(!cfg.ui.drum_metronome);
    assert!(!mini_midi_synth::config::Config::default().ui.drum_metronome);
}

/// Stitch two click_events runs into one absolute timeline: `tail` block
/// indices are shifted by `pre_blocks`.
fn stitched(mut pre: Vec<(usize, f32)>, mut tail: Vec<(usize, f32)>, pre_blocks: usize)
    -> Vec<(usize, f32)>
{
    for t in tail.iter_mut() { t.0 += pre_blocks; }
    pre.append(&mut tail);
    pre
}

fn assert_beat_intervals(events: &[(usize, f32)], min_events: usize) {
    assert!(events.len() >= min_events, "expected >= {min_events} beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750: {events:?}");
    }
}

/// Requirement: metronome click must keep sounding at the same beat interval
/// when sequencer recording starts mid-playback (TECHDEBT #22a).
#[test]
fn tc6_rec_mid_playback_click_continues() {
    let mut e = metronome_engine(120.0, true);
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let pre = click_events(&mut e, 500);
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    let tail = click_events(&mut e, 1250);
    assert!(!pre.is_empty(), "no clicks before rec");
    assert!(!tail.is_empty(), "clicks vanished after rec start");
    assert_beat_intervals(&stitched(pre, tail, 500), 3);
}

/// Same via the keyboard transport path (pad note 83 on ch 9).
#[test]
fn tc7_keyboard_rec_note83_click_continues() {
    let mut e = metronome_engine(120.0, true);
    e.seq_target_atom().store(0, Ordering::Relaxed); // pad transport: drums
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let pre = click_events(&mut e, 500);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 });
    e.handle_event(MidiEvent::NoteOff { channel: 9, note: 83 });
    let tail = click_events(&mut e, 1250);
    assert!(!pre.is_empty(), "no clicks before kb rec toggle");
    assert!(!tail.is_empty(), "clicks vanished after kb rec toggle");
    assert_beat_intervals(&stitched(pre, tail, 500), 3);
}

/// Rec from a stopped transport auto-plays (commit 0983413) — the click must
/// START (it was silent while stopped by design).
#[test]
fn tc8_rec_from_stopped_autoplay_clicks_start() {
    let mut e = metronome_engine(120.0, true);
    assert!(click_events(&mut e, 400).is_empty(), "click while stopped");
    let _ = e.handle_control(ControlEvent::DrumSeqRecord { recording: true });
    let events = click_events(&mut e, 1600);
    assert_beat_intervals(&events, 2);
}

/// Double press of note 83: rec ends OFF, click timeline must not break.
#[test]
fn tc9_double_toggle_click_continues() {
    let mut e = metronome_engine(120.0, true);
    e.seq_target_atom().store(0, Ordering::Relaxed); // pad transport: drums
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    let pre = click_events(&mut e, 500);
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 }); // rec on
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 127 }); // rec off
    let tail = click_events(&mut e, 1250);
    assert!(!tail.is_empty(), "clicks vanished after double toggle");
    assert_beat_intervals(&stitched(pre, tail, 500), 3);
}

/// NoteOn with velocity 0 is the MIDI note-off encoding: it must NOT toggle
/// rec (no auto-play, hence silence while the transport is stopped).
#[test]
fn tc10_note83_velocity0_does_not_toggle_rec() {
    let mut e = metronome_engine(120.0, true);
    e.seq_target_atom().store(0, Ordering::Relaxed); // pad transport: drums
    e.handle_event(MidiEvent::NoteOn { channel: 9, note: 83, velocity: 0 });
    assert!(click_events(&mut e, 800).is_empty(),
        "velocity-0 toggled rec / started transport");
}

// --- Time signatures (TECHDEBT #21): метроном уже корректен при любой длине.
// 120 BPM, 48 kHz, BLOCK_SIZE=32 → шаг = 6000 сэмплов = 187.5 блоков, доля
// (4 шага) = 750 блоков. DrumSeqPlay сбрасывает step в 0, первый клик приходит
// рано (~блок 187) — интервалы меряем windows().

fn engine_with_length(bpm: f32, length: u8) -> SynthEngine {
    let mut e = metronome_engine(bpm, true);
    let _ = e.handle_control(ControlEvent::DrumSeqLength { length });
    let _ = e.handle_control(ControlEvent::DrumSeqPlay { playing: true });
    e
}

/// Акценты (пики > 1.2× соседнего бита) идут с периодом `bar_events` событий.
/// Последнее событие не проверяется: замыкание периода сравнивало бы такт
/// с тактом (равные gain'и), а не такт с долей.
fn assert_bar_period(events: &[(usize, f32)], bar_events: usize) {
    assert!(events.len() >= 2 * bar_events, "too few events: {events:?}");
    for i in 0..events.len() - 1 {
        let is_bar_start = i % bar_events == 0;
        let next = events[i + 1].1;
        let louder = events[i].1 > next * 1.2;
        if is_bar_start {
            assert!(louder, "event {i} (bar start) not accented: {events:?}");
        } else {
            assert!(!louder, "event {i} unexpectedly accented: {events:?}");
        }
    }
}

/// TC11 (3/4, length=12): доли 0,4,8; такт = 12 шагов = 3 события. ~6000 блоков ≈ 3 такта.
#[test]
fn tc11_three_quarter_length_12_clicks_and_accents() {
    let mut e = engine_with_length(120.0, 12);
    let events = click_events(&mut e, 6000);
    assert!(events.len() >= 6, "expected >= 6 beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750");
    }
    assert_bar_period(&events, 3);
}

/// TC12 (2/4, length=8): доля = 4 шага, такт = 2 события.
#[test]
fn tc12_two_quarter_length_8_clicks_and_accents() {
    let mut e = engine_with_length(120.0, 8);
    let events = click_events(&mut e, 6000);
    assert!(events.len() >= 6, "expected >= 6 beats, got {events:?}");
    for w in events.windows(2) {
        let dt = (w[1].0 - w[0].0) as f32;
        assert!((dt - 750.0).abs() < 4.0, "beat interval {dt} blocks, expected ~750");
    }
    assert_bar_period(&events, 2);
}

/// TC13 (free, length=5): клики на шагах 0 и 4 → интервалы чередуются
/// 750 / 187.5. Акцент на шаге 0 = каждое ЧЁТНОЕ событие.
#[test]
fn tc13_free_length_5_clicks_at_0_and_4() {
    let mut e = engine_with_length(120.0, 5);
    let events = click_events(&mut e, 6000);
    assert!(events.len() >= 6, "expected >= 6 clicks, got {events:?}");
    for (i, w) in events.windows(2).enumerate() {
        let dt = (w[1].0 - w[0].0) as f32;
        // Чётное событие = такт (шаг 0) → до следующего 4 шага; нечётное = шаг 4 → 1 шаг до края такта.
        let expect = if i % 2 == 0 { 750.0 } else { 187.5 };
        assert!((dt - expect).abs() < 4.0, "interval {dt} blocks, expected ~{expect}: {events:?}");
    }
    assert_bar_period(&events, 2);
}
