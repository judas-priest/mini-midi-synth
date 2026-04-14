#!/usr/bin/env python3
"""
Surge XT .fxp → mini_midi_synth JSON preset converter.
Converts all factory presets from /usr/share/surge-xt/patches_factory/
Output goes to ~/.local/share/mini_midi_synth/presets/ (category subfolders).

Usage: python3 surge_converter.py
"""

import os
import sys
import json
import math
import xml.etree.ElementTree as ET
from pathlib import Path

# ── Paths ────────────────────────────────────────────────────────────────────

SURGE_PRESETS = Path("/usr/share/surge-xt/patches_factory")
SURGE_WT = Path("/usr/share/surge-xt/wavetables")
OUT_DIR = Path.home() / ".local/share/mini_midi_synth/presets"
WT_DIR = Path.home() / ".local/share/mini_midi_synth/wavetables"

# ── Oscillator type mapping ───────────────────────────────────────────────────
# Surge osc types:
# 0=Classic(saw/square/tri), 1=Sine, 2=Wavetable, 3=Window, 4=SH Noise,
# 5=FM2, 6=FM3, 7=String, 8=Alias, 9=Twist, 10=Modern, 11=Audio Input
#
# Our osc types:
# 0=Sine, 1=Saw, 2=Square, 3=Triangle, 4=FM, 5=Noise, 6=KS,
# 27=Wavetable, 28=FM3, 29=Twist

def surge_osc_to_ours(surge_type: int, param0: float) -> int:
    """Map Surge osc type to our osc_type. param0 affects Classic shape."""
    surge_type = int(surge_type)
    if surge_type == 0:   # Classic: param0 controls shape (-1=saw, 0=square ish, 1=tri-ish)
        if param0 < -0.5:
            return 1  # Saw
        elif param0 > 0.5:
            return 3  # Triangle
        else:
            return 2  # Square
    elif surge_type == 1:  return 0   # Sine
    elif surge_type == 2:  return 27  # Wavetable
    elif surge_type == 3:  return 27  # Window → Wavetable approx
    elif surge_type == 4:  return 5   # SH Noise → Noise
    elif surge_type == 5:  return 4   # FM2 → FM
    elif surge_type == 6:  return 28  # FM3
    elif surge_type == 7:  return 6   # String → Karplus-Strong
    elif surge_type == 8:  return 25  # Alias
    elif surge_type == 9:  return 29  # Twist / Plaits
    elif surge_type == 10: return 1   # Modern → Saw (closest)
    else:                  return 1   # Default to Saw

def surge_filter_type_to_ours(surge_type: int) -> int:
    """Map Surge filter type to our filter_type.
    Surge: 0=LP, 1=LP2B, 2=HP, 3=HP2B, 4=BP, 5=Notch, 6=Multi, 7=Comb+, ...
    Ours:  0=LowPass, 1=HighPass, 2=BandPass, 3=Formant, 4=MoogLP24, 5=MoogLP12, ...
    """
    t = int(surge_type)
    # Surge filter type families (each has 4 subtypes)
    family = t // 4
    # 0=LP, 1=HP, 2=BP, 3=Notch, 4=Multi, 5=LadderLP, 6=LadderHP, 7=Comb
    if family == 0:   return 0   # LP → our LowPass SVF
    elif family == 1: return 1   # HP → our HighPass SVF
    elif family == 2: return 2   # BP → our BandPass SVF
    elif family == 3: return 8   # Notch → our Allpass/Notch (index 8)
    elif family == 5: return 4   # Ladder LP → our MoogLP24
    elif family == 6: return 5   # Ladder HP → our MoogLP12 approx
    elif family == 7: return 7   # Comb
    else:             return 0   # Default LP

def surge_env_time(log_val: float) -> float:
    """Convert Surge log-scale envelope time to seconds.
    Surge uses: -8 ≈ 0.001s (instant), 0 = 1s, positive = longer
    Formula: t = 2^val seconds, clamped
    """
    if log_val <= -8:
        return 0.001
    t = 2.0 ** log_val
    return max(0.001, min(t, 30.0))

def surge_cutoff_to_hz(semitones: float) -> float:
    """Convert Surge filter cutoff (semitones from C4 = 261.63 Hz) to Hz."""
    # Surge cutoff: 0 = C4 (261.63 Hz), each unit = 1 semitone
    hz = 261.63 * (2.0 ** (semitones / 12.0))
    return max(20.0, min(hz, 20000.0))

def surge_lfo_rate_to_hz(val: float) -> float:
    """Convert Surge LFO rate to Hz. Surge uses 2^val * base."""
    # Surge LFO rate: 0 = ~1 Hz, each unit doubles
    return max(0.01, min(2.0 ** val, 30.0))

def surge_lfo_shape(val: int) -> int:
    """Map Surge LFO shape to our lfo_waveform.
    Surge: 0=Sine, 1=Tri, 2=Square, 3=Ramp(saw down), 4=Noise, 5=S&H, 6=Envelope
    Ours:  0=Sine, 1=Triangle, 2=Square, 3=S&H, 4=Sawtooth, 5=Envelope, 6=Noise
    """
    m = {0: 0, 1: 1, 2: 2, 3: 4, 4: 6, 5: 3, 6: 5}
    return m.get(int(val), 0)

def get_param(params: dict, key: str, default=0.0):
    el = params.get(key)
    if el is None:
        return default
    try:
        return float(el.get('value', default))
    except:
        return default

def get_modrouting(el) -> list:
    """Extract modulation routings from a param element."""
    routes = []
    if el is None:
        return routes
    for mod in el.findall('modrouting'):
        try:
            routes.append({
                'source': int(mod.get('source', 0)),
                'depth': float(mod.get('depth', 0)),
            })
        except:
            pass
    return routes

# Surge modrouting source IDs
SURGE_MOD_SOURCES = {
    1: None,   # LFO A (scene A lfo1)
    2: None,   # LFO B
    4: None,   # AEG (amp env)
    5: None,   # FEG (filter env)
    6: None,   # keytrack
    7: None,   # velocity
    11: None,  # pitchbend
    12: None,  # modwheel
}

def convert_fxp(fxp_path: Path, category: str) -> dict | None:
    """Convert a single .fxp file to our preset dict. Returns None on failure."""
    try:
        data = fxp_path.read_bytes()
    except:
        return None

    xml_start = data.find(b'<?xml')
    if xml_start < 0:
        return None

    try:
        xml_bytes = data[xml_start:]
        # Truncate at </patch> to remove trailing binary data
        end_tag = b'</patch>'
        end_pos = xml_bytes.rfind(end_tag)
        if end_pos >= 0:
            xml_bytes = xml_bytes[:end_pos + len(end_tag)]
        xml_str = xml_bytes.decode('utf-8', errors='replace')
        root = ET.fromstring(xml_str)
    except ET.ParseError:
        try:
            xml_clean = xml_str.rstrip('\x00').rstrip()
            root = ET.fromstring(xml_clean)
        except:
            return None

    meta = root.find('meta')
    name = meta.get('name', fxp_path.stem) if meta is not None else fxp_path.stem

    params_el = root.find('parameters')
    if params_el is None:
        return None

    # Build lookup: tag → element
    P = {el.tag: el for el in params_el}

    def gp(key, default=0.0):
        return get_param(P, key, default)

    # ── Scene A oscillator 1 ──────────────────────────────────────────────
    osc1_type = int(gp('a_osc1_type'))
    osc1_p0   = gp('a_osc1_param0')
    osc1_oct  = int(gp('a_osc1_octave'))
    our_osc   = surge_osc_to_ours(osc1_type, osc1_p0)
    detune    = gp('a_osc1_pitch') / 100.0  # cents → 0..1 approx

    # Osc 2
    osc2_type = int(gp('a_osc2_type'))
    osc2_p0   = gp('a_osc2_param0')
    osc2_oct  = int(gp('a_osc2_octave'))
    osc2_level_el = P.get('a_level_o2')
    osc2_mute = gp('a_mute_o2') > 0.5
    osc2_level = 0.0 if osc2_mute else max(0.0, min(1.0, gp('a_level_o2') * 0.5 + 0.5))
    our_osc2  = surge_osc_to_ours(osc2_type, osc2_p0)

    # ── Filters ───────────────────────────────────────────────────────────
    f1_type = int(gp('a_filter1_type'))
    f1_cutoff_semi = gp('a_filter1_cutoff')
    f1_reso   = gp('a_filter1_resonance')
    f1_envmod = gp('a_filter1_envmod')
    f1_ktrack = gp('a_filter1_keytrack')
    our_ftype = surge_filter_type_to_ours(f1_type)
    our_fcut  = surge_cutoff_to_hz(f1_cutoff_semi)

    # ── AEG (amp envelope = env1) ─────────────────────────────────────────
    aeg_a = surge_env_time(gp('a_env1_attack', -8))
    aeg_d = surge_env_time(gp('a_env1_decay',  0))
    aeg_s = max(0.0, min(1.0, gp('a_env1_sustain', 1.0)))
    aeg_r = surge_env_time(gp('a_env1_release', -4))
    aeg_as = int(gp('a_env1_attack_shape', 0))
    aeg_ds = int(gp('a_env1_decay_shape', 0))
    aeg_rs = int(gp('a_env1_release_shape', 0))

    # ── FEG (filter envelope = env2) ─────────────────────────────────────
    feg_a = surge_env_time(gp('a_env2_attack', -8))
    feg_d = surge_env_time(gp('a_env2_decay',  0))
    feg_s = max(0.0, min(1.0, gp('a_env2_sustain', 0.0)))
    feg_r = surge_env_time(gp('a_env2_release', -4))

    # Filter env amount (semitones * hz_scale → Hz offset)
    # f1_envmod is in semitones, convert to our Hz-based env amount
    env_hz = f1_envmod * 40.0  # rough: each semitone ≈ 40 Hz at midrange

    # ── LFO 0 (voice LFO, often pitch/filter vibrato) ─────────────────────
    lfo0_shape = surge_lfo_shape(int(gp('a_lfo0_shape')))
    lfo0_rate  = surge_lfo_rate_to_hz(gp('a_lfo0_rate'))
    lfo0_mag   = gp('a_lfo0_magnitude')
    lfo0_deform= gp('a_lfo0_deform')
    lfo0_uni   = gp('a_lfo0_unipolar') > 0.5

    # LFO 1 (often scene/chorus LFO)
    lfo1_shape = surge_lfo_shape(int(gp('a_lfo1_shape')))
    lfo1_rate  = surge_lfo_rate_to_hz(gp('a_lfo1_rate'))
    lfo1_mag   = gp('a_lfo1_magnitude')

    # ── Check modulation routings for pitch/filter depth ─────────────────
    # Surge stores modrouting as child elements of the destination param
    lfo_pitch_depth = 0.0
    lfo_filter_depth = 0.0

    # Check pitch modrouting
    pitch_el = P.get('a_osc1_pitch')
    if pitch_el is not None:
        for mod in pitch_el.findall('modrouting'):
            src = int(mod.get('source', 0))
            depth = float(mod.get('depth', 0))
            if src in (1, 2, 3, 4, 5, 6):  # LFO sources
                lfo_pitch_depth = max(lfo_pitch_depth, abs(depth) / 12.0)

    f1cut_el = P.get('a_filter1_cutoff')
    if f1cut_el is not None:
        for mod in f1cut_el.findall('modrouting'):
            src = int(mod.get('source', 0))
            depth = float(mod.get('depth', 0))
            if src in (1, 2, 3, 4, 5, 6):
                lfo_filter_depth = max(lfo_filter_depth, abs(depth) / 20.0)

    # ── FX slots ──────────────────────────────────────────────────────────
    # Surge FX slots 1-8, types: 1=EQ, 2=Chorus, 3=Flanger, 4=Phaser,
    # 5=Rotary, 6=Delay, 8=Reverb1, 10=Reverb2, 11=Airwindows, ...
    reverb_mix = 0.0
    chorus_mix = 0.0
    delay_mix  = 0.0
    chorus_rate = 0.5
    chorus_depth = 0.3

    fx_disable = int(gp('fx_disable'))
    for slot in range(1, 9):
        if (fx_disable >> (slot - 1)) & 1:
            continue  # disabled
        ftype = int(gp(f'fx{slot}_type'))
        p0 = gp(f'fx{slot}_p0')
        p1 = gp(f'fx{slot}_p1')
        if ftype == 2:   # Chorus
            chorus_mix = max(chorus_mix, 0.3)
            chorus_rate = max(0.1, min(p0, 8.0)) if p0 > 0 else 1.0
        elif ftype == 3: # Flanger → treat as chorus
            chorus_mix = max(chorus_mix, 0.2)
        elif ftype == 6: # Delay
            delay_mix = max(delay_mix, 0.3)
        elif ftype == 8 or ftype == 10:  # Reverb 1 or 2
            reverb_mix = max(reverb_mix, 0.35)
        elif ftype == 5: # Rotary (Leslie)
            chorus_mix = max(chorus_mix, 0.25)

    # Clamp mix values
    reverb_mix = min(reverb_mix, 0.6)
    chorus_mix = min(chorus_mix, 0.5)
    delay_mix  = min(delay_mix, 0.4)

    # ── Octave shift → detune ─────────────────────────────────────────────
    # osc1_oct shifts by octaves (-2..+2), incorporate into our detune
    # (our detune is in ratio units, 1 octave = 1.0 in detune param)
    # Actually for transpose we use osc_detune differently; just note it
    transpose_semitones = osc1_oct * 12

    # ── Wavetable file ────────────────────────────────────────────────────
    wavetable_file = None
    if our_osc == 27 and osc1_type == 2:  # Wavetable
        wt_name = gp('a_osc1_wavetable_name') if 'a_osc1_wavetable_name' in P else None
        # Try to find .wt file
        if wt_name is None:
            # Look for wavetable param
            wt_el = P.get('a_osc1_wavetable_name')
            if wt_el is not None:
                wt_name = wt_el.get('value', '')
        if wt_name:
            wt_rel = find_wt_file(wt_name)
            if wt_rel:
                wavetable_file = wt_rel

    # ── Master volume ─────────────────────────────────────────────────────
    master_vol = max(0.0, min(1.0, gp('volume', 0.0) / 24.0 + 0.75))

    # ── Build output params ───────────────────────────────────────────────
    out = {
        "name": name,
        "category": category,
        "params": {
            "osc_type": float(our_osc),
            "osc_detune": max(-0.1, min(0.1, detune)),
            "filter_type": float(our_ftype),
            "filter_cutoff": round(our_fcut, 1),
            "filter_resonance": round(max(0.0, min(0.99, f1_reso)), 3),
            "filter_env_amount": round(env_hz, 1),
            "filter_key_track": round(max(-1.0, min(1.0, f1_ktrack)), 3),
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
            "lfo_waveform": float(lfo0_shape),
            "lfo_rate": round(lfo0_rate, 3),
            "lfo_pitch_depth": round(lfo_pitch_depth, 3),
            "lfo_filter_depth": round(lfo_filter_depth, 3),
            "lfo_deform": round(lfo0_deform, 3),
            "lfo1_waveform": float(lfo1_shape),
            "lfo1_rate": round(lfo1_rate, 3),
            "reverb_mix": round(reverb_mix, 2),
            "chorus_mix": round(chorus_mix, 2),
            "delay_mix":  round(delay_mix, 2),
            "master_volume": round(master_vol, 2),
        }
    }

    # Osc 2 if active
    if osc2_level > 0.05:
        out["params"]["osc_count"] = 2.0
        out["params"]["osc2_type"] = float(our_osc2)
        out["params"]["osc2_level"] = round(osc2_level, 2)
        out["params"]["osc1_level"] = 1.0

    # Wavetable file
    if wavetable_file:
        out["wavetable_file"] = wavetable_file

    return out


def find_wt_file(name: str) -> str | None:
    """Find a .wt file by name in our wavetable dir. Returns relative path or None."""
    name_lower = name.lower().strip()
    for wt in WT_DIR.rglob("*.wt"):
        if wt.stem.lower() == name_lower:
            return str(wt.relative_to(WT_DIR))
    return None


def convert_all():
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    converted = 0
    failed = 0

    for cat_dir in sorted(SURGE_PRESETS.iterdir()):
        if not cat_dir.is_dir():
            continue
        category = cat_dir.name

        # Map Surge categories to our categories
        cat_map = {
            "Basses": "Bass", "Leads": "Lead", "Pads": "Pad",
            "Plucks": "Pluck", "Polysynths": "Pad", "Sequences": "Seq",
            "Keys": "Keys", "Brass": "Brass", "FX": "FX",
            "Chords": "Pad", "Winds": "Lead", "Percussion": "Perc",
            "MPE": "MPE", "Splits": "Splits", "Templates": "Templates",
            "Vocoder": "FX",
        }
        out_cat = cat_map.get(category, category)
        out_subdir = OUT_DIR / f"Surge_{out_cat}"
        out_subdir.mkdir(exist_ok=True)

        for fxp in sorted(cat_dir.glob("*.fxp")):
            result = convert_fxp(fxp, f"Surge {out_cat}")
            if result is None:
                failed += 1
                continue

            # Safe filename
            safe_name = result["name"].replace("/", "-").replace("\\", "-")
            safe_name = "".join(c if c.isalnum() or c in " _-()" else "_" for c in safe_name)
            out_path = out_subdir / f"{safe_name}.json"

            with open(out_path, 'w') as f:
                json.dump(result, f, indent=2)
            converted += 1

    print(f"✓ Converted: {converted}")
    print(f"✗ Failed:    {failed}")
    print(f"Output: {OUT_DIR}")


if __name__ == "__main__":
    convert_all()
