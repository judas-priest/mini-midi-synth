//! #49-followup / SF2 channel gain regression (2026-11-03 user report:
//! "загружаю перф — напутает громкость и панораму, лечится перезагрузкой").
//! Root cause: SetPartSf2Mode allocated the rustysynth channel AFTER the
//! Volume/Mute/Pan events (which early-return with no channel), so the
//! channel kept the previous occupant's CC7/CC10 (or the GM default).
//! Tests use the GUI push order verbatim (apply_performance, mod.rs:2822).
//! Requires the local soundfont; skipped automatically if it is absent.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;
const SF2_PATH: &str = "/home/dima/.local/share/mini_midi_synth/sf2/SC-55.sf2";

fn load_font() -> Option<std::sync::Arc<rustysynth::SoundFont>> {
    let mut file = std::fs::File::open(SF2_PATH).ok()?;
    rustysynth::SoundFont::new(&mut file).ok().map(std::sync::Arc::new)
}

fn engine_with_sf2(sf: std::sync::Arc<rustysynth::SoundFont>) -> SynthEngine {
    let mut e = SynthEngine::new(SR);
    e.set_patches(preset::load_all_patches());
    let _ = e.handle_control(ControlEvent::LoadKeysSoundFont { soundfont: sf });
    e
}

/// GUI push order for ONE part from apply_performance (mod.rs:2856-2872),
/// reduced to the volume/pan/mute/mode essentials. `lo/hi` — диапазон парта:
/// роутинг звучит во ВСЕ включённые немьютнутые парты с подходящим range
/// (synth/mod.rs:2258-2271), поэтому тесты с двумя партами обязаны дать
/// непересекающиеся диапазоны.
#[allow(clippy::too_many_arguments)]
fn gui_switch_part_to_sf2(e: &mut SynthEngine, part: usize, vol: f32, pan: f32, mute: bool, lo: u8, hi: u8) {
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part, volume: vol });
    let _ = e.handle_control(ControlEvent::SetPartRange { part, min_note: lo, max_note: hi });
    let _ = e.handle_control(ControlEvent::SetPartPan { part, pan });
    let _ = e.handle_control(ControlEvent::SetPartMute { part, mute });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartSf2Program { part, program: 0, bank: 0 });
}

fn peaks(e: &mut SynthEngine, note: u8, blocks: usize) -> (f32, f32) {
    e.handle_event(MidiEvent::NoteOn { channel: 0, note, velocity: 127 });
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let (mut pl, mut pr) = (0.0f32, 0.0f32);
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        e.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            pl = pl.max(l[i].abs());
            pr = pr.max(r[i].abs());
        }
    }
    e.handle_event(MidiEvent::NoteOff { channel: 0, note });
    (pl, pr.max(1e-9))
}

fn overall_peak(e: &mut SynthEngine, note: u8, blocks: usize) -> f32 {
    let (pl, pr) = peaks(e, note, blocks);
    pl.max(pr)
}

#[test]
fn tc1_zero_volume_survives_engine_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 0.0, 0.0, false, 0, 127);
    let p = overall_peak(&mut e, 60, 200);
    assert!(p < SILENCE, "vol 0.0 must stay silent after DSP->SF2 switch (peak {p})");
}

#[test]
fn tc2_mute_survives_engine_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, true, 0, 127);
    let p = overall_peak(&mut e, 60, 200);
    assert!(p < SILENCE, "muted part must stay silent after switch (peak {p})");
}

#[test]
fn tc3_pan_survives_engine_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, -1.0, false, 0, 127);
    let (pl, pr) = peaks(&mut e, 60, 200);
    assert!(pl > pr * 4.0, "pan -1.0 lost on switch: left {pl}, right {pr}");
}

#[test]
fn tc4_volume_value_applied_on_switch() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    // Непересекающиеся диапазоны: иначе NoteOn звучит в ОБОИХ партах и
    // quiet ≈ loud независимо от CC7. Дренаж хвоста перед вторым замером.
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, false, 0, 59);
    let loud = overall_peak(&mut e, 40, 200);
    for _ in 0..100 { let mut l = [0.0f32; BLOCK_SIZE]; let mut r = [0.0f32; BLOCK_SIZE]; e.tick_block(&mut l, &mut r); }
    gui_switch_part_to_sf2(&mut e, 1, 0.1, 0.0, false, 60, 127);
    let quiet = overall_peak(&mut e, 72, 200);
    assert!(quiet < loud * 0.5, "vol 0.1 ({quiet}) not quieter than 1.0 ({loud})");
}

#[test]
fn tc5_noop_switch_keeps_volume_and_no_panic() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };
    let mut e = engine_with_sf2(sf);
    gui_switch_part_to_sf2(&mut e, 0, 1.0, 0.0, false, 0, 127);
    // no-op switch (SF2 -> SF2), затем volume поверх — как perf-load пушит
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 0.5 });
    let p = overall_peak(&mut e, 60, 200);
    assert!(p > SILENCE, "no-op switch must keep the part audible (peak {p})");
}
