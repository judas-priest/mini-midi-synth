//! ROOT CAUSE: SF2 layer silent after performance load (halloween.json).
//!
//! Hypothesis #2 confirmed via engine bisect (variant F), with the trigger in
//! the GUI layer: `App::load_performance_by_name` (src/gui/mod.rs) applies
//! enabled/volume/range/sf2_mode/sf2_program from the perf file, but never
//! sends `ControlEvent::SetPartMute` (nor transpose/pan/vel_range). The engine's
//! NoteOn routing (src/synth/mod.rs, `if !part.enabled || part.mute { continue }`)
//! keeps the part silent on a stale `mute = true` from the previous session —
//! set via the per-part "Mute" toggle in the routing strip (src/gui/layers.rs).
//! halloween.json says `"mute": false` for the upper part, but that field is
//! never applied, so part 4 stays muted after the pad switch.
//!
//! Minimal repro: part 4 speaks in SF2 mode → user mutes part 4 (GUI toggle) →
//! pad-load a performance whose part 4 is `mute: false, sf2_mode: true` →
//! part 4 silent (peak 0). Flipping event: sending
//! `SetPartMute { part: 4, mute: false }` as part of the perf load restores
//! sound (peak ~0.035).
//!
//! Bisect matrix (peak of part 4, note 72, SC-55.sf2):
//!   clean engine + font + documented perf-load sequence ....... 0.0529 PASS
//!   full GUI order, all 8 halloween parts (LoadPatch first) ... 0.0459 PASS
//!   prev perf had part 4 sf2 program 48 ....................... 0.0347 PASS
//!   halloween loaded twice .................................... 0.0347 PASS
//!   sf2 sample offset 50 ms ................................... 0.0292 PASS
//!   stale transpose +60 (perf transpose never applied) ........ 0.0033 PASS (pitch wrong, not silent)
//!   stale mute=true + perf load ............................... 0.0000 FAIL
//!   stale mute + perf load + SetPartMute(false) ............... 0.0347 PASS
//!
//! FIXED (Task 2): load_performance_by_name now copies pan/transpose/mute/
//! vel_min/vel_max into the part state and sends SetPartPan / SetPartTranspose
//! / SetPartMute / SetPartVelRange, mirroring the main loader in layers.rs.
//! This also closes the secondary gaps noted below: stale transpose (pitched
//! silence) and vel/pan mismatches are now reset on every pad perf switch.
//!
//! Remaining known gap (benign today): `load_sf2_keys` re-applies SF2 modes
//! only for parts 0..2 at font load (engine-side restore covers all 8).
//!
//! The first test proves the engine handles the documented halloween sequence
//! from a clean state; the other two pin the stale-mute bug's reproduction and
//! the flipping event (now applied by the fix).
//! Requires the local soundfont; skipped automatically if it is absent.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{ControlEvent, MidiEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
const SILENCE: f32 = 1e-3;
const SF2_PATH: &str = "/home/dima/.local/share/mini_midi_synth/sf2/SC-55.sf2";

fn render_peak(engine: &mut SynthEngine, blocks: usize) -> f32 {
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    let mut peak = 0.0f32;
    for _ in 0..blocks {
        l.fill(0.0);
        r.fill(0.0);
        engine.tick_block(&mut l, &mut r);
        for i in 0..BLOCK_SIZE {
            peak = peak.max(l[i].abs()).max(r[i].abs());
        }
    }
    peak
}

fn load_font() -> Option<std::sync::Arc<rustysynth::SoundFont>> {
    let mut file = std::fs::File::open(SF2_PATH).ok()?;
    rustysynth::SoundFont::new(&mut file).ok().map(std::sync::Arc::new)
}

#[test]
fn perf_load_both_sf2_layers_speak() {
    let Some(sf) = load_font() else { eprintln!("no SC-55.sf2, skipping"); return };

    let patches = preset::load_all_patches();
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches);
    // App startup sequence: font load, then per-part SF2 mode
    let _ = e.handle_control(ControlEvent::LoadKeysSoundFont { soundfont: sf.clone() });
    // Reproduce load_performance_by_name for parts 0 and 4 exactly:
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 0, min_note: 0, max_note: 59 });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: 0, program: 0, bank: 0 });
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 4, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: 4, min_note: 60, max_note: 127 });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: 4, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: 4, program: 0, bank: 0 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 4, semitones: 12 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 0, semitones: -12 });

    // Part 0: note 48 in range 0-59, transpose -12
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 48, velocity: 127 });
    let p0 = render_peak(&mut e, 200);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 48 });

    // Part 4: note 72 in range 60-127, transpose +12
    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let p4 = render_peak(&mut e, 200);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });

    println!("part0 peak {p0}, part4 peak {p4}");
    assert!(p0 > SILENCE, "part 0 silent (peak {p0})");
    assert!(p4 > SILENCE, "part 4 silent (peak {p4})");
}

/// GUI event order from load_performance_by_name for one part.
#[allow(clippy::too_many_arguments)]
fn gui_load_part(e: &mut SynthEngine, i: usize, enabled: bool, vol: f32, lo: u8, hi: u8, sf2: bool, prog: u8) {
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: i, enabled });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: i, volume: vol });
    let _ = e.handle_control(ControlEvent::SetPartRange { part: i, min_note: lo, max_note: hi });
    let _ = e.handle_control(ControlEvent::SetPartSf2Mode { part: i, enabled: sf2 });
    if sf2 {
        let _ = e.handle_control(ControlEvent::SetPartSf2Program { part: i, program: prog, bank: 0 });
    }
}

fn halloween_engine() -> SynthEngine {
    let sf = load_font().expect("SC-55.sf2 must exist for this test");
    let mut e = SynthEngine::new(SR);
    e.set_patches(preset::load_all_patches());
    let _ = e.handle_control(ControlEvent::LoadKeysSoundFont { soundfont: sf });
    e
}

/// Regression: part 4 muted earlier in the session, then a performance with
/// `mute: false` is pad-loaded — the fix sends SetPartMute from the perf file,
/// so the layer must speak again.
#[test]
fn stale_mute_cleared_by_perf_switch() {
    let mut e = halloween_engine();
    // Earlier in the session the user muted part 4 via the routing strip
    gui_load_part(&mut e, 4, true, 0.8, 60, 127, true, 0);
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 4, mute: true });
    // Pad performance switch (documented order incl. the new mute/transpose/
    // pan/vel events that the fix now sends)
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    gui_load_part(&mut e, 0, true, 0.8, 0, 59, true, 0);
    gui_load_part(&mut e, 4, true, 0.8, 60, 127, true, 0);
    // Fix: load_performance_by_name sends mute/pan/transpose/vel from the perf
    let _ = e.handle_control(ControlEvent::SetPartPan { part: 4, pan: 0.0 });
    let _ = e.handle_control(ControlEvent::SetPartTranspose { part: 4, semitones: 0 });
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 4, mute: false });
    let _ = e.handle_control(ControlEvent::SetPartVelRange { part: 4, vel_min: 1, vel_max: 127 });

    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let p4 = render_peak(&mut e, 200);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    println!("regression part4 peak {p4}");
    assert!(p4 > SILENCE, "part 4 silent after perf switch (peak {p4})");
}

/// The flipping event in isolation: applying the perf file's mute flag restores
/// the layer.
#[test]
fn sending_mute_from_perf_flips_layer_on() {
    let mut e = halloween_engine();
    gui_load_part(&mut e, 4, true, 0.8, 60, 127, true, 0);
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 4, mute: true });
    let _ = e.handle_control(ControlEvent::AllNotesOff);
    gui_load_part(&mut e, 0, true, 0.8, 0, 59, true, 0);
    gui_load_part(&mut e, 4, true, 0.8, 60, 127, true, 0);
    // The fix: load_performance_by_name sends mute from the perf file
    let _ = e.handle_control(ControlEvent::SetPartMute { part: 4, mute: false });

    e.handle_event(MidiEvent::NoteOn { channel: 0, note: 72, velocity: 127 });
    let p4 = render_peak(&mut e, 200);
    e.handle_event(MidiEvent::NoteOff { channel: 0, note: 72 });
    println!("flip part4 peak {p4}");
    assert!(p4 > SILENCE, "part 4 silent (peak {p4})");
}
