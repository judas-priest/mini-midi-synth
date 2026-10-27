# Tech Debt / Backlog

Recorded 2026-10-27. Ordered roughly by user-perceived impact.

## 1. Metronome tied to sequencer BPM
- Audible click (toggleable) synced to the drum-sequencer BPM; plus a visual click indicator in the GUI (step highlight already exists — add a downbeat accent + GUI flash).
- Click sound: synthesize a short tick in the drum engine (accent on beat 1) — no sample needed.
- Should follow the same BPM source as the sequencer + looper sync (`looper_sync_bpm`).

## 2. Sequencer records only one layer, not the whole performance
- Current: drum step sequencer records hits into the active drum pattern; part/looper recording targets a single layer (`active_part`). Expected: "record" captures the whole performance (all enabled parts), not just one layer.
- Needs design: multi-layer capture is what the looper already does (audio). Decide: extend looper to capture all enabled layers, or make the sequencer/PitchSeq record across parts.

## 3. Default empty init performance
- No factory "Init" performance exists — pad perf slots start empty and the first saved perf becomes the reference.
- Add `Init` performance (single part A1 = init preset, full range, no SF2) compiled into the binary the same way the Init preset is in `FACTORY_PRESETS`; reset action restores it.

## 4. Panorama: works for DSP parts, not for SF2 parts
- DSP: per-part pan applied in `tick_block` (constant-power, mod.rs ~2560).
- SF2: `sampler.tick()` output is summed into the mix BEFORE the per-part pan? — verify actual routing; rustysynth has its own stereo field per preset and our pan likely never reaches SF2 channels.
- Options: pan SF2 channels via rustysynth CC10 (check if vendor implements CC10 — likely not; vendor patch), or split sampler output per part channel and apply pan in the mix (rustysynth renders all channels mixed — needs per-channel render, expensive) — investigate before deciding.

## Carried-over (from the eternal-sustain audit, out of scope there)
- Mute is not applied in the mix — only gates new note-ons (mod.rs tick_block checks `enabled` only). Muted part keeps sounding if notes started before mute.
- `seq_program_set` desyncs `part_program[]` cache: MIDI-file program changes overwrite a live part's rustysynth channel program; next `load_keys_soundfont` restore clobbers the sequencer's program mid-playback.
- CC120 (All Sound Off) handled as release-style note_off, not an immediate kill.
- Sustain-atom divergence: `sampler.all_notes_off()` force-clears sampler pedal state while engine atom/part flags may stay ON (MidiSeqLoad/Play-stop paths) — DSP and SF2 sustain can diverge until the next pedal edge.
- Panic-path arp flushes (mod.rs ~1722/~2047/~2198) dispatch untransposed off_notes — harmless today (full drains follow) but drifts from `flush_arp_note_off`.
- `SetPartEnabled{false}` runs its cleanup block twice (drain + legacy graceful note-off) — collapse.
- `cargo deny`: rtrb 0.3.3 advisory RUSTSEC-2026-0274 — `cargo update -p rtrb` (→ 0.3.5).
