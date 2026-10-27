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

## 5. Chord detection for recording
- Problem: sequencer/looper records every played note; user wants to play a chord shape and have a single chord recorded (smart chord capture), not N separate notes.
- Research when picked up: chord detection algorithms (pitch-class-set → chord name/midi chord), how Smart Chord on M-VAVE / DAW chord tracks capture; likely: detect simultaneously-pressed notes within a window, collapse to one chord event.

## 6. Root-note-only mode for a layer (bass split)
- Problem: many layers play the full chord; bass gets muddy. User wants a per-layer mode "play only the root note" (lowest played note / detected chord root), transposed down an octave — bass layer follows the chord root while other layers play the chord.
- Research when picked up: chord-root detection from held notes (same detection as #5), existing implementations (arpeggiator root mode, "bass follow" in workstations).
- Depends on #5 (root detection).

## 7. Custom sustain release (SR) — вернуть как статичную настройку
- Research done 2026-10-27 (best practices: Ravenscroft 275, Ivory II, NI Claire).
- Industry standard: TWO-VALUE model — release while pedal held (native/long) vs
  release after pedal-up (user-configurable). NOT coupled to fader/pedal
  position live: scaling the tail by pedal position makes fast pedal taps chop
  tails and is unplayable (this exact failure was reverted in 59ed413).
- Half-pedal (continuous pedal scaling release+resonance) needs a continuous
  pedal with configurable Min/Max CC range — out of scope until such a pedal.
- Re-pedal (quick off→on re-catch) already works.
- Implementation exists in git history: DSP override `Envelope::note_off_for` +
  `SetSustainRelease` (commit 9a200a4), SF2 vendor `set_release_time_scale`
  (commit 1d11ee2, vendor patch, default 1.0 = file defaults). Restore as a
  GUI static setting "Pedal-up release: default / X ms", applies to both
  engines; never read CC64 value into it.

## 8. Bank 2 (knobs/pads) config — MidiSuite под Wine не работает
- Fader bank 2 передаёт события из коробки; knob bank 2 и pad bank 2 молчат
  (проверено aseqdump: клава не шлёт ничего на этих банках — пустые назначения).
- Назначения делаются только в MidiSuite (Win/Mac/Android). Под Wine приложение
  запускается, но Flutter-окно не создаётся (BLE/WinUSB-плагины) — тупик.
- Пути: телефон + приложение MidiSuite по BT, или Windows-машина/VM.
- После назначения CC на кнобы банка 2 — забиндить через MIDI Learn; пэды банка 2
  (фактически шлют ноты 74/77/81/83/84 на канале 1) — нацелить транспорт секвенсора.

## 8.1 Bank 2 config via SysEx — feasibility audit (repos cloned to ~/Projects/mvave-research)
- Transport from Linux: POSSIBLE. M-VAVE SysEx header F0 00 32 45..., handshake
  F0 00 32 45 00 00 00 40 7F F7, proven over standard USB-MIDI ports by
  smk37-firmware-custom-mod/tools/smk_ota_win.py (winmm → maps 1:1 to ALSA).
  MidiSuite mobile works over BLE-MIDI → config protocol rides as ordinary
  MIDI bytes (SysEx), no vendor USB transfers needed.
- MISSING: the knob/pad assignment message format is undocumented in both
  repos (fm1-custom-fw has FM-1 synth OTA/voice protocol only;
  smk37-firmware-custom-mod is BLE-firmware patches + audio-only ASIO driver).
- To obtain the protocol: capture MidiSuite traffic (Windows + loopMIDI, or
  Android BLE HCI snoop log while assigning a bank-2 knob/pad), then write a
  Python replay script via amidi/mido. Fallback: firmware RE
  (firmware_work/analysis/functions_catalog.csv + docs/FIRMWARE_STRUCTURE.md
  locate the USB-MIDI SysEx dispatcher in the AC791N image).
- Manual confirms: knobs/pads messages are configurable only via MidiSuite
  (smk37-firmware-custom-mod/firmware_work/manual_text.txt:115-121).
