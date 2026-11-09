//! App substructs: god-struct App разбит на подсистемы (techdebt S4-gui
//! residue). Behavior-preserving move — поля verbatim из `App` (тип, vis,
//! doc- и cfg-атрибуты сохранены 1:1).

use super::*;

/// LED console / companion-process state.
pub struct LedUi {
    // Bank-2 pad console
    pub console: led_console::LedConsole,
    pub console_rx: Option<std::sync::mpsc::Receiver<(u8, bool)>>,
    pub flash: note_flash::NoteFlashState,
    pub flash_rx: rtrb::Consumer<crate::midi::NoteFlash>,
    pub led_snapshot: [[u8; 3]; 16],
    pub led_dirty: bool,
    pub led_socket: Option<std::sync::mpsc::Sender<String>>,
    pub led_socket_alive: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub led_socket_failed_at: Option<std::time::Instant>,
    /// In-flight connect attempt (runs off the GUI thread — unix connect
    /// blocks on a full peer backlog).
    pub led_connecting:
        Option<std::sync::mpsc::Receiver<std::io::Result<std::os::unix::net::UnixStream>>>,
    pub led_last_send: Option<std::time::Instant>,
    /// Companion process WE spawned (never kill a foreign one).
    pub led_child: Option<std::process::Child>,
    /// Backoff gate for (re)spawn attempts (crash / spawn failure).
    pub led_spawn_backoff: Option<std::time::Instant>,
    /// Last seen console mode — mode-entry window sync (#50 finding 4).
    pub console_prev_mode: Option<led_console::Mode>,
    // Navigate button timing
    pub nav_press_time: Option<std::time::Instant>,
    pub mute_window: usize,   // first part idx visible on MUTE slots 1..=7
}
