//! Keyboard shortcut system for live performance.
//! Binds computer keyboard keys to actions (switch part, looper controls, etc.).
//! Persisted in config.json as `keybinds: { "F1": "SwitchPart(0)", ... }`.

use eframe::egui;
use std::collections::HashMap;

pub use crate::key_action::KeyAction;

/// Resolved keybind: egui::Key → KeyAction.
pub struct Keybinds {
    binds: Vec<(egui::Key, KeyAction)>,
}

impl Keybinds {
    pub fn from_config(map: &HashMap<String, String>) -> Self {
        let mut binds = Vec::new();
        for (key_str, action_str) in map {
            if let (Some(key), Some(action)) = (key_from_str(key_str), KeyAction::from_str(action_str)) {
                binds.push((key, action));
            }
        }
        // Fill in defaults for actions not yet bound
        if binds.is_empty() {
            binds = Self::defaults().binds;
        }
        Self { binds }
    }

    pub fn defaults() -> Self {
        Self {
            binds: vec![
                (egui::Key::F1, KeyAction::SwitchPart(0)),
                (egui::Key::F2, KeyAction::SwitchPart(1)),
                (egui::Key::F3, KeyAction::SwitchPart(2)),
                (egui::Key::F4, KeyAction::SwitchPart(3)),
                (egui::Key::F5, KeyAction::SwitchPart(4)),
                (egui::Key::F6, KeyAction::SwitchPart(5)),
                (egui::Key::F7, KeyAction::SwitchPart(6)),
                (egui::Key::F8, KeyAction::SwitchPart(7)),
                (egui::Key::Space, KeyAction::LooperTogglePlay),
                (egui::Key::R, KeyAction::LooperRecord),
                (egui::Key::Z, KeyAction::LooperUndo),
                (egui::Key::X, KeyAction::LooperClear),
                (egui::Key::Tab, KeyAction::ToggleDrumsSynth),
                (egui::Key::OpenBracket, KeyAction::PrevPreset),
                (egui::Key::CloseBracket, KeyAction::NextPreset),
            ],
        }
    }

    pub fn to_config(&self) -> HashMap<String, String> {
        self.binds.iter()
            .map(|(key, action)| (key_to_str(*key).to_string(), action.to_config_string()))
            .collect()
    }

    pub fn binds(&self) -> &[(egui::Key, KeyAction)] {
        &self.binds
    }

    pub fn binds_mut(&mut self) -> &mut Vec<(egui::Key, KeyAction)> {
        &mut self.binds
    }

    pub fn set_key_for_action(&mut self, action: &KeyAction, new_key: egui::Key) {
        // Remove any existing bind for this key
        self.binds.retain(|(k, _)| *k != new_key);
        // Update or add
        if let Some(entry) = self.binds.iter_mut().find(|(_, a)| a == action) {
            entry.0 = new_key;
        } else {
            self.binds.push((new_key, action.clone()));
        }
    }

    pub fn key_for_action(&self, action: &KeyAction) -> Option<egui::Key> {
        self.binds.iter().find(|(_, a)| a == action).map(|(k, _)| *k)
    }
}

// --- Key ↔ String conversion ---

const KEY_TABLE: &[(egui::Key, &str)] = &[
    (egui::Key::A, "A"), (egui::Key::B, "B"), (egui::Key::C, "C"),
    (egui::Key::D, "D"), (egui::Key::E, "E"), (egui::Key::F, "F"),
    (egui::Key::G, "G"), (egui::Key::H, "H"), (egui::Key::I, "I"),
    (egui::Key::J, "J"), (egui::Key::K, "K"), (egui::Key::L, "L"),
    (egui::Key::M, "M"), (egui::Key::N, "N"), (egui::Key::O, "O"),
    (egui::Key::P, "P"), (egui::Key::Q, "Q"), (egui::Key::R, "R"),
    (egui::Key::S, "S"), (egui::Key::T, "T"), (egui::Key::U, "U"),
    (egui::Key::V, "V"), (egui::Key::W, "W"), (egui::Key::X, "X"),
    (egui::Key::Y, "Y"), (egui::Key::Z, "Z"),
    (egui::Key::Num0, "0"), (egui::Key::Num1, "1"), (egui::Key::Num2, "2"),
    (egui::Key::Num3, "3"), (egui::Key::Num4, "4"), (egui::Key::Num5, "5"),
    (egui::Key::Num6, "6"), (egui::Key::Num7, "7"), (egui::Key::Num8, "8"),
    (egui::Key::Num9, "9"),
    (egui::Key::F1, "F1"), (egui::Key::F2, "F2"), (egui::Key::F3, "F3"),
    (egui::Key::F4, "F4"), (egui::Key::F5, "F5"), (egui::Key::F6, "F6"),
    (egui::Key::F7, "F7"), (egui::Key::F8, "F8"), (egui::Key::F9, "F9"),
    (egui::Key::F10, "F10"), (egui::Key::F11, "F11"), (egui::Key::F12, "F12"),
    (egui::Key::Space, "Space"), (egui::Key::Tab, "Tab"),
    (egui::Key::Enter, "Enter"), (egui::Key::Escape, "Escape"),
    (egui::Key::Backspace, "Backspace"),
    (egui::Key::ArrowUp, "Up"), (egui::Key::ArrowDown, "Down"),
    (egui::Key::ArrowLeft, "Left"), (egui::Key::ArrowRight, "Right"),
    (egui::Key::Home, "Home"), (egui::Key::End, "End"),
    (egui::Key::PageUp, "PageUp"), (egui::Key::PageDown, "PageDown"),
    (egui::Key::Delete, "Delete"), (egui::Key::Insert, "Insert"),
    (egui::Key::Minus, "Minus"), (egui::Key::Plus, "Plus"),
    (egui::Key::OpenBracket, "["), (egui::Key::CloseBracket, "]"),
];

pub fn key_to_str(key: egui::Key) -> &'static str {
    KEY_TABLE.iter().find(|(k, _)| *k == key).map(|(_, s)| *s).unwrap_or("?")
}

pub fn key_from_str(s: &str) -> Option<egui::Key> {
    KEY_TABLE.iter().find(|(_, name)| *name == s).map(|(k, _)| *k)
}

/// Detect any key press this frame (for rebind capture).
pub fn detect_key_press(ctx: &egui::Context) -> Option<egui::Key> {
    ctx.input(|i| {
        KEY_TABLE.iter().map(|&(key, _)| key).find(|&key| i.key_pressed(key))
    })
}
