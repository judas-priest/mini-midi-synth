#!/usr/bin/env python3
"""
Surge XT .fxp → mini_midi_synth JSON preset converter.
Uses exact parameter mappings from Surge XT source code.

Run: python3 tools/surge_converter.py
"""

import os, json, math
import xml.etree.ElementTree as ET
from pathlib import Path
from collections import defaultdict

SURGE_PRESETS = Path("/usr/share/surge-xt/patches_factory")
WT_DIR = Path.home() / ".local/share/mini_midi_synth/wavetables"
OUT_DIR = Path.home() / ".local/share/mini_midi_synth/presets"

# ── Oscillator type IDs (from SurgeStorage.h) ────────────────────────────────
# ot_classic=0, ot_sine=1, ot_wavetable=2, ot_shnoise=3, ot_audioinput=4,
# ot_FM3=5, ot_FM2=6, ot_window=7, ot_modern=8, ot_string=9,
# ot_twist=10, ot_alias=11
OSC_MAP = {
    0: 1,   # Classic → Saw (default, shape adjusts below)
    1: 0,   # Sine
    2: 27,  # Wavetable
    3: 5,   # S&H Noise → Noise
    4: 1,   # Audio Input → Saw (not supported)
    5: 28,  # FM3
    6: 4,   # FM2 → FM
    7: 27,  # Window → Wavetable
    8: 1,   # Modern → Saw
    9: 6,   # String → Karplus-Strong
    10: 29, # Twist / Plaits
    11: 25, # Alias
}

def surge_osc(surge_type, param0):
    """Map Surge osc type to ours. Classic: param0 = -1=Saw, 0=Square, 1=Tri"""
    t = int(surge_type)
    if t == 0:  # Classic oscillator
        p = float(param0)
        if p < -0.33:   return 1   # Saw
        elif p > 0.33:  return 3   # Triangle
        else:           return 2   # Square
    return OSC_MAP.get(t, 1)

# ── Filter type IDs (from SurgeStorage.h fut_* enum) ─────────────────────────
# Our filter types: 0=LP SVF, 1=HP SVF, 2=BP SVF, 3=Formant,
# 4=MoogLP24, 5=MoogLP12, 6=DiodeLP, 7=Comb, 8=Allpass
FILTER_MAP = {
    # LP family (0-3): fut_lp12, fut_lp24, fut_bp12_A, fut_lpmoog
    0: 0, 1: 0, 2: 2, 3: 4,
    # HP family (4-5)
    4: 1, 5: 1,
    # BP (6)
    6: 2,
    # Notch (7)
    7: 8,
    # Comb (8-9)
    8: 7, 9: 7,
    # S&H (9) → allpass approx
    10: 4,  # fut_vintageladder → Moog
    11: 5,  # → LP12
    12: 4,  # OB-Xd 4-pole → Moog
    13: 4,  # K35 LP
    14: 1,  # K35 HP
    15: 6,  # Diode
    # Others → LP default
}
def surge_filter(surge_type):
    return FILTER_MAP.get(int(surge_type), 0)

# ── Filter cutoff: Hz = MIDI_0_FREQ * 2^(semitones/12) ───────────────────────
# MIDI_0_FREQ = 8.1757994155... Hz (C-1)
MIDI_0_FREQ = 8.1757994155

def surge_cutoff_hz(semitones):
    # In Surge, filter cutoff 0 = A4 (440 Hz), stored as semitones offset
    hz = 440.0 * (2.0 ** (float(semitones) / 12.0))
    return max(20.0, min(20000.0, hz))

# ── Envelope time: t = 2^param seconds ───────────────────────────────────────
# param -8 ≈ 0.004s (instant), 0 = 1s, 4 = 16s
def surge_env_time(val):
    t = 2.0 ** float(val)
    return max(0.001, min(30.0, t))

# ── LFO rate: 2^val Hz ───────────────────────────────────────────────────────
def surge_lfo_rate(val):
    return max(0.01, min(30.0, 2.0 ** float(val)))

# ── LFO shape (lt_* enum) ────────────────────────────────────────────────────
# lt_sine=0, lt_tri=1, lt_square=2, lt_ramp=3, lt_noise=4, lt_snh=5,
# lt_envelope=6, lt_stepseq=7, lt_mseg=8, lt_formula=9
# Our: 0=Sine, 1=Triangle, 2=Square, 3=S&H, 4=Sawtooth, 5=Envelope, 6=Noise
LFO_SHAPE_MAP = {0:0, 1:1, 2:2, 3:4, 4:6, 5:3, 6:5, 7:0, 8:0, 9:0}
def surge_lfo_shape(val): return LFO_SHAPE_MAP.get(int(val), 0)

# ── FX type IDs (from SurgeStorage.h fxt_* enum) ─────────────────────────────
# fxt_off=0, fxt_delay=1, fxt_reverb=2, fxt_phaser=3, fxt_rotaryspeaker=4,
# fxt_distortion=5, fxt_eq=6, fxt_freqshift=7, fxt_conditioner=8,
# fxt_chorus4=9, fxt_vocoder=10, fxt_reverb2=11, fxt_flanger=12,
# fxt_ringmod=13, fxt_airwindows=14, fxt_neuron=15, fxt_geq11=16,
# fxt_resonator=17, fxt_chow=18, fxt_exciter=19, fxt_ensemble=20,
# fxt_combulator=21, fxt_nimbus=22, fxt_tape=23, fxt_treemonster=24,
# fxt_waveshaper=25, fxt_mstool=26, fxt_spring_reverb=27, fxt_bonsai=28,
# fxt_floaty_delay=30, fxt_convolution=31

# Modulation sources (from ModulationSource.h ms_* enum)
# ms_lfo1=17..ms_lfo6=22, ms_slfo1=23..ms_slfo6=28
# ms_ampeg=15, ms_filtereg=16
# ms_velocity=1, ms_keytrack=2, ms_aftertouch=4, ms_pitchbend=5, ms_modwheel=6

MOD_SRC_MAP = {
    1: 'velocity', 2: 'keytrack', 4: 'aftertouch', 5: 'pitchbend',
    6: 'modwheel',
    15: 'amp_env', 16: 'filter_env',
    17: 'lfo1', 18: 'lfo2', 19: 'lfo3', 20: 'lfo4',
    23: 'slfo1', 24: 'slfo2',
}

def gv(params_dict, key, default=0.0):
    el = params_dict.get(key)
    if el is None: return default
    try: return float(el.get('value', default))
    except: return float(default)

def get_mods(params_dict, key):
    """Get modrouting child elements of a param."""
    el = params_dict.get(key)
    if el is None: return []
    return [{'src': int(m.get('source', 0)), 'depth': float(m.get('depth', 0))}
            for m in el.findall('modrouting')]

def find_wt(name):
    if not name: return None
    name_l = name.lower().strip()
    for f in WT_DIR.rglob("*.wt"):
        if f.stem.lower() == name_l:
            return str(f.relative_to(WT_DIR))
    return None

def convert_fxp(fxp_path, category):
    try:
        data = fxp_path.read_bytes()
    except: return None

    xml_start = data.find(b'<?xml')
    if xml_start < 0: return None

    xml_bytes = data[xml_start:]
    end_tag = b'</patch>'
    end_pos = xml_bytes.rfind(end_tag)
    if end_pos >= 0:
        xml_bytes = xml_bytes[:end_pos + len(end_tag)]

    try:
        root = ET.fromstring(xml_bytes.decode('utf-8', errors='replace'))
    except ET.ParseError:
        return None

    meta = root.find('meta')
    name = (meta.get('name') if meta is not None else None) or fxp_path.stem

    params_el = root.find('parameters')
    if params_el is None: return None
    P = {el.tag: el for el in params_el}

    # ── Scene A oscillator 1 ─────────────────────────────────────────────
    osc1_type = int(gv(P, 'a_osc1_type'))
    osc1_p0   = gv(P, 'a_osc1_param0')
    osc1_oct  = int(gv(P, 'a_osc1_octave'))  # -2..+2 octave shift
    osc1_pitch= gv(P, 'a_osc1_pitch')        # fine tune cents
    our_osc   = surge_osc(osc1_type, osc1_p0)

    # Pulse width (param1 of Classic, 0.5 = 50%)
    pulse_width = gv(P, 'a_osc1_param1', 0.5) if osc1_type == 0 else 0.5
    pulse_width = max(0.05, min(0.95, pulse_width))

    # ── Osc 2 ────────────────────────────────────────────────────────────
    # a_level_o* is linear 0..1, a_mute_o* = 1 means muted
    osc2_type  = int(gv(P, 'a_osc2_type'))
    osc2_p0    = gv(P, 'a_osc2_param0')
    osc2_muted = gv(P, 'a_mute_o2') > 0.5
    osc2_level = 0.0 if osc2_muted else max(0.0, min(1.0, gv(P, 'a_level_o2', 1.0)))
    our_osc2 = surge_osc(osc2_type, osc2_p0)

    # ── Osc 3 ────────────────────────────────────────────────────────────
    osc3_type  = int(gv(P, 'a_osc3_type'))
    osc3_p0    = gv(P, 'a_osc3_param0')
    osc3_muted = gv(P, 'a_mute_o3') > 0.5
    osc3_level = 0.0 if osc3_muted else max(0.0, min(1.0, gv(P, 'a_level_o3', 1.0)))
    our_osc3   = surge_osc(osc3_type, osc3_p0)

    # ── Osc 1 level ──────────────────────────────────────────────────────
    o1_muted = gv(P, 'a_mute_o1') > 0.5
    o1_level = 0.0 if o1_muted else max(0.0, min(1.0, gv(P, 'a_level_o1', 1.0)))

    # ── Wavetable reference ───────────────────────────────────────────────
    wt_file = None
    if osc1_type == 2:  # Wavetable osc
        wt_el = P.get('a_osc1_wavetable_name')
        wt_name = wt_el.get('value', '') if wt_el is not None else ''
        if wt_name:
            wt_file = find_wt(wt_name)

    # ── Filters ───────────────────────────────────────────────────────────
    f1_type  = int(gv(P, 'a_filter1_type'))
    f1_cut   = surge_cutoff_hz(gv(P, 'a_filter1_cutoff'))
    f1_res   = max(0.0, min(0.99, gv(P, 'a_filter1_resonance')))
    f1_env   = gv(P, 'a_filter1_envmod')   # semitones
    f1_kt    = gv(P, 'a_filter1_keytrack') # 0..1
    our_ft   = surge_filter(f1_type)

    # Filter env mod: f1_env is in semitones added to cutoff
    # delta_hz = cutoff_at_0 * (2^(N/12) - 1) where cutoff_at_0 = 440 Hz (A4)
    # This gives the Hz offset from the base 440Hz reference point
    env_hz = 440.0 * (2.0**(f1_env/12.0) - 1.0) if abs(f1_env) > 0.1 else 0.0
    env_hz = max(-20000.0, min(20000.0, env_hz))

    # Filter 2
    f2_type = int(gv(P, 'a_filter2_type'))
    f2_cut  = surge_cutoff_hz(gv(P, 'a_filter2_cutoff'))
    f2_res  = max(0.0, min(0.99, gv(P, 'a_filter2_resonance')))
    our_ft2 = surge_filter(f2_type)

    # Filter routing (0=series, 1=parallel, 2-5=other)
    filter_route_surge = int(gv(P, 'a_filter_balance'))
    # 0=Serial, 1=Parallel in Surge maps to our 1=Serial, 2=Parallel
    filter_route = 0  # single by default
    if f2_type > 0 and not gv(P, 'a_filter2_type') == 0:
        filter_route = 1  # serial

    # ── AEG (env1 = amplitude) ────────────────────────────────────────────
    aeg_a  = surge_env_time(gv(P, 'a_env1_attack',  -8))
    aeg_d  = surge_env_time(gv(P, 'a_env1_decay',    0))
    aeg_s  = max(0.0, min(1.0, gv(P, 'a_env1_sustain', 1.0)))
    aeg_r  = surge_env_time(gv(P, 'a_env1_release', -4))
    aeg_as = int(gv(P, 'a_env1_attack_shape',  0))  # 0=sqrt,1=linear,2=quad
    aeg_ds = int(gv(P, 'a_env1_decay_shape',   0))
    aeg_rs = int(gv(P, 'a_env1_release_shape', 0))

    # ── FEG (env2 = filter) ───────────────────────────────────────────────
    feg_a  = surge_env_time(gv(P, 'a_env2_attack',  -8))
    feg_d  = surge_env_time(gv(P, 'a_env2_decay',    0))
    feg_s  = max(0.0, min(1.0, gv(P, 'a_env2_sustain', 0.0)))
    feg_r  = surge_env_time(gv(P, 'a_env2_release', -4))
    feg_as = int(gv(P, 'a_env2_attack_shape', 0))
    feg_ds = int(gv(P, 'a_env2_decay_shape',  0))
    feg_rs = int(gv(P, 'a_env2_release_shape',0))

    # ── LFO 0 (voice LFO 1) ──────────────────────────────────────────────
    lfo0_sh = surge_lfo_shape(gv(P, 'a_lfo0_shape'))
    lfo0_rt = surge_lfo_rate(gv(P, 'a_lfo0_rate'))
    lfo0_mg = gv(P, 'a_lfo0_magnitude', 0.0)
    lfo0_df = gv(P, 'a_lfo0_deform', 0.0)
    lfo0_uni= gv(P, 'a_lfo0_unipolar', 0.0) > 0.5
    lfo0_trg= int(gv(P, 'a_lfo0_trigmode', 0))  # 0=free,1=keytrigger

    # LFO 1 (voice LFO 2)
    lfo1_sh = surge_lfo_shape(gv(P, 'a_lfo1_shape'))
    lfo1_rt = surge_lfo_rate(gv(P, 'a_lfo1_rate'))
    lfo1_mg = gv(P, 'a_lfo1_magnitude', 0.0)
    lfo1_df = gv(P, 'a_lfo1_deform', 0.0)
    lfo1_uni= gv(P, 'a_lfo1_unipolar', 0.0) > 0.5
    lfo1_trg= int(gv(P, 'a_lfo1_trigmode', 0))

    # ── Modulation routings ───────────────────────────────────────────────
    # Check pitch modulation (from LFO)
    lfo_pitch_depth = 0.0
    lfo_filter_depth = 0.0
    lfo_amp_depth = 0.0

    for dest_key, depth_factor in [
        ('a_pitch', 1.0/48.0),          # pitch: semitones normalized
        ('a_osc1_pitch', 1.0/48.0),
    ]:
        for mod in get_mods(P, dest_key):
            src = mod['src']
            if src in (17, 18):  # LFO1, LFO2
                lfo_pitch_depth = max(lfo_pitch_depth, abs(mod['depth']) * depth_factor)

    for mod in get_mods(P, 'a_filter1_cutoff'):
        src = mod['src']
        if src in (17, 18):
            # depth is in semitones, normalize to 0..1
            lfo_filter_depth = max(lfo_filter_depth, abs(mod['depth']) / 60.0)

    for mod in get_mods(P, 'a_amp_gain'):
        src = mod['src']
        if src in (17, 18):
            lfo_amp_depth = max(lfo_amp_depth, abs(mod['depth']))

    # LFO trigger mode → our lfo1_trigger_mode
    # Surge: 0=free, 1=keytrigger, 2=random
    lfo0_trigger = min(3, max(0, lfo0_trg))
    lfo1_trigger = min(3, max(0, lfo1_trg))

    # ── FX slots ──────────────────────────────────────────────────────────
    reverb_mix  = 0.0; reverb_room = 0.5; reverb_damp = 0.5
    reverb2_mix = 0.0
    chorus_mix  = 0.0
    delay_mix   = 0.0; delay_tl = 0.3; delay_tr = 0.4; delay_fb = 0.4
    phaser_mix  = 0.0
    rotary_mix  = 0.0
    flanger_mix = 0.0
    tape_mix    = 0.0; tape_drive = 0.0
    distort_mix = 0.0
    spring_mix  = 0.0
    ensemble_mix= 0.0

    fx_disable = int(gv(P, 'fx_disable', 0))
    fx_bypass  = int(gv(P, 'fx_bypass', 0))

    for slot in range(1, 9):
        bit = 1 << (slot - 1)
        if (fx_disable & bit) or (fx_bypass & bit): continue
        ftype = int(gv(P, f'fx{slot}_type', 0))
        p = [gv(P, f'fx{slot}_p{i}', 0.0) for i in range(12)]

        if ftype == 1:    # Delay
            delay_mix = max(delay_mix, 0.35)
            delay_tl = max(0.01, min(2.0, abs(p[0]) if p[0] > 0 else 0.3))
            delay_tr = max(0.01, min(2.0, abs(p[1]) if p[1] > 0 else 0.4))
            delay_fb = max(0.0, min(0.95, p[2] / 2.0 + 0.5 if abs(p[2]) < 2 else 0.4))
        elif ftype == 2:  # Reverb 1
            reverb_mix  = max(reverb_mix, 0.35)
            reverb_room = max(0.0, min(1.0, p[0] / 10.0 + 0.5))
            reverb_damp = max(0.0, min(1.0, p[1] / 10.0 + 0.5))
        elif ftype == 3:  # Phaser
            phaser_mix = max(phaser_mix, 0.3)
        elif ftype == 4:  # Rotary Speaker
            rotary_mix = max(rotary_mix, 0.4)
        elif ftype == 5:  # Distortion → overdrive
            distort_mix = max(distort_mix, 0.4)
        elif ftype == 9:  # Chorus4
            chorus_mix = max(chorus_mix, 0.3)
        elif ftype == 11: # Reverb2
            reverb2_mix = max(reverb2_mix, 0.35)
        elif ftype == 12: # Flanger
            flanger_mix = max(flanger_mix, 0.3)
        elif ftype == 20: # Ensemble
            ensemble_mix = max(ensemble_mix, 0.35)
        elif ftype == 23: # Tape
            tape_mix = max(tape_mix, 0.4)
            tape_drive = max(0.0, min(1.0, p[0] / 10.0 + 0.5))
        elif ftype == 27: # Spring Reverb
            spring_mix = max(spring_mix, 0.35)

    # Pick dominant reverb
    use_reverb  = max(reverb_mix, spring_mix)
    use_reverb2 = reverb2_mix

    # ── Output params ─────────────────────────────────────────────────────
    # Master volume: Surge volume param is in dB (-144 to +12)
    vol_db = gv(P, 'volume', 0.0)
    master_vol = round(min(1.0, 10**(vol_db/20.0)) if vol_db > -96 else 0.75, 2)
    master_vol = max(0.5, min(1.0, master_vol))

    # Octave shift → transpose
    transpose = osc1_oct * 12

    params = {
        "osc_type": float(our_osc),
        "osc_detune": round(osc1_pitch / 1200.0, 4),  # cents → small float
        "pulse_width": round(pulse_width, 3),
        "filter_type": float(our_ft),
        "filter_cutoff": round(f1_cut, 1),
        "filter_resonance": round(f1_res, 3),
        "filter_env_amount": round(env_hz, 1),
        "filter_key_track": round(f1_kt, 3),
        "amp_attack":  round(aeg_a, 4),
        "amp_decay":   round(aeg_d, 4),
        "amp_sustain": round(aeg_s, 3),
        "amp_release": round(aeg_r, 4),
        "env_attack_shape":  float(aeg_as),
        "env_decay_shape":   float(aeg_ds),
        "env_release_shape": float(aeg_rs),
        "filter_attack":  round(feg_a, 4),
        "filter_decay":   round(feg_d, 4),
        "filter_sustain": round(feg_s, 3),
        "filter_release": round(feg_r, 4),
        "filter_env_attack_shape":  float(feg_as),
        "filter_env_decay_shape":   float(feg_ds),
        "filter_env_release_shape": float(feg_rs),
        "lfo_waveform": float(lfo0_sh),
        "lfo_rate": round(lfo0_rt, 3),
        "lfo_deform": round(lfo0_df, 3),
        "lfo_unipolar": 1.0 if lfo0_uni else 0.0,
        "lfo1_trigger_mode": float(lfo0_trigger),
        "lfo_pitch_depth": round(lfo_pitch_depth, 3),
        "lfo_filter_depth": round(lfo_filter_depth, 3),
        "lfo_amp_depth": round(lfo_amp_depth, 3),
        "lfo2_waveform": float(lfo1_sh),
        "lfo2_rate": round(lfo1_rt, 3),
        "lfo2_deform": round(lfo1_df, 3),
        "lfo2_unipolar": 1.0 if lfo1_uni else 0.0,
        "lfo2_trigger_mode": float(lfo1_trigger),
        "reverb_mix":  round(use_reverb, 2),
        "reverb_room_size": round(reverb_room, 2),
        "reverb_damping": round(reverb_damp, 2),
        "reverb2_mix": round(use_reverb2, 2),
        "chorus_mix":  round(chorus_mix, 2),
        "delay_mix":   round(delay_mix, 2),
        "delay_time_l": round(delay_tl, 3),
        "delay_time_r": round(delay_tr, 3),
        "delay_feedback": round(delay_fb, 3),
        "phaser_mix":  round(phaser_mix, 2),
        "rotary_mix":  round(rotary_mix, 2),
        "ensemble_mix": round(ensemble_mix, 2),
        "tape_mix":    round(tape_mix, 2),
        "tape_drive":  round(tape_drive, 2),
        "master_volume": master_vol,
    }

    # Filter 2 if active
    if f2_type > 0:
        params["filter2_type"] = float(our_ft2)
        params["filter2_cutoff"] = round(f2_cut, 1)
        params["filter2_resonance"] = round(f2_res, 3)
        params["filter_routing"] = 1.0  # serial

    # Osc 2 if audible
    if osc2_level > 0.05:
        params["osc_count"] = 2.0
        params["osc1_level"] = round(o1_level if o1_level > 0.01 else 1.0, 2)
        params["osc2_type"] = float(our_osc2)
        params["osc2_level"] = round(osc2_level, 2)

    # Osc 3 if audible
    if osc3_level > 0.05:
        params["osc_count"] = 3.0
        params["osc3_type"] = float(our_osc3)
        params["osc3_level"] = round(osc3_level, 2)

    out = {"name": name, "category": category, "params": params}
    if wt_file:
        out["wavetable_file"] = wt_file
    return out


# ── Category map ─────────────────────────────────────────────────────────────
CAT_MAP = {
    "Basses": "Surge Bass", "Leads": "Surge Lead", "Pads": "Surge Pad",
    "Plucks": "Surge Pluck", "Polysynths": "Surge Pad", "Sequences": "Surge Seq",
    "Keys": "Surge Keys", "Brass": "Surge Brass", "FX": "Surge FX",
    "Chords": "Surge Pad", "Winds": "Surge Lead", "Percussion": "Surge Perc",
    "MPE": "Surge MPE", "Splits": "Surge Splits", "Templates": "Surge Templates",
    "Vocoder": "Surge FX", "Tutorials": "Surge Templates",
}

def convert_all():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    converted = failed = 0

    for cat_dir in sorted(SURGE_PRESETS.iterdir()):
        if not cat_dir.is_dir(): continue
        category = CAT_MAP.get(cat_dir.name, f"Surge {cat_dir.name}")
        subdir = OUT_DIR / category.replace(" ", "_")
        subdir.mkdir(exist_ok=True)

        for fxp in sorted(cat_dir.glob("*.fxp")):
            result = convert_fxp(fxp, category)
            if result is None:
                failed += 1
                continue
            safe = "".join(c if c.isalnum() or c in " _-()" else "_" for c in result["name"])
            with open(subdir / f"{safe}.json", 'w') as f:
                json.dump(result, f, indent=2)
            converted += 1

    print(f"✓ Converted: {converted}")
    print(f"✗ Failed:    {failed}")
    print(f"Output: {OUT_DIR}")

if __name__ == "__main__":
    # Clear old output
    import shutil
    for d in OUT_DIR.glob("Surge_*"):
        shutil.rmtree(d, ignore_errors=True)
    for d in OUT_DIR.glob("Surge *"):
        shutil.rmtree(d, ignore_errors=True)
    convert_all()
