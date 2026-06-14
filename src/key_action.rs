/// Shared key action definitions used by both GUI keybinds and headless evdev input.
/// No GUI dependencies — safe to use in headless mode.

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum KeyAction {
    SwitchPart(u8),
    LooperRecord,
    LooperTogglePlay,
    LooperUndo,
    LooperClear,
    DrumTogglePlay,
    ToggleDrumsSynth,
}

#[allow(dead_code)]
impl KeyAction {
    pub fn label(&self) -> &'static str {
        match self {
            Self::SwitchPart(0) => "Part A1",
            Self::SwitchPart(1) => "Part A2",
            Self::SwitchPart(2) => "Part A3",
            Self::SwitchPart(3) => "Part A4",
            Self::SwitchPart(4) => "Part B1",
            Self::SwitchPart(5) => "Part B2",
            Self::SwitchPart(6) => "Part B3",
            Self::SwitchPart(7) => "Part B4",
            Self::SwitchPart(_) => "Part ?",
            Self::LooperRecord => "Looper Record",
            Self::LooperTogglePlay => "Looper Play/Stop",
            Self::LooperUndo => "Looper Undo",
            Self::LooperClear => "Looper Clear",
            Self::DrumTogglePlay => "Drum Seq Play/Stop",
            Self::ToggleDrumsSynth => "Toggle Drums/Synth",
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Self::SwitchPart(p) => format!("SwitchPart({p})"),
            Self::LooperRecord => "LooperRecord".into(),
            Self::LooperTogglePlay => "LooperTogglePlay".into(),
            Self::LooperUndo => "LooperUndo".into(),
            Self::LooperClear => "LooperClear".into(),
            Self::DrumTogglePlay => "DrumTogglePlay".into(),
            Self::ToggleDrumsSynth => "ToggleDrumsSynth".into(),
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        if let Some(rest) = s.strip_prefix("SwitchPart(") {
            let num = rest.strip_suffix(')')?.parse::<u8>().ok()?;
            return Some(Self::SwitchPart(num));
        }
        match s {
            "LooperRecord" => Some(Self::LooperRecord),
            "LooperTogglePlay" => Some(Self::LooperTogglePlay),
            "LooperUndo" => Some(Self::LooperUndo),
            "LooperClear" => Some(Self::LooperClear),
            "DrumTogglePlay" => Some(Self::DrumTogglePlay),
            "ToggleDrumsSynth" => Some(Self::ToggleDrumsSynth),
            _ => None,
        }
    }

    /// All bindable actions in display order.
    pub fn all() -> Vec<Self> {
        vec![
            Self::SwitchPart(0), Self::SwitchPart(1),
            Self::SwitchPart(2), Self::SwitchPart(3),
            Self::SwitchPart(4), Self::SwitchPart(5),
            Self::SwitchPart(6), Self::SwitchPart(7),
            Self::LooperRecord, Self::LooperTogglePlay,
            Self::LooperUndo, Self::LooperClear,
            Self::DrumTogglePlay, Self::ToggleDrumsSynth,
        ]
    }
}
