//! Headless keyboard input via Linux evdev.
//! Reads key events from /dev/input/event* devices in a background thread,
//! maps them to KeyAction and sends corresponding ControlEvent to the audio engine.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::key_action::KeyAction;

/// evdev key code → config key name string.
/// Same names as used in config.json keybinds (shared with GUI).
fn evdev_key_to_str(code: u16) -> Option<&'static str> {
    // Key codes from linux/input-event-codes.h
    match code {
        16 => Some("Q"), 17 => Some("W"), 18 => Some("E"), 19 => Some("R"),
        20 => Some("T"), 21 => Some("Y"), 22 => Some("U"), 23 => Some("I"),
        24 => Some("O"), 25 => Some("P"),
        30 => Some("A"), 31 => Some("S"), 32 => Some("D"), 33 => Some("F"),
        34 => Some("G"), 35 => Some("H"), 36 => Some("J"), 37 => Some("K"),
        38 => Some("L"),
        44 => Some("Z"), 45 => Some("X"), 46 => Some("C"), 47 => Some("V"),
        48 => Some("B"), 49 => Some("N"), 50 => Some("M"),
        2  => Some("1"), 3  => Some("2"), 4  => Some("3"), 5  => Some("4"),
        6  => Some("5"), 7  => Some("6"), 8  => Some("7"), 9  => Some("8"),
        10 => Some("9"), 11 => Some("0"),
        57 => Some("Space"), 15 => Some("Tab"), 28 => Some("Enter"),
        1  => Some("Escape"), 14 => Some("Backspace"),
        103 => Some("Up"), 108 => Some("Down"), 105 => Some("Left"), 106 => Some("Right"),
        102 => Some("Home"), 107 => Some("End"),
        104 => Some("PageUp"), 109 => Some("PageDown"),
        111 => Some("Delete"), 110 => Some("Insert"),
        12 => Some("Minus"), 13 => Some("Plus"),
        59 => Some("F1"), 60 => Some("F2"), 61 => Some("F3"), 62 => Some("F4"),
        63 => Some("F5"), 64 => Some("F6"), 65 => Some("F7"), 66 => Some("F8"),
        67 => Some("F9"), 68 => Some("F10"), 87 => Some("F11"), 88 => Some("F12"),
        _ => None,
    }
}

/// Build a lookup table: evdev key code → KeyAction, from config keybinds.
fn build_evdev_binds(keybinds: &HashMap<String, String>) -> HashMap<u16, KeyAction> {
    let mut map = HashMap::new();

    // Build reverse: key_name → evdev_code
    let mut name_to_code: HashMap<&str, u16> = HashMap::new();
    for code in 0..256u16 {
        if let Some(name) = evdev_key_to_str(code) {
            name_to_code.insert(name, code);
        }
    }

    for (key_name, action_str) in keybinds {
        if let (Some(&code), Some(action)) = (name_to_code.get(key_name.as_str()), KeyAction::from_str(action_str)) {
            map.insert(code, action);
        }
    }

    // If empty (no config), use defaults matching GUI defaults
    if map.is_empty() {
        let defaults: &[(&str, KeyAction)] = &[
            ("F1", KeyAction::SwitchPart(0)), ("F2", KeyAction::SwitchPart(1)),
            ("F3", KeyAction::SwitchPart(2)), ("F4", KeyAction::SwitchPart(3)),
            ("F5", KeyAction::SwitchPart(4)), ("F6", KeyAction::SwitchPart(5)),
            ("F7", KeyAction::SwitchPart(6)), ("F8", KeyAction::SwitchPart(7)),
            ("Space", KeyAction::LooperTogglePlay), ("R", KeyAction::LooperRecord),
            ("Z", KeyAction::LooperUndo), ("X", KeyAction::LooperClear),
            ("Tab", KeyAction::ToggleDrumsSynth),
        ];
        for (name, action) in defaults {
            if let Some(&code) = name_to_code.get(name) {
                map.insert(code, action.clone());
            }
        }
    }

    map
}

/// Find keyboard devices in /dev/input/.
fn find_keyboards() -> Vec<std::path::PathBuf> {
    let mut keyboards = Vec::new();
    let Ok(entries) = std::fs::read_dir("/dev/input") else { return keyboards };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("event") { continue; }

        // Check if device has EV_KEY capability by reading its bits
        if let Ok(file) = std::fs::File::open(&path) {
            use std::os::unix::io::AsRawFd;
            let fd = file.as_raw_fd();
            let mut ev_bits = [0u8; 4]; // EV_MAX/8 + 1, we only need bit 1 (EV_KEY)
            // EVIOCGBIT(0, size) = ioctl to get event type bits
            let ret = unsafe {
                libc::ioctl(fd, 0x80044520u64, ev_bits.as_mut_ptr()) // EVIOCGBIT(0, 4)
            };
            if ret >= 0 && (ev_bits[0] & (1 << 1)) != 0 {
                // Has EV_KEY — check if it has actual letter keys (not just a mouse)
                let mut key_bits = [0u8; 128]; // KEY_MAX/8 + 1
                let ret2 = unsafe {
                    libc::ioctl(fd, 0x80804521u64, key_bits.as_mut_ptr()) // EVIOCGBIT(EV_KEY, 128)
                };
                if ret2 >= 0 {
                    // Check for KEY_A (30) presence — real keyboards have it
                    let has_keys = (key_bits[30 / 8] & (1 << (30 % 8))) != 0;
                    if has_keys {
                        keyboards.push(path);
                    }
                }
            }
        }
    }
    keyboards.sort();
    keyboards
}

/// Spawn a background thread that reads keyboard events and sends KeyActions.
/// Returns (join handle, receiver). The main loop reads KeyActions and translates
/// them to ControlEvents — this avoids sharing rtrb::Producer across threads.
pub fn spawn_keyboard_thread(
    keybinds: &HashMap<String, String>,
    shutdown: Arc<AtomicBool>,
) -> Option<(std::thread::JoinHandle<()>, std::sync::mpsc::Receiver<KeyAction>)> {
    let binds = build_evdev_binds(keybinds);
    if binds.is_empty() { return None; }

    let keyboards = find_keyboards();
    if keyboards.is_empty() {
        eprintln!("[input] No keyboard devices found in /dev/input/");
        eprintln!("[input] Hint: run as root or add user to 'input' group");
        return None;
    }

    for kb in &keyboards {
        eprintln!("[input] Found keyboard: {}", kb.display());
    }

    let (tx, rx) = std::sync::mpsc::channel();

    let handle = std::thread::spawn(move || {
        run_keyboard_loop(keyboards, binds, tx, shutdown);
    });

    Some((handle, rx))
}

fn run_keyboard_loop(
    keyboard_paths: Vec<std::path::PathBuf>,
    binds: HashMap<u16, KeyAction>,
    action_tx: std::sync::mpsc::Sender<KeyAction>,
    shutdown: Arc<AtomicBool>,
) {
    use std::os::unix::io::AsRawFd;

    // Open all keyboard devices
    let mut fds: Vec<(std::fs::File, std::path::PathBuf)> = Vec::new();
    for path in &keyboard_paths {
        match std::fs::File::open(path) {
            Ok(file) => {
                // Set non-blocking
                unsafe {
                    let fd = file.as_raw_fd();
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
                fds.push((file, path.clone()));
            }
            Err(e) => eprintln!("[input] Cannot open {}: {e}", path.display()),
        }
    }

    if fds.is_empty() {
        eprintln!("[input] No keyboard devices opened");
        return;
    }

    // epoll setup
    let epoll_fd = unsafe { libc::epoll_create1(0) };
    if epoll_fd < 0 {
        eprintln!("[input] epoll_create1 failed");
        return;
    }

    for (i, (file, _)) in fds.iter().enumerate() {
        let mut ev = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: i as u64,
        };
        unsafe {
            libc::epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, file.as_raw_fd(), &mut ev);
        }
    }

    // struct input_event: timeval(16) + type(2) + code(2) + value(4) = 24 bytes on 64-bit
    const INPUT_EVENT_SIZE: usize = 24;
    // Compile-time check: sizeof(timeval) must be 16 (64-bit Linux)
    const _: () = assert!(std::mem::size_of::<libc::timeval>() == 16);
    let mut buf = [0u8; INPUT_EVENT_SIZE * 16]; // Read up to 16 events at once
    let mut events = [libc::epoll_event { events: 0, u64: 0 }; 4];

    eprintln!("[input] Keyboard input active ({} device(s))", fds.len());

    while !shutdown.load(Ordering::Relaxed) {
        let n = unsafe {
            libc::epoll_wait(epoll_fd, events.as_mut_ptr(), events.len() as i32, 200) // 200ms timeout
        };

        if n <= 0 { continue; }

        for event in &events[..n as usize] {
            let dev_idx = event.u64 as usize;
            let fd = fds[dev_idx].0.as_raw_fd();

            loop {
                let bytes = unsafe {
                    libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len())
                };
                if bytes <= 0 { break; }

                let count = bytes as usize / INPUT_EVENT_SIZE;

                for j in 0..count {
                    let offset = j * INPUT_EVENT_SIZE;
                    // Parse: skip timeval (16 bytes), read type(2), code(2), value(4)
                    let ev_type = u16::from_ne_bytes([buf[offset + 16], buf[offset + 17]]);
                    let ev_code = u16::from_ne_bytes([buf[offset + 18], buf[offset + 19]]);
                    let ev_value = i32::from_ne_bytes([
                        buf[offset + 20], buf[offset + 21],
                        buf[offset + 22], buf[offset + 23],
                    ]);

                    // EV_KEY = 1, value 1 = press, 0 = release, 2 = repeat
                    if ev_type != 1 || ev_value != 1 { continue; }

                    if let Some(action) = binds.get(&ev_code) {
                        let _ = action_tx.send(action.clone());
                    }
                }
            }
        }
    }

    unsafe { libc::close(epoll_fd); }
    eprintln!("[input] Keyboard input stopped");
}
