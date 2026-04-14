/// CC mapping: maps MIDI CC numbers to synth parameters.
/// RT-safe: CcBinding is Copy, CcMap uses fixed [Option<CcBinding>; 128] — zero heap.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamScope {
    Global,
    Patch,
}

/// Metadata for a mappable parameter.
#[allow(dead_code)]
pub struct ParamMeta {
    pub key: &'static str,
    pub label: &'static str,
    pub min: f32,
    pub max: f32,
    pub logarithmic: bool,
    pub scope: ParamScope,
}

/// All mappable parameters.
pub const PARAM_REGISTRY: &[ParamMeta] = &[
    // Global params (persist across patch changes — only volume and tone)
    ParamMeta { key: "master_volume", label: "Volume", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Global },
    ParamMeta { key: "master_tone", label: "Tone", min: 200.0, max: 20000.0, logarithmic: true, scope: ParamScope::Global },
    // Patch params (reset on patch change)
    ParamMeta { key: "filter_cutoff", label: "Filter Cutoff", min: 20.0, max: 20000.0, logarithmic: true, scope: ParamScope::Patch },
    ParamMeta { key: "filter_resonance", label: "Filter Resonance", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "filter_env_amount", label: "Filter Env Amt", min: 0.0, max: 15000.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "amp_attack", label: "Amp Attack", min: 0.001, max: 5.0, logarithmic: true, scope: ParamScope::Patch },
    ParamMeta { key: "amp_decay", label: "Amp Decay", min: 0.0, max: 5.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "amp_sustain", label: "Amp Sustain", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "amp_release", label: "Amp Release", min: 0.001, max: 5.0, logarithmic: true, scope: ParamScope::Patch },
    ParamMeta { key: "filter_attack", label: "Filter Attack", min: 0.001, max: 5.0, logarithmic: true, scope: ParamScope::Patch },
    ParamMeta { key: "filter_decay", label: "Filter Decay", min: 0.0, max: 5.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "filter_sustain", label: "Filter Sustain", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "filter_release", label: "Filter Release", min: 0.001, max: 5.0, logarithmic: true, scope: ParamScope::Patch },
    ParamMeta { key: "lfo_rate", label: "LFO Rate", min: 0.1, max: 20.0, logarithmic: true, scope: ParamScope::Patch },
    ParamMeta { key: "lfo_pitch_depth", label: "LFO Pitch", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "lfo_filter_depth", label: "LFO Filter", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "lfo_amp_depth", label: "LFO Amp", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "vel_to_filter", label: "Vel->Filter", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "noise_level", label: "Noise Mix", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "osc_detune", label: "Osc Detune", min: 0.0, max: 0.05, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "portamento_time", label: "Portamento", min: 0.0, max: 2.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "unison_detune", label: "Unison Detune", min: 0.0, max: 50.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "chorus_mix", label: "Chorus", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "delay_mix", label: "Delay Mix", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "delay_time_l", label: "Delay Time L", min: 0.01, max: 2.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "delay_time_r", label: "Delay Time R", min: 0.01, max: 2.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "delay_feedback", label: "Delay Feedback", min: 0.0, max: 0.95, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "delay_filter", label: "Delay Filter", min: 0.0, max: 0.95, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "reverb_mix", label: "Reverb Mix", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "reverb_room_size", label: "Reverb Room", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "reverb_damping", label: "Reverb Damp", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "reverb_width", label: "Reverb Width", min: 0.0, max: 1.0, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "reverb_pre_delay", label: "Reverb Pre-Delay", min: 0.0, max: 0.1, logarithmic: false, scope: ParamScope::Patch },
    ParamMeta { key: "pitch_bend_range", label: "Pitch Bend Range (st)", min: 1.0, max: 24.0, logarithmic: false, scope: ParamScope::Global },
];

/// List of global param keys for quick lookup.
pub const GLOBAL_PARAM_KEYS: &[&str] = &[
    "master_volume", "master_tone", "reverb_mix", "delay_mix", "pitch_bend_range",
];

pub fn is_global_param(key: &str) -> bool {
    GLOBAL_PARAM_KEYS.contains(&key)
}

pub fn find_param_meta(key: &str) -> Option<&'static ParamMeta> {
    PARAM_REGISTRY.iter().find(|m| m.key == key)
}

/// Resolve a string key to &'static str from PARAM_REGISTRY.
/// Called on GUI thread only (config load, MIDI learn).
pub fn resolve_key(key: &str) -> Option<&'static str> {
    PARAM_REGISTRY.iter().find(|m| m.key == key).map(|m| m.key)
}

/// Human-readable name for SMK-37 Pro controls.
pub fn cc_to_control_name(cc: u8) -> &'static str {
    match cc {
        48 => "K1", 49 => "K2", 50 => "K3", 51 => "K4",
        52 => "K5", 53 => "K6", 54 => "K7", 55 => "K8",
        64 => "F1", 65 => "F2", 66 => "F3", 67 => "F4",
        _ => "CC",
    }
}

/// RT-safe CC binding. All fields are Copy — no heap, no drop.
#[derive(Clone, Copy, Debug)]
pub struct CcBinding {
    pub param_key: &'static str,
    pub min_val: f32,
    pub max_val: f32,
    pub logarithmic: bool,
    pub scope: ParamScope,
}

/// RT-safe CC map. Fixed-size array indexed by CC number (0-127).
/// Copy — can be sent through rtrb with zero allocation.
#[derive(Clone, Copy, Debug)]
pub struct CcMap {
    pub bindings: [Option<CcBinding>; 128],
}

impl Default for CcMap {
    fn default() -> Self {
        Self::default_map()
    }
}

impl CcMap {
    pub fn default_map() -> Self {
        let mut bindings = [None; 128];
        // SMK-37 Pro: Knobs K1-K8 = CC 48-55 (all patch-scoped)
        bindings[48] = Some(CcBinding {
            param_key: "filter_cutoff", min_val: 20.0, max_val: 20000.0, logarithmic: true, scope: ParamScope::Patch,
        });
        bindings[49] = Some(CcBinding {
            param_key: "filter_resonance", min_val: 0.0, max_val: 1.0, logarithmic: false, scope: ParamScope::Patch,
        });
        bindings[50] = Some(CcBinding {
            param_key: "filter_env_amount", min_val: 0.0, max_val: 15000.0, logarithmic: false, scope: ParamScope::Patch,
        });
        bindings[51] = Some(CcBinding {
            param_key: "amp_release", min_val: 0.001, max_val: 5.0, logarithmic: true, scope: ParamScope::Patch,
        });
        bindings[52] = Some(CcBinding {
            param_key: "amp_attack", min_val: 0.001, max_val: 5.0, logarithmic: true, scope: ParamScope::Patch,
        });
        bindings[53] = Some(CcBinding {
            param_key: "portamento_time", min_val: 0.0, max_val: 2.0, logarithmic: false, scope: ParamScope::Patch,
        });
        bindings[54] = Some(CcBinding {
            param_key: "lfo_filter_depth", min_val: 0.0, max_val: 1.0, logarithmic: false, scope: ParamScope::Patch,
        });
        bindings[55] = Some(CcBinding {
            param_key: "chorus_mix", min_val: 0.0, max_val: 1.0, logarithmic: false, scope: ParamScope::Patch,
        });
        // SMK-37 Pro: Faders F1-F4 = CC 64-67 (all global — persist across presets)
        bindings[64] = Some(CcBinding {
            param_key: "master_volume", min_val: 0.0, max_val: 1.0, logarithmic: false, scope: ParamScope::Global,
        });
        bindings[65] = Some(CcBinding {
            param_key: "master_tone", min_val: 200.0, max_val: 20000.0, logarithmic: true, scope: ParamScope::Global,
        });
        bindings[66] = Some(CcBinding {
            param_key: "reverb_mix", min_val: 0.0, max_val: 1.0, logarithmic: false, scope: ParamScope::Global,
        });
        bindings[67] = Some(CcBinding {
            param_key: "delay_mix", min_val: 0.0, max_val: 1.0, logarithmic: false, scope: ParamScope::Global,
        });
        Self { bindings }
    }

    /// Load from config JSON. Runs on GUI thread — String→&'static str resolution here.
    pub fn from_config(config: &crate::config::Config) -> Self {
        let raw = config.cc_map.as_ref();
        let Some(raw) = raw else { return Self::default_map() };
        let mut map = Self::default_map();
        // Overwrite from saved config
        for (cc_str, raw_binding) in &raw.bindings {
            let cc = *cc_str;
            if (cc as usize) < 128 {
                if let Some(static_key) = resolve_key(&raw_binding.param_key) {
                    let scope = find_param_meta(static_key).map(|m| m.scope).unwrap_or(ParamScope::Patch);
                    map.bindings[cc as usize] = Some(CcBinding {
                        param_key: static_key,
                        min_val: raw_binding.min_val,
                        max_val: raw_binding.max_val,
                        logarithmic: raw_binding.logarithmic,
                        scope,
                    });
                }
            }
        }
        map
    }

    /// Convert CC value (0-127) to parameter value using binding's range.
    pub fn cc_to_param(binding: &CcBinding, cc_value: u8) -> f32 {
        let t = cc_value as f32 / 127.0;
        if binding.logarithmic {
            let log_min = binding.min_val.max(0.001).ln();
            let log_max = binding.max_val.ln();
            (log_min + t * (log_max - log_min)).exp()
        } else {
            binding.min_val + t * (binding.max_val - binding.min_val)
        }
    }

    /// Convert parameter value back to CC value (0-127) — inverse of cc_to_param.
    pub fn param_to_cc(binding: &CcBinding, param_value: f32) -> u8 {
        let t = if binding.logarithmic {
            let log_min = binding.min_val.max(0.001).ln();
            let log_max = binding.max_val.ln();
            (param_value.max(0.001).ln() - log_min) / (log_max - log_min)
        } else {
            (param_value - binding.min_val) / (binding.max_val - binding.min_val)
        };
        (t.clamp(0.0, 1.0) * 127.0).round() as u8
    }
}

/// Serde-compatible raw CC map for config file loading/saving.
/// Only used on GUI thread for JSON serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CcBindingRaw {
    pub param_key: String,
    pub min_val: f32,
    pub max_val: f32,
    pub logarithmic: bool,
    #[serde(default = "default_scope")]
    pub scope: ParamScope,
}

fn default_scope() -> ParamScope { ParamScope::Patch }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CcMapRaw {
    pub bindings: std::collections::BTreeMap<u8, CcBindingRaw>,
}

impl CcMapRaw {
    /// Convert RT-safe CcMap to serializable CcMapRaw.
    pub fn from_cc_map(map: &CcMap) -> Self {
        let mut bindings = std::collections::BTreeMap::new();
        for (i, slot) in map.bindings.iter().enumerate() {
            if let Some(b) = slot {
                bindings.insert(i as u8, CcBindingRaw {
                    param_key: b.param_key.to_string(),
                    min_val: b.min_val,
                    max_val: b.max_val,
                    logarithmic: b.logarithmic,
                    scope: b.scope,
                });
            }
        }
        Self { bindings }
    }
}
