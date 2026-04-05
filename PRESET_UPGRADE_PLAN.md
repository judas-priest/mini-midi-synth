# Preset Upgrade Plan

Audited 2026-04-05. Based on reading 32 representative presets and researching musical applications of each DSP module.

---

## Module: WaveShaper

**Best musical fit:** Waveshaping adds controlled harmonic distortion — from subtle tube-style saturation to aggressive clipping — which thickens bass, adds bite to leads, and gives analog character to digital oscillators. It is most valuable where a sound needs grit, presence, or the "driven" quality of real analog circuits.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| reese_bass.json | The two detuned saws are clean; soft saturation glues them and adds the sub harmonic thickness characteristic of Reese basslines in D&B. | `wave_shaper_drive: 0.35`, `wave_shaper_mode: 1` (soft clip), `wave_shaper_mix: 0.55` |
| tb303_acid.json | Real 303 filter distorts on high resonance; waveshaping after the filter reproduces that hard-clip edge that distinguishes acid from mere filter sweeps. | `wave_shaper_drive: 0.55`, `wave_shaper_mode: 2` (hard clip), `wave_shaper_mix: 0.4` |
| moog_lead.json | Ladder filter already thickens the sound but the oscillators are clean; a gentle fold gives the odd-harmonic richness of a real Minimoog pushed into saturation. | `wave_shaper_drive: 0.28`, `wave_shaper_mode: 3` (fold), `wave_shaper_mix: 0.45` |
| screaming_lead.json | High-resonance square stack needs harmonic density above the filter cutoff — a moderate wavefold converts filter-peak ringing into harmonic spread. | `wave_shaper_drive: 0.4`, `wave_shaper_mode: 3` (fold), `wave_shaper_mix: 0.35` |
| bass_finger.json | The physical bass model has realistic pluck but no amp-sim grit; soft clipping at low drive mimics a slightly overdriven DI signal. | `wave_shaper_drive: 0.18`, `wave_shaper_mode: 1` (soft clip), `wave_shaper_mix: 0.3` |

---

## Module: MsTool

**Best musical fit:** Mid-side processing lets you independently control center content (Mid) and stereo width (Side), and the rotation parameter enables creative stereo field manipulation. It is most useful on pads and strings with unison spread, where tightening the mono center and expanding the sides creates a wider, more enveloping sound without phase problems at low frequencies.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| cs80_strings.json | 5-voice unison with full spread can sound wide but diffuse; boosting the Mid slightly re-centers the fundamental while the Side boost keeps the shimmer. | `ms_mid_gain: 1.05`, `ms_side_gain: 1.25`, `ms_rotation: 0.0`, `ms_mix: 0.8` |
| blade_runner.json | 4-voice unison pad already has spread 0.8; narrowing the low-mid image with a slight rotation gives the characteristic Vangelis mono-center/wide-top quality. | `ms_mid_gain: 1.0`, `ms_side_gain: 1.3`, `ms_rotation: 4.0`, `ms_mix: 0.7` |
| juno_pad.json | 3-voice unison with moderate spread; a small side boost adds width without altering the Juno's characteristic center punch. | `ms_mid_gain: 1.0`, `ms_side_gain: 1.2`, `ms_rotation: 0.0`, `ms_mix: 0.65` |
| string_pad.json | Tripled saws with ±0.015 detune create natural width that could be enhanced; modest side expansion and a slight mid cut opens the stereo image. | `ms_mid_gain: 0.95`, `ms_side_gain: 1.3`, `ms_rotation: 0.0`, `ms_mix: 0.75` |
| ambient_pad.json | Slow attack pad benefits from a very wide stereo image; large side boost transforms a narrow detuned unison into an enveloping atmospheric texture. | `ms_mid_gain: 0.9`, `ms_side_gain: 1.45`, `ms_rotation: 0.0`, `ms_mix: 0.8` |

---

## Module: Exciter

**Best musical fit:** Harmonic exciters add synthesized upper harmonics (typically 3–12 kHz) through bandlimited saturation, adding "air" and presence without the harshness of broadband distortion. They are most effective on strings, choir, and acoustic instruments where the high-frequency partials define realism and cut-through, and on pads that sound muffled after heavy reverb.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| violin.json | The physical model has correct low/mid resonance but can lack the scratchy upper-partial "scratch" of a real bow; exciting above 4 kHz restores that. | `exciter_drive: 0.3`, `exciter_freq: 4000.0`, `exciter_presence: 0.6`, `exciter_mix: 0.25` |
| cello.json | Same physical model as violin with lower body; an exciter focused above 3 kHz adds the woody rosin-scratch without brightening the fundamental. | `exciter_drive: 0.25`, `exciter_freq: 3000.0`, `exciter_presence: 0.5`, `exciter_mix: 0.2` |
| female_choir.json | Formant oscillator chorus sounds good in mids but lacks the "breath" and sibilance of real soprano voices; exciting above 5 kHz adds vocal air. | `exciter_drive: 0.2`, `exciter_freq: 5000.0`, `exciter_presence: 0.7`, `exciter_mix: 0.2` |
| cs80_strings.json | 5-voice unison with no reverb sounds slightly hollow on top; a gentle exciter above 3.5 kHz adds the silky CS-80 high-mid shimmer. | `exciter_drive: 0.22`, `exciter_freq: 3500.0`, `exciter_presence: 0.55`, `exciter_mix: 0.22` |
| rhodes_piano_v2.json | The e-piano model has correct bell tone but little tine "sparkle" above 4 kHz; a low-drive exciter mimics the acoustic tine brightness of a real Rhodes. | `exciter_drive: 0.18`, `exciter_freq: 4500.0`, `exciter_presence: 0.5`, `exciter_mix: 0.18` |

---

## Module: FloatyDelay

**Best musical fit:** FloatyDelay is a tape-style delay with wow/flutter modulation (wobble/rate parameters), high-frequency damping, and feedback — the defining character of vintage tape echo units (Echoplex, RE-201). The pitch-instability of worn tape adds organic warmth. It is most suited to leads and electro-acoustic sounds where a regular clean delay would sound too digital.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| shakuhachi.json | Dry reverb already applied; a short floaty delay with high flutter mimics the slap-back of a shakuhachi recorded in a stone room. | `floaty_time: 0.22`, `floaty_feedback: 0.25`, `floaty_wobble: 0.35`, `floaty_rate: 1.8`, `floaty_damp: 0.55`, `floaty_mix: 0.18` |
| blade_runner.json | Long attack pad; a medium delay with subtle flutter thickens the texture without destroying the slow build — similar to the Yamaha CS-80 live sound. | `floaty_time: 0.38`, `floaty_feedback: 0.35`, `floaty_wobble: 0.2`, `floaty_rate: 0.9`, `floaty_damp: 0.45`, `floaty_mix: 0.22` |
| dx7_epiano.json | DX7 pianos classically used the Roland CE-1 chorus/echo; a floaty delay with light wobble approximates that seasick tape quality. | `floaty_time: 0.28`, `floaty_feedback: 0.2`, `floaty_wobble: 0.28`, `floaty_rate: 1.2`, `floaty_damp: 0.4`, `floaty_mix: 0.2` |
| flute.json | Short, slightly modulated echo evokes the natural acoustic delay of a flute in a wooden room and prevents the sound from being too dry. | `floaty_time: 0.18`, `floaty_feedback: 0.15`, `floaty_wobble: 0.15`, `floaty_rate: 2.2`, `floaty_damp: 0.6`, `floaty_mix: 0.15` |
| ethereal_pad.json | Chorus_mix is already 0.5; adding a floaty delay with high wobble turns the chorus-washed pad into a slowly-drifting ambient texture. | `floaty_time: 0.45`, `floaty_feedback: 0.4`, `floaty_wobble: 0.5`, `floaty_rate: 0.6`, `floaty_damp: 0.35`, `floaty_mix: 0.25` |

---

## Module: Reverb2

**Best musical fit:** Reverb2 is an FDN (Feedback Delay Network) reverb capable of large hall and plate simulation with separate damping and size control — the standard academic tool for concert-hall placement of orchestral and ambient sounds. FDN reverbs are distinctly superior to simple Schroeder designs for strings and choir because of their smooth, diffuse late field and frequency-dependent decay.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| cs80_strings.json | Currently has no reverb at all; a large hall reverb places the 5-voice unison in a concert-hall space and is essential for this preset to breathe. | `reverb2_decay: 2.8`, `reverb2_damping: 0.35`, `reverb2_size: 0.75`, `reverb2_mix: 0.28` |
| male_choir.json | No reverb currently; choir without room sounds like a dry recording booth — a medium-large hall transform it to cathedral space. | `reverb2_decay: 3.5`, `reverb2_damping: 0.25`, `reverb2_size: 0.85`, `reverb2_mix: 0.32` |
| female_choir.json | Same as male choir — no reverb; a slightly brighter (lower damping) hall with longer decay suits soprano voices. | `reverb2_decay: 4.0`, `reverb2_damping: 0.18`, `reverb2_size: 0.88`, `reverb2_mix: 0.35` |
| dark_pad.json | No reverb at all on this preset; a long decay with high damping gives the low-cutoff pad a dark, cavernous space without cluttering the frequency. | `reverb2_decay: 4.5`, `reverb2_damping: 0.7`, `reverb2_size: 0.9`, `reverb2_mix: 0.3` |
| fm_bell.json | Bell tones need long, diffuse reverb tails — the current preset has no reverb, leaving it sounding isolated and un-physical. | `reverb2_decay: 3.2`, `reverb2_damping: 0.2`, `reverb2_size: 0.6`, `reverb2_mix: 0.25` |

---

## Module: Combulator

**Best musical fit:** The Combulator applies multiple tuned comb filters with feedback, producing resonant inharmonic overtones characteristic of physical resonators — bells, bar percussion, plucked metal, and blown pipes. Its three offset parameters generate the slight inharmonicity that distinguishes modal resonance from pure sine-based FM, making it particularly effective on existing FM and physical model presets.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| fm_bell.json | FM already provides bell inharmonicity; adding a combulator tuned to the same fundamental extends the metallic ring into a physically convincing resonator cluster. | `combulator_freq: 440.0`, `combulator_offset2: 7.5`, `combulator_offset3: 14.2`, `combulator_feedback: 0.35`, `combulator_tone: 0.6`, `combulator_mix: 0.3` |
| church_bell_modal.json | Modal synthesis captures the attack modes; a combulator with spaced offsets adds the slow-decaying minor-third partials missing from the modal material 7 model. | `combulator_freq: 220.0`, `combulator_offset2: 11.5`, `combulator_offset3: 19.0`, `combulator_feedback: 0.45`, `combulator_tone: 0.5`, `combulator_mix: 0.25` |
| vibraphone.json | The vibraphone already uses FM ratio 3.5; a combulator at low feedback adds bar-resonator ringing that FM alone cannot produce. | `combulator_freq: 440.0`, `combulator_offset2: 5.5`, `combulator_offset3: 10.8`, `combulator_feedback: 0.2`, `combulator_tone: 0.7`, `combulator_mix: 0.22` |
| marimba_modal.json | Marimba bars have a characteristic hollow second partial at ~4× the fundamental; adding a comb tuned to that offset enriches the attack transient. | `combulator_freq: 330.0`, `combulator_offset2: 3.9`, `combulator_offset3: 9.3`, `combulator_feedback: 0.15`, `combulator_tone: 0.8`, `combulator_mix: 0.2` |
| flute.json | The formant-based flute needs the bore resonance that a tightly-tuned comb filter provides — low feedback, low offset, adds the cylindrical tube character. | `combulator_freq: 440.0`, `combulator_offset2: 1.0`, `combulator_offset3: 2.0`, `combulator_feedback: 0.18`, `combulator_tone: 0.75`, `combulator_mix: 0.18` |

---

## Module: Treemonster

**Best musical fit:** Treemonster is a ring modulator that tracks the pitch of the input signal, locking ring modulation to the playing pitch rather than a fixed carrier — producing sidebands that remain harmonically related. This creates metallic or vocal timbres that track melody, making it ideal for electronic sounds that need a sci-fi robotic quality or for adding inharmonic upper partials to percussion without detuning at different pitches.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| hoover_lead.json | The hoover's detuned saws already create a harsh mid-range cluster; treemonster at a small shift ratio locks a metallic sideband to the root and enhances the characteristic "hoover snarl." | `treemonster_threshold: -24.0`, `treemonster_shift: 0.5`, `treemonster_ring_mix: 0.3`, `treemonster_mix: 0.35` |
| fm_bell.json | FM bell produces clean inharmonic partials; treemonster with a shift of ~0.33 below fundamental adds the characteristic sub-ring of a struck metal plate. | `treemonster_threshold: -18.0`, `treemonster_shift: 0.33`, `treemonster_ring_mix: 0.25`, `treemonster_mix: 0.25` |
| blade_runner.json | The slow LFO filter sweep already creates movement; layering a very low treemonster mix (no threshold gate) adds metallic beating that slowly evolves with the LFO. | `treemonster_threshold: -30.0`, `treemonster_shift: 0.25`, `treemonster_ring_mix: 0.15`, `treemonster_mix: 0.15` |
| screaming_lead.json | High-resonance square lead; treemonster at a 5th interval (shift 1.5) creates tracked parallel metallic sideband — the effect used in classic EMS VCS3 sounds. | `treemonster_threshold: -20.0`, `treemonster_shift: 1.5`, `treemonster_ring_mix: 0.2`, `treemonster_mix: 0.22` |

---

## Module: Nimbus

**Best musical fit:** Nimbus is a granular processor based on Mutable Instruments Clouds — it continuously granularizes the incoming audio into clouds of micro-grains, creating diffuse, slowly-evolving textural pads from almost any input. It excels on long-attack pads and drones, converting held synth notes into evolving atmospheric clouds, and it adds textural depth to ambient and cinematic sounds with no compositional effort required.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| ambient_pad.json | Long attack/release sine pad is the ideal candidate for granularization — converting the held note into a cloud of overlapping grains produces the signature Eno-style ambient texture. | `nimbus_position: 0.5`, `nimbus_size: 0.7`, `nimbus_pitch: 0.0`, `nimbus_density: 0.55`, `nimbus_spread: 0.4`, `nimbus_texture: 0.65`, `nimbus_mix: 0.35` |
| ethereal_pad.json | Already has chorus; adding Nimbus at low density and high texture creates a granular halo around the chorus that diffuses the sharp unison beating into ambient shimmer. | `nimbus_position: 0.5`, `nimbus_size: 0.8`, `nimbus_pitch: 0.0`, `nimbus_density: 0.35`, `nimbus_spread: 0.5`, `nimbus_texture: 0.8`, `nimbus_mix: 0.25` |
| dark_pad.json | Low-cutoff slow pad; Nimbus with a large size and low density stretches single grains into slow-swelling smears — essential for cinematic dark-ambient production. | `nimbus_position: 0.6`, `nimbus_size: 0.85`, `nimbus_pitch: 0.0`, `nimbus_density: 0.25`, `nimbus_spread: 0.3`, `nimbus_texture: 0.55`, `nimbus_mix: 0.3` |
| blade_runner.json | The Vangelis-style pad benefits from a granular cloud that de-synchronizes the unison voices into shifting grain constellations. | `nimbus_position: 0.5`, `nimbus_size: 0.75`, `nimbus_pitch: 0.0`, `nimbus_density: 0.4`, `nimbus_spread: 0.55`, `nimbus_texture: 0.7`, `nimbus_mix: 0.2` |
| shakuhachi.json | Short granular blurs on a sustained shakuhachi note produce the wavering, breath-cloud quality of a real instrument recorded in a reverberant space. | `nimbus_position: 0.45`, `nimbus_size: 0.4`, `nimbus_pitch: 0.0`, `nimbus_density: 0.6`, `nimbus_spread: 0.25`, `nimbus_texture: 0.5`, `nimbus_mix: 0.15` |

---

## Module: Vocoder

**Best musical fit:** The vocoder imposes the formant/spectral envelope of a modulator signal (typically speech or choir) onto a carrier (typically a synth), converting a synth tone into a speech-shaped or choir-shaped timbre. Within a self-contained synth (without external audio input), the modulator can be the synth's own noise or a second oscillator, making it most useful for robot-voice or talking-synth effects. In this context, the vocoder makes most sense on presets that already use formant synthesis or choir sources.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| male_choir.json | Already uses formant filter (type 3) with vowel tracking; adding a vocoder layer over a saw carrier broadens the formant structure into a 16-band spectral envelope, making the vowel transitions more convincing. | `vocoder_env_follow: 0.6`, `vocoder_gate: -32.0`, `vocoder_mix: 0.3` |
| female_choir.json | Same formant oscillator setup; the vocoder adds the spectral "breath attack" between vowels that formant filters alone cannot model. | `vocoder_env_follow: 0.7`, `vocoder_gate: -30.0`, `vocoder_mix: 0.28` |
| hoover_lead.json | The hoover's characteristic mid-range nasal quality is essentially a vowel formant effect; applying a low-gate vocoder smooths and shapes the formant peak into a vowel sweep. | `vocoder_env_follow: 0.5`, `vocoder_gate: -36.0`, `vocoder_mix: 0.2` |

---

## Module: ConvolutionReverb

**Best musical fit:** Convolution reverb captures the true impulse response of real acoustic spaces, producing a degree of realism unavailable from algorithmic reverbs. It is the correct tool for acoustic instruments (piano, strings, winds) that need to sound as though they are in a real room — a concert hall IR on a grand piano is indistinguishable from the original recording. It is overkill for synthetic pads where algorithmic reverb is preferred for its parameter control.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| grand_piano_v2.json | Already has reverb_mix: 0.12 with a basic algorithmic reverb; replacing with a convolution reverb using a Steinway concert hall IR dramatically raises realism. | `conv_reverb_room: 0.6` (medium hall), `conv_reverb_damping: 0.45`, `conv_reverb_predelay: 0.015`, `conv_reverb_mix: 0.18` |
| soft_piano.json | Soft piano already has reverb_damping: 0.7 suggesting an intimate space; a small room IR with high damping gives the intimate felt-muted piano-room character. | `conv_reverb_room: 0.3` (small room), `conv_reverb_damping: 0.65`, `conv_reverb_predelay: 0.008`, `conv_reverb_mix: 0.22` |
| violin.json | Physical model has correct body resonance; placing it in a chamber IR (reverb_room 0.4) completes the acoustic illusion of a solo violin in a recording session. | `conv_reverb_room: 0.4`, `conv_reverb_damping: 0.4`, `conv_reverb_predelay: 0.012`, `conv_reverb_mix: 0.2` |
| church_bell_modal.json | Church bell belongs in a cathedral; a long high-ceiling IR (large room, low damping) is the only convincing reverb for this sound — hall algorithm alone sounds wrong. | `conv_reverb_room: 0.9` (cathedral), `conv_reverb_damping: 0.1`, `conv_reverb_predelay: 0.04`, `conv_reverb_mix: 0.3` |
| shakuhachi.json | Already has algorithmic reverb; a Japanese wooden-room or stone-corridor IR would contextualize the instrument culturally and acoustically. | `conv_reverb_room: 0.45`, `conv_reverb_damping: 0.5`, `conv_reverb_predelay: 0.018`, `conv_reverb_mix: 0.22` |

---

## Module: GraphicEQ

**Best musical fit:** A graphic EQ is a corrective and creative tone-shaping tool. It is most useful on presets where the oscillator or physical model produces accurate timbre that is then colored wrongly by the synthesis chain — for instance, a filter sweep that leaves a mid hump, or a physical model with too much body resonance at a specific frequency. It is also the standard mastering-style tone control for getting string pads to sit in a mix.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| cs80_strings.json | 5-voice unison can build up a 500 Hz boxiness; cutting at geq_3 (500 Hz) and boosting geq_8 (4 kHz) gives the classic CS-80 warmth-with-air curve. | `geq_3: -2.5`, `geq_8: +2.0`, `graphic_eq_output: 0.0` |
| reese_bass.json | Low detuned saws accumulate sub-100 Hz energy that can muddy the mix; a high-pass shelf cut at geq_0 (32 Hz) and geq_1 (64 Hz) cleans it up. | `geq_0: -4.0`, `geq_1: -2.0`, `geq_5: +1.5`, `graphic_eq_output: 0.0` |
| male_choir.json | Formant filter with noise_level 0.06 can build harshness at 2–3 kHz; cutting geq_6 (2 kHz) and geq_7 (3 kHz) smooths vowel transitions. | `geq_6: -2.0`, `geq_7: -1.5`, `geq_9: +1.5`, `graphic_eq_output: 0.0` |
| wurlitzer_v2.json | Wurlitzer 200A has a characteristic mid-honk at 800 Hz and needs a slight presence boost at 5 kHz for the tine click; the graphic EQ can sculpt this precisely. | `geq_4: -1.8`, `geq_8: +1.5`, `graphic_eq_output: 0.0` |
| string_pad.json | Tripled detuned saws can accumulate 250–400 Hz mud; cutting geq_2 (125 Hz) and geq_3 (500 Hz) and boosting geq_9 (8 kHz) gives the classic string-pad air. | `geq_2: -1.5`, `geq_3: -2.0`, `geq_9: +2.5`, `graphic_eq_output: 0.0` |

---

## Module: Conditioner

**Best musical fit:** The Conditioner combines a bass cut (high-pass), stereo width control, and a limiter/threshold — it is a mastering-chain tool in miniature, most useful as the final stage on presets that are too wide in mono, too low-end-heavy, or that clip on transients. It is not a sound-design tool but a mix-glue and safety tool.

### Recommended additions to existing presets

| Preset file | Why | Parameters to add |
|-------------|-----|-------------------|
| cs80_strings.json | 5-voice unison at full spread can cause stereo phase issues at low frequencies; bass cut below 60 Hz and controlled width prevent mono collapse. | `conditioner_bass_cut: 60.0`, `conditioner_width: 1.1`, `conditioner_threshold: -3.0`, `conditioner_mix: 1.0` |
| ambient_pad.json | Long-release pad with 3 oscillators can clip during slow fade-ins if MsTool is also widening it; a soft limiter at -2 dBFS prevents output spikes. | `conditioner_bass_cut: 40.0`, `conditioner_width: 1.0`, `conditioner_threshold: -2.0`, `conditioner_mix: 1.0` |
| hoover_lead.json | Master volume 0.4 is very conservative, likely to avoid clipping from the 5-voice unison; a proper conditioner at the end of the chain allows master_volume to be raised to 0.6. | `conditioner_bass_cut: 80.0`, `conditioner_width: 1.0`, `conditioner_threshold: -1.5`, `conditioner_mix: 1.0` |
| reese_bass.json | Detuned bass with sub content can have unpredictable peak levels; a bass-cut at 30 Hz (sub rumble only) and a hard threshold protects downstream processing. | `conditioner_bass_cut: 30.0`, `conditioner_width: 0.85`, `conditioner_threshold: -2.0`, `conditioner_mix: 1.0` |

---

## New v2 Presets to Create

These are fully new presets that primarily showcase the new filters and effects. They should be created as new files (never overwrite existing presets).

### Filter showcases

| Suggested filename | Base concept | New filter/effects to use | Notes |
|-------------------|--------------|---------------------------|-------|
| `obxd_warm_strings_v2.json` | Warm 80s synth strings | `filter_type: 22` (OBXd4P), Reverb2, MsTool | OBXd4P has the Oberheim "creamy" 24 dB character; combine with unison 5 and slow attack for the classic OB-Xa string sound |
| `obxd_brass_v2.json` | Punchy synth brass stab | `filter_type: 22` (OBXd4P), WaveShaper, Exciter | OBXd4P on a detuned saw with fast attack and medium decay; waveshaper adds the pushed-filter distortion of analog brass |
| `tripole_wobble_bass.json` | Aggressive mono bass | `filter_type: 23` (Tripole), WaveShaper, Conditioner | Tripole is naturally suited to heavy mono bass; combine with high resonance and wavefold for industrial character |
| `cutwarp_acid_v2.json` | Smooth acid bassline | `filter_type: 25` (CutoffWarpLP), FloatyDelay | CutoffWarpLP has a warmer, more rounded resonance peak than the 303 filter; produces a smooth acid that works at lower resonance settings |
| `reswarp_lead_v2.json` | Silky resonant lead | `filter_type: 30` (ResWarpLP), Exciter, Reverb2 | ResWarp produces smooth resonance ideal for melodic leads; combine with exciter presence and hall reverb |
| `lofi_digital_pad.json` | Lo-fi digital artifact pad | `filter_type: 24` (SampleHold), Nimbus, GraphicEQ | Sample-and-hold filter introduces quantized stepping artifacts; granularize with Nimbus for a glitchy, drifting texture |
| `obxd_bright_lead.json` | Bright analog lead | `filter_type: 18` (OBXd2LP), WaveShaper, FloatyDelay | OBXd2LP gives a 2-pole bright Oberheim flavor; excellent for melody leads with moderate drive and tape delay |

### Effect showcases

| Suggested filename | Base concept | Primary new effects | Notes |
|-------------------|--------------|---------------------|-------|
| `granular_drone.json` | Granular ambient drone | Nimbus (high density, large size), Reverb2, MsTool | Start from a slow saw pad and use Nimbus as the primary sound source with high density and texture |
| `robot_choir.json` | Vocoder robot voice | Vocoder (high mix), male_choir base, GraphicEQ | Uses a saw carrier through the vocoder to produce the classic robot choir; pitch-locked sidebands track the played note |
| `metallic_bell_comb.json` | Metallic resonator bell | Combulator (wide offsets, moderate feedback), Reverb2 | An FM bell base with combulator replacing the standard reverb tail; the comb resonators extend the partial decay physically |
| `treemonster_ring_lead.json` | Ring-modulated sci-fi lead | Treemonster (shift 0.5 or 1.5), WaveShaper, Reverb2 | Clean sine or triangle oscillator processed by treemonster for pitch-tracked ring modulation sidebands |
| `tape_echo_keys.json` | Vintage tape echo electric piano | Rhodes base with FloatyDelay (high wobble), WaveShaper | FloatyDelay as primary effect emulating Roland RE-201 Space Echo on the Rhodes — a classic 1970s production sound |
| `convolution_grand.json` | Realistic concert grand | Grand piano physical model + ConvolutionReverb (hall IR) | Replaces the algorithmic reverb on grand_piano_v2 with a convolution hall — the same physical model sound with real acoustic space |

---

## Summary: Priority Ranking

Based on musical impact per preset, the highest-priority additions are:

1. **Reverb2** on `cs80_strings.json`, `male_choir.json`, `female_choir.json` — these presets have zero reverb and need it urgently for musical viability.
2. **WaveShaper** on `reese_bass.json`, `tb303_acid.json` — the most impactful single change per preset; bass and acid sounds are defined by their saturation character.
3. **Nimbus** on `ambient_pad.json`, `dark_pad.json` — granular processing transforms these from static pads to evolving textures with one module added.
4. **Exciter** on `violin.json`, `female_choir.json` — the single change that most closes the gap between physical/formant synthesis and acoustic realism.
5. **ConvolutionReverb** on `grand_piano_v2.json` — the only reverb upgrade that genuinely changes the perceived quality of an acoustic instrument preset.
6. **FloatyDelay** on `dx7_epiano.json`, `shakuhachi.json` — these instruments are historically associated with tape echo and the effect is immediately recognizable.
7. **MsTool** on `cs80_strings.json`, `blade_runner.json` — high-unison pads most directly benefit from stereo field control.
8. **Combulator** on `church_bell_modal.json`, `fm_bell.json` — adds physical resonator character to percussive sounds that already have the right attack structure.
9. **GraphicEQ** on `cs80_strings.json`, `reese_bass.json` — corrective shaping that improves mix-readiness without altering fundamental character.
10. **Conditioner** on `hoover_lead.json`, `cs80_strings.json` — mastering-chain safety; lower creative priority but important for loudness headroom.
11. **Treemonster** on `hoover_lead.json`, `screaming_lead.json` — high-impact on specific sounds but narrower applicability.
12. **Vocoder** on `male_choir.json`, `female_choir.json` — genuinely useful but requires careful integration to avoid sounding gimmicky.
