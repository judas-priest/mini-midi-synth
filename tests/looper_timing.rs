//! Engine-level looper timing (REFACTOR_AUDIT S0): LooperRestore + play must
//! fire recorded events at sample-accurate positions through tick_block,
//! and the loop wrap must repeat them every pass.

use mini_midi_synth::preset;
use mini_midi_synth::synth::{looper::LooperEventSnapshot, ControlEvent, SynthEngine, BLOCK_SIZE};

const SR: f32 = 48_000.0;
/// Loop 24000 ticks = exactly 750 BLOCK_SIZE blocks — block-aligned passes.
const LOOP_TICKS: u32 = 24_000;
const LOOP_BLOCKS: usize = LOOP_TICKS as usize / BLOCK_SIZE;
const PASSES: usize = 3;

fn engine_with_note() -> SynthEngine {
    let patches = preset::load_all_patches();
    let mut patch = patches
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case("init"))
        .or_else(|| patches.first())
        .expect("factory presets are compiled in")
        .clone();
    // init's amp_release is 0.3 s — the release tail of a pass still sits
    // around 0.09 peak two passes later and would drown the 1e-3 attack
    // detector. Shorten it so each pass's tail dies inside the 3-block skip.
    patch.params.insert("amp_release".into(), 0.02);
    let mut e = SynthEngine::new(SR);
    e.set_patches(patches);
    let spent = e.handle_control(ControlEvent::load_patch_from(0, &patch));
    drop(spent);
    let _ = e.handle_control(ControlEvent::SetPartEnabled { part: 0, enabled: true });
    let _ = e.handle_control(ControlEvent::SetPartVolume { part: 0, volume: 1.0 });
    e
}

#[test]
fn looper_replays_events_at_recorded_sample_positions() {
    let mut e = engine_with_note();
    // note-on at tick 1000 (block 31.25), note-off at tick 2000 (block 62.5)
    let events: Box<[LooperEventSnapshot]> = vec![
        LooperEventSnapshot { tick: 1000, note: 60, velocity: 127, layer_id: 0, part_id: 0 },
        LooperEventSnapshot { tick: 2000, note: 60, velocity: 0, layer_id: 0, part_id: 0 },
    ]
    .into_boxed_slice();
    let spent = e.handle_control(ControlEvent::LooperRestore { events, loop_length_ticks: LOOP_TICKS });
    drop(spent); // heap payload comes back out of handle_control
    let _ = e.handle_control(ControlEvent::LooperTogglePlay); // Playing from tick 0

    let mut first_sound = [None; PASSES];
    let mut l = [0.0f32; BLOCK_SIZE];
    let mut r = [0.0f32; BLOCK_SIZE];
    for b in 0..PASSES * LOOP_BLOCKS {
        e.tick_block(&mut l, &mut r);
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |m, &s| m.max(s.abs()));
        let pass = b / LOOP_BLOCKS;
        let in_pass = b % LOOP_BLOCKS;
        // skip the first 3 blocks of a pass: release tail of the previous
        // pass must not be mistaken for the new attack
        if peak > 1e-3 && in_pass > 3 && first_sound[pass].is_none() {
            first_sound[pass] = Some(in_pass);
        }
    }
    for (p, fs) in first_sound.iter().enumerate() {
        let fs = fs.unwrap_or_else(|| panic!("pass {p}: looper never sounded"));
        // tick 1000 lives in block 31 (ticks 992..1023)
        assert!(
            (29..=33).contains(&fs),
            "pass {p}: first sounding block {fs}, expected ~31 (event at tick 1000)"
        );
    }
}
