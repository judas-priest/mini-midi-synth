# Companion Auto-Spawn (TECHDEBT #16) — Implementation Plan

anchor-checked: 2026-10-29 (якоря сняты с master @ d4c22d7 чтением кода в этой сессии)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Галка `config.ui.led_companion: true` → синт сам запускает компаньон
`python3 ~/Projects/smk37_config/pad_console.py` (путь настраивается,
`config.ui.led_companion_cmd: String`). Галка off → убивает **только своего**
child'а. Компаньон умер, пока галка on → автореспавн с backoff 5 с. Двойной
запуск разруливает сам компаньон (bind-проба → тихий exit 0).

**Architecture (решено, не менять):**
- Спавн на GUI-потоке в `App::update` рядом с существующим LED socket lifecycle
  (gui/mod.rs:580). `std::process::Command::new("python3").arg(expand_home(&cmd))` —
  `python3` захардкожен, конфигурируется только путь. `~` расширяется через
  `dirs::home_dir()` (крейт `dirs = "6"` уже в Cargo.toml:36, другого прецедента
  ручного расширения `~` в базе нет).
- Kill: `child.kill()` + `child.wait()` (не зомби). Только `led_child`, который мы
  заспавнили — чужой процесс не трогаем. SIGKILL оставляет stale socket-файл —
  это обрабатывает новая probe-логика компаньона (задача 2), SIGTERM-хендлер в
  питоне не нужен.
- Респавн-триггер: `child.try_wait()` == exited при галке on → backoff 5 с.
  Спавн-условие: `led_child.is_none() && led_socket.is_none() && backoff истёк`.
  Гейт `!connected` (сокет-клиент уже подключился) важен: если жив ЧУЖОЙ
  компаньон, наш спавн выйдет 0 по guard'у — без гейта получим вечный цикл
  «спавн python3 → exit 0» каждые 5 с. С гейтом: чужой жив → мы подключены →
  не спавним вообще.
- **При выходе синта child НЕ убивается** (осознанный выбор): компаньон держит
  BLE-коннект «на много synth-сессий» (pad_console.py:72), рестарт синта не
  должен рвать BLE. Дубликат при следующем старте тихо выйдет по guard'у.
- Существующий socket-клиент (connect/retry 2s/alive/heartbeat 2s, gui/mod.rs:
  580–627, 629–668) не меняется.

**Tech Stack:** std::process, dirs (уже есть), serde config — без новых зависимостей.

**Spec context:** docs/TECHDEBT.md п.16 (строки 276–291).

**Репозитории:** коммиты 1, 3, 4 — в mini_midi_synth; коммит 2 — в
`/home/dima/Projects/smk37_config` (отдельный репо, отдельный коммит).

**Проверки перед каждым коммитом (mini_midi_synth):**

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

Полные гейты перед финальным коммитом группы: добавить `cargo deny check`.
GUI-код (`src/gui/mod.rs`) не в RT-зоне, но гейт всё равно прогоняем.

---

## Test Cases (from the requirement)

1. Config без `led_companion_cmd` → дефолт `"~/Projects/smk37_config/pad_console.py"`
   (serde default fn).
2. `Config::default().ui.led_companion_cmd` — та же литеральная строка.
3. `expand_home("~/x")` при установленном HOME → `<HOME>/x`; без `~/`-префикса —
   путь как есть; `~` без слэша (`~foo`) — как есть.
4. Компаньон стартует, сокет занят живым компаньоном → печатает «already
   running», exit 0, НЕ unlink'ает чужой сокет (живой компаньон продолжает
   слушать).
5. Компаньон стартует, на диске stale socket-файл (мертвый слушатель) →
   unlink + bind успешен.
6. Сокет-файла нет → bind обычным порядком.
7. Галка on, наш child умер (exit) → child reaped, респавн не раньше 5 с.
8. Галка off → наш child убит и wait'нут (нет зомби); чужой процесс не убивается.
9. Сокет-клиент: connect/retry 2s/alive/heartbeat — поведение не изменилось
   (существующий код, регрессия по код-ревью + ручной чеклист).

## Negative Matrix

| Случай | Поведение |
|---|---|
| config без `led_companion_cmd` | serde default → литеральный `~/Projects/...` (TC1) |
| config `led_companion_cmd: ""` | `expand_home("")` → `PathBuf::from("")` → spawn fail → backoff 5 с, лог warning, не паника |
| путь не существует / python3 нет | `Command::spawn()` → `Err` → лог + backoff 5 с, вечный retry пока галка on (осознанно: «пульт должен просто работать») |
| `~`, но HOME не задан | `dirs::home_dir()` → None → путь как есть → spawn fail → backoff |
| чужой компаньон уже слушает | наш spawn → guard → exit 0; реап + гейт `!connected` → без цикла респавнов |
| stale socket-файл | probe-connect fail (ECONNREFUSED) → unlink → bind ок (TC5) |
| child убит извне (kill/pgrep kill) | `try_wait()` → Some → reaped → backoff 5 с → респавн (TC7) |
| toggle off, чужой процесс | kill только `led_child`; чужой не тронут (TC8) |
| toggle off→on быстро | led_child None, backoff сброшен при off → спавн сразу |
| дублирующий вызов kill | led_child.take() → второй вызов по None ничего не делает |
| synth exit при on | child живёт (документировано), дубликат при рестарте тихо выйдет |

---

## Task 1: config `led_companion_cmd` + `expand_home` + тесты

**Modify:** `src/config.rs`, `tests/config_led.rs`

### 1a. Красный тест

Append to `/home/dima/Projects/mini_midi_synth/tests/config_led.rs`:

```rust
#[test]
fn led_companion_cmd_defaults_to_literal_path() {
    let s: mini_midi_synth::config::UiSettings = serde_json::from_str(
        r#"{"window_width": 720.0, "window_height": 400.0, "last_preset": null}"#,
    )
    .unwrap();
    assert_eq!(s.led_companion_cmd, "~/Projects/smk37_config/pad_console.py");
}

#[test]
fn expand_home_expands_tilde_slash() {
    if dirs::home_dir().is_none() { return; } // HOME-less env: путь как есть
    // SECURITY NOTE: не читаем реальный HOME тестом — подменяем через канал.
    let p = mini_midi_synth::config::expand_home("~/sub/file.py").display().to_string();
    assert!(!p.starts_with('~'), "tilde must be expanded: {p}");
    assert!(p.ends_with("/sub/file.py"), "suffix preserved: {p}");
}

#[test]
fn expand_home_keeps_plain_paths() {
    assert_eq!(mini_midi_synth::config::expand_home("/abs/x.py"), std::path::PathBuf::from("/abs/x.py"));
    assert_eq!(mini_midi_synth::config::expand_home(""), std::path::PathBuf::from(""));
    assert_eq!(mini_midi_synth::config::expand_home("~foo"), std::path::PathBuf::from("~foo"));
}
```

Замечание по TC3: первый тест не завязан на конкретный HOME (CI/контейнеры без
HOME) — только инварианты «тильды нет», «суффикс на месте».

### 1b. Run → red

```bash
cargo test --test config_led
```
Ожидаемо: compile error — нет поля `led_companion_cmd` и функции `expand_home`.

### 1c. Implement

`src/config.rs` — после поля `led_companion` (якорь: строки 136–138):

```rust
    /// Bank-2 LED console companion (unix socket). Off = never connect.
    #[serde(default)]
    pub led_companion: bool,
    /// Companion spawn command path (python3 hardcoded); `~/` expands to HOME.
    #[serde(default = "default_led_companion_cmd")]
    pub led_companion_cmd: String,
```

Рядом с `fn default_looper_sync_bpm()` (якорь: строка 147) добавить:

```rust
fn default_led_companion_cmd() -> String { "~/Projects/smk37_config/pad_console.py".to_string() }

/// Expand a leading `~/` to the user home directory; everything else verbatim.
pub fn expand_home(p: &str) -> std::path::PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(p)
}
```

В `impl Default for Config` → `UiSettings` литерал (якорь: строка 191
`led_companion: false,`) добавить:

```rust
                led_companion: false,
                led_companion_cmd: default_led_companion_cmd(),
```

### 1d. Green + гейты

```bash
cargo test --test config_led
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
```

### 1e. Commit (mini_midi_synth)

```bash
git add src/config.rs tests/config_led.rs
git commit -m "feat(config): led_companion_cmd path setting with ~ expansion (TECHDEBT #16)"
```

---

## Task 2: pad_console.py double-run guard (ОТДЕЛЬНЫЙ РЕПО)

**Modify:** `/home/dima/Projects/smk37_config/pad_console.py` (якорь: `main()`,
строки 61–71; финальный unlink строки 106–109 не трогаем).

**Смысл:** текущий код сначала `os.unlink(SOCK)` (строка 66) — если живёт ЧУЖОЙ
компаньон, unlink вырвет у него слушающий сокет. Probe-connect ДО unlink:
connect успешен → живой компаньон → тихий exit 0; connect упал (стale/нет
файла) → unlink безопасен → bind.

### 2a. Implement (тест — TC4/5/6, шаг 2c)

В `main()` заменить:

```python
    bus = dbus.SystemBus()
    try:
        os.unlink(SOCK)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCK); srv.listen(1)
```

на:

```python
    bus = dbus.SystemBus()
    # double-run guard: probe BEFORE unlink — unlinking a LIVE companion's
    # socket would break it. connect ok  -> someone already serves; fall back
    # to this script? no: exit 0 quietly (synth will just use the live one).
    probe = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        probe.connect(SOCK)
        print("already running on", SOCK)
        return
    except OSError:
        pass  # no listener (absent file or stale socket) -> safe to take over
    finally:
        probe.close()
    try:
        os.unlink(SOCK)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    srv.bind(SOCK); srv.listen(1)
```

Замечания:
- probe ДОЛЖЕН закрываться в `finally` до unlink — иначе открытый fd держит
  файл.
- `except OSError` покрывает и `FileNotFoundError` (нет файла), и
  `ConnectionRefusedError` (stale socket после SIGKILL) — оба случая = «можно
  забирать».
- `--selftest` ветка (строки 62–63) стоит ДО guard'а — selftest не требует
  сокета, не ломается.

### 2b. Верификация без железа

```bash
cd ~/Projects/smk37_config
python3 pad_console.py --selftest
# TC5/TC6 manual: stale file
python3 -c "
import socket, os
s=socket.socket(socket.AF_UNIX, socket.SOCK_STREAM); s.bind('/tmp/mini_midi_synth_led.sock')
" # создаёт stale файл без listen; процесс умрёт, файл останется
python3 pad_console.py &   # должен напечатать listening (unlink stale + bind)
sleep 1
python3 pad_console.py     # должен напечатать "already running" и выйти 0
echo "exit=$?"
kill %1; rm -f /tmp/mini_midi_synth_led.sock
```
(TC4 — живой слушатель покрывается шагом «второй запуск → already running».)

### 2c. Commit (smk37_config repo!)

```bash
cd /home/dima/Projects/smk37_config
git add pad_console.py
git commit -m "fix: double-run guard — probe socket before unlink, exit 0 if live companion"
```

---

## Task 3: spawn/kill/respawn в gui/mod.rs

**Modify:** `src/gui/mod.rs` (поля App:334–342, lifecycle-блок:580–584),
`src/lib.rs` (конструктор App:790–795).

### 3a. Поля App

`src/gui/mod.rs`, якорь строки 339–342:

```rust
    pub led_socket: Option<std::sync::mpsc::Sender<String>>,
    pub led_socket_alive: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    pub led_socket_failed_at: Option<std::time::Instant>,
    pub led_last_send: Option<std::time::Instant>,
```

добавить после:

```rust
    /// Companion process WE spawned (never kill a foreign one).
    pub led_child: Option<std::process::Child>,
    /// Backoff gate for (re)spawn attempts (crash / spawn failure).
    pub led_spawn_backoff: Option<std::time::Instant>,
```

`src/lib.rs`, якорь строки 792–795 в литерале `gui::App {`:

```rust
        led_socket: None,
        led_socket_alive: None,
        led_socket_failed_at: None,
        led_last_send: None,
```

добавить после:

```rust
        led_child: None,
        led_spawn_backoff: None,
```

### 3b. Lifecycle-блок

`src/gui/mod.rs`, якорь строки 580 `// LED companion socket lifecycle`.
Вставить ПЕРЕД ним (существующий блок 581–617 не менять):

```rust
        // Companion process lifecycle (TECHDEBT #16): spawn on toggle-on /
        // first need, kill only OUR child on toggle-off, respawn with 5s
        // backoff if it dies while enabled.
        if !self.config.ui.led_companion {
            if let Some(mut child) = self.led_child.take() {
                log::info!("[console] killing companion child");
                if let Err(e) = child.kill() {
                    log::warn!("[console] companion kill: {e}");
                }
                if let Err(e) = child.wait() {
                    log::warn!("[console] companion wait: {e}");
                }
            }
            self.led_spawn_backoff = None;
        } else {
            // reap exited child (crash, foreign kill, guard-exit 0)
            if let Some(mut child) = self.led_child.take() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        log::info!("[console] companion child exited: {status}");
                        self.led_spawn_backoff = Some(std::time::Instant::now());
                    }
                    Ok(None) => self.led_child = Some(child),
                    Err(e) => log::warn!("[console] companion try_wait: {e}"),
                }
            }
            // Spawn only when nothing connected (a live foreign companion means
            // led_socket connects and we must not churn-spawn every 5s).
            let backoff_ok = self.led_spawn_backoff
                .map(|t| t.elapsed() > std::time::Duration::from_secs(5))
                .unwrap_or(true);
            if self.led_child.is_none()
                && self.led_socket.is_none()
                && backoff_ok
            {
                let path = crate::config::expand_home(&self.config.ui.led_companion_cmd);
                match std::process::Command::new("python3").arg(&path).spawn() {
                    Ok(child) => {
                        log::info!("[console] spawned companion: {}", path.display());
                        self.led_child = Some(child);
                    }
                    Err(e) => {
                        log::warn!("[console] companion spawn failed: {e}");
                        self.led_spawn_backoff = Some(std::time::Instant::now());
                    }
                }
            }
        }
```

**Почему это не ломает socket-клиент:** блок стоит выше; его гейт
`!self.config.ui.led_companion` (581) продолжает управлять сокетом; наш блок
спавнит процесс независимо от состояния сокета, а сокет-клиент сам
подконнектится в своём 2s-цикле после bind'а компаньона. Kill компаньона →
writer-тред умрёт → `socket_dead` → стандартный retry. Порядок в кадре:
spawn-блок читает `led_socket` ДО того, как сокет-блок в этом же кадре может
его установить — задержка на 1 кадр, не влияет.

**RT-безопасность:** код только на GUI-потоке (`App::update`), спавн/kill
аллоцируют — это нормально, аудио-поток не задет. `format!` в `log::info!` —
тоже GUI-поток.

### 3c. Компиляция + гейты

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
```

Спавн-логика unit-тестами не покрывается: GUI-поток + внешний процесс + dbus
SystemBus у компаньона (в CI/контейнере нет). Покрытие — Task 4 (ручной
чеклист) + ревью инвариантов в Negative Matrix.

### 3d. Commit

```bash
git add src/gui/mod.rs src/lib.rs
git commit -m "feat(console): auto-spawn/kill/respawn LED companion (TECHDEBT #16)"
```

---

## Task 4: Ручная верификация (юзер, железо)

Чеклист — прогнать после Task 3, до закрытия TECHDEBT #16:

- [ ] `cargo run` с `led_companion: true` в config → `pgrep -f pad_console`
      показывает процесс (наш spawn), клавиатурные LED красятся.
- [ ] Тогл off в Settings → в течение кадра процесс исчез (`pgrep -f
      pad_console` пуст), `ps -ef | grep pad_console` — нет `<defunct>`.
- [ ] Тогл on → процесс вернулся.
- [ ] `kill <PID>` нашего child при on → через ~5 с новый PID (`pgrep -f`),
      LED живут.
- [ ] Заранее запустить компаньон вручную (`python3
      ~/Projects/smk37_config/pad_console.py &`), затем синт с on → синт НЕ
      плодит цикл спавнов (`pgrep -f pad_console` — ровно один процесс,
      стабильный PID), LED работают через чужой.
- [ ] `led_companion_cmd: "/nonexistent"` → warning в логе раз в ~5 с, синт не
      падает, галка работает.
- [ ] Выход из синта при on → компаньон остаётся жив (BLE-коннект сохранён);
      повторный старт синта → дубликат тихо вышел, всё работает.
- [ ] Тест smk37_config (Task 2b) — guard-сценарии.

---

## Verification (полные гейты перед финалом)

```bash
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features rt-assert --test rt_safety
cargo deny check
cargo test --test config_led
```

Плюс чеклист Task 4. После прохождения — пометить TECHDEBT #16 решённой
(короткая заметка в docs/TECHDEBT.md: автоспавн реализован, см. план
2026-10-29-companion-autospawn.md) отдельным docs-коммитом.
