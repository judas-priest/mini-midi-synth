# LED Companion Reconnect (TECHDEBT #17) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

anchor-checked: 2026-11-03 (база = ~/Projects/smk37_config master f4966db + незакоммиченный рабочий фикс pad_console.py; spec = TECHDEBT #17 в mini_midi_synth, PR #66)

**ВАЖНО — другой репозиторий.** Код живёт в `~/Projects/smk37_config`
(отдельный git-репо, branch master). План лежит в mini_midi_synth, но ВСЕ
правки/коммиты — в smk37_config. Rust-сайд мини-синта НЕ меняется (пункты (б)/(в)
спеки закрыты аудитом #18: reconnect_audio не рвёт console_rx/LED-socket).

**Goal:** LED-компаньон (pad_console.py) самовосстанавливается после разрыва BT
клавиатуры: DBus-watch вместо слепого поллинга, экспоненциальный backoff,
Disconnect+Connect bounce для полумёртвого линка, различение «клава off» vs
«stale cache BlueZ».

**Architecture:** Чистая логика (`ReconnectPolicy` — backoff/решения, без dbus)
вынесена в тестируемый класс. Dbus-клей: glib-mainloop в daemon-потоке на
ПРИВАТНОМ SystemBus-коннекте слушает `PropertiesChanged(Device1.Connected)` →
`threading.Event`; основной поток по событию/таймауту делает явный
`Device1.Connect()` + probe-запись. Watchdog: 3 подряд неудачных записи при
Connected=true → полный bounce; 3 bounce без результата → лог-хинт юзеру.

**Tech Stack:** Python 3, dbus-python (raw, glib-mainloop уже в шапке модуля),
stdlib `threading`/`unittest`. Зависимости проверены: `python3 -c "import dbus, gi; from gi.repository import GLib"` → ok.

**Проверенные ревью/чтением факты (не пересматривать):**
- `pad_console.py` (128 строк): `ble_connect(bus)` :48-59 — probe-цикл sleep(3);
  BLE-lifetime loop :88-121; `paint()` :29-36 (60 ms pacing); незакоммиченный
  фикс probe-timeout/backlog уже в рабочем дереве (Task 0 коммитит его).
- `DBusGMainLoop(set_as_default=True)` уже в шапке (:11-12) — glib-mainloop
  можно крутить в отдельном потоке, сигналы будут диспатчиться туда.
- `dbus.exceptions.DBusException.get_dbus_name()` — имя ошибки
  (`org.bluez.Error.InProgress` и т.п.).
- `Device1.Connect()` на уже подключённом устройстве — успешный no-op.
- Писать и слушать ОДИН общий bus из двух потоков нельзя (dbus-python не
  thread-safe) — для watch используется `dbus.SystemBus(private=True)`.
- Адрес клавы: `BASE = "/org/bluez/hci0/dev_D8_31_0D_19_D1_0C"` (:19).

---

## Test Cases (from the requirement)

Требование: «консоль самовосстанавливается во всех комбинациях разрывов без
действий пользователя». Тестируемая единица — `ReconnectPolicy` (решения
backoff/bounce/hint) и классификация ошибок Connect. Экранные кейсы (BT
выкл/вкл, полумёртвый линк) — ручная верификация в Task 5.

- TC1: первый провал — wait, задержка 2.0 с (1×2).
- TC2: задержка растёт 1→2→4→8→16 и капается на 30 (не растёт дальше).
- TC3: успех — полный ресет (задержка 1.0, счётчики 0).
- TC4: 3 подряд неудачных записи при Connected=true → «bounce»; после bounce
  write_fails сброшен, счётчик bounces=1.
- TC5: ещё 3×3 неудачи (3 bounce суммарно) → следующий вердикт «hint».
- TC6: те же 3 неудачи при Connected=false → «wait» (клава выключена — bounce
  бессмыслен).
- TC7: классификатор Connect-ошибок: InProgress — не провал (не триггерит
  счётчики), остальные имена — провал.

## Negative Matrix

- пустое кольцо/нет полей — N/A (нет структур данных с полями-коллекциями).
- double-run компаньона — уже решено (guard закоммичен в f4966db — текущий
  HEAD; Task 0 коммитит только probe.settimeout(1) + listen(16)).
- duplicate concurrent invocation: watch-поток один (запуск в main до цикла);
  `Connect()` из основного потока и сигнал из watch-потока не конфликтуют —
  разные bus-коннекты.
- external-call timeout: `dev.Connect()` может зависнуть? — BlueZ отвечает
  ошибкой/сигналом, dbus-python вызов блокируется до reply; события DBus-watch
  не блокируются (другой коннект). Приём: probe после таймаута event.wait —
  основной поток не вечен.
- clock skew — N/A (нет сравнения часов между источниками; только threading.Event).
- stale cache BlueZ — покрывается «hint» (TC5): автоматом не чиним, лог-хинт.
- bus недоступен (BlueZ рестартует): watch-поток умрёт с исключением — daemon,
  основной цикл продолжит probe-пути по таймауту (graceful degradation).
  Отдельно не обрабатываем — задокументировано в комментарии watch-потока.
  Известная цена: без watch восстановление после BT-ON — по таймауту
  (до wait_delay(), в худшем случае 30 с), с живым watch — ~1-2 с.

---

### Task 0: Закоммитить рабочий фикс (baseline)

**Files:** Modify: `~/Projects/smk37_config/pad_console.py` (уже правлен в
рабочем дереве — probe.settimeout(1) + listen(16); см. git diff)

- [ ] Step 1: коммит

```bash
cd ~/Projects/smk37_config
git add pad_console.py
git commit -m "fix: probe timeout 1s + accept backlog 16 + timeout-tolerant guard — stalled peer / full backlog no longer hangs startup or synth GUI thread"
```

### Task 1: ReconnectPolicy — чистая логика + тесты (TDD)

**Files:**
- Modify: `~/Projects/smk37_config/pad_console.py` (класс в шапке, после импортов)
- Create: `~/Projects/smk37_config/tests/test_reconnect.py`
- Create: `~/Projects/smk37_config/tests/__init__.py` (пустой, чтобы unittest
  находил пакет)

- [ ] Step 1: падающий тест

```python
# tests/test_reconnect.py
"""TC из требования TECHDEBT #17: решения reconnect-политики."""
import sys, os, unittest
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
from pad_console import ReconnectPolicy


class TestReconnectPolicy(unittest.TestCase):
    def test_tc1_first_fail_waits_2s(self):
        pol = ReconnectPolicy()
        pol.on_probe_fail()
        act = pol.next_action(device_connected=True)
        self.assertEqual(act, "wait")
        self.assertEqual(pol.wait_delay(), 2.0)

    def test_tc2_delay_grows_and_caps(self):
        pol = ReconnectPolicy()
        seq = []
        for _ in range(7):
            pol.on_probe_fail()
            seq.append(pol.wait_delay())
        self.assertEqual(seq, [2.0, 4.0, 8.0, 16.0, 30.0, 30.0, 30.0])

    def test_tc3_success_resets(self):
        pol = ReconnectPolicy()
        for _ in range(5):
            pol.on_probe_fail()
        pol.on_success()
        self.assertEqual(pol.wait_delay(), 1.0)
        self.assertEqual(pol.next_action(device_connected=True), "wait")

    def test_tc4_three_write_fails_connected_bounces(self):
        pol = ReconnectPolicy()
        for _ in range(3):
            pol.on_probe_fail()
        act = pol.next_action(device_connected=True)
        self.assertEqual(act, "bounce")
        # после bounce: write_fails сброшен, счётчик bounces=1
        self.assertEqual(pol.next_action(device_connected=True), "wait")
        self.assertEqual(pol.bounces_done(), 1)

    def test_tc5_three_bounces_then_hint(self):
        pol = ReconnectPolicy()
        for _round in range(3):
            for _ in range(3):
                pol.on_probe_fail()
            self.assertEqual(pol.next_action(device_connected=True), "bounce")
        # 4-я серия — уже hint, не bounce
        for _ in range(3):
            pol.on_probe_fail()
        self.assertEqual(pol.next_action(device_connected=True), "hint")

    def test_tc6_not_connected_never_bounces(self):
        pol = ReconnectPolicy()
        for _ in range(6):
            pol.on_probe_fail()
        self.assertEqual(pol.next_action(device_connected=False), "wait")
        self.assertEqual(pol.bounces_done(), 0)

    def test_tc7_in_progress_not_a_fail(self):
        pol = ReconnectPolicy()
        self.assertFalse(pol.connect_failed("org.bluez.Error.InProgress"))
        self.assertTrue(pol.connect_failed("org.bluez.Error.Failed"))
        # InProgress не двигает счётчики
        self.assertEqual(pol.wait_delay(), 1.0)


if __name__ == "__main__":
    unittest.main()
```

- [ ] Step 2: прогнать, убедиться в падении (AttributeError: ReconnectPolicy)

```bash
cd ~/Projects/smk37_config && python3 -m unittest tests.test_reconnect -v
```

Expected: ImportError/AttributeError — класса нет.

- [ ] Step 3: минимальная реализация — в `pad_console.py` после `import socket, os, sys, time` (:15):

```python
class ReconnectPolicy:
    """Решения переподключения: экспоненциальный backoff + bounce/hint.
    Чистая логика без dbus — юнит-тесты tests/test_reconnect.py.
    Спека: TECHDEBT #17 (mini_midi_synth), best practices 2026-11-03."""

    MAX_DELAY = 30.0
    BOUNCE_AFTER = 3   # подряд неудачных записей при Connected=true -> bounce
    HINT_AFTER = 3     # bounce без результата -> stale-cache hint

    def __init__(self):
        self.delay = 1.0
        self.write_fails = 0
        self.bounces = 0

    def on_success(self):
        self.delay = 1.0
        self.write_fails = 0
        self.bounces = 0

    def on_probe_fail(self):
        self.write_fails += 1
        self.delay = min(self.delay * 2, self.MAX_DELAY)

    @staticmethod
    def connect_failed(dbus_error_name):
        """True — реальный провал Connect; InProgress (попытка уже идёт) — нет.
        Вызов: ble_connect делает raise при True — счёт посчитает общий except
        (ровно один on_probe_fail на итерацию)."""
        return dbus_error_name != "org.bluez.Error.InProgress"

    def wait_delay(self):
        return self.delay

    def bounces_done(self):
        return self.bounces

    def next_action(self, device_connected):
        """МУТИРУЮЩИЙ вызов: bounce сбрасывает write_fails и инкрементит
        bounces. Вызывать РОВНО ОДИН раз на вердикт (двойной вызов = двойной
        счёт bounces)."""
        if device_connected and self.write_fails >= self.BOUNCE_AFTER:
            if self.bounces >= self.HINT_AFTER:
                return "hint"
            self.write_fails = 0
            self.bounces += 1
            return "bounce"
        return "wait"
```

- [ ] Step 4: прогнать — 7 тестов зелёные

```bash
cd ~/Projects/smk37_config && python3 -m unittest tests.test_reconnect -v
```

Expected: `OK` (7 tests). Замечание: импорт pad_console исполняет
`DBusGMainLoop(set_as_default=True)` — на машине с dbus это безопасно.

- [ ] Step 5: коммит

```bash
cd ~/Projects/smk37_config && git add pad_console.py tests/
git commit -m "feat: ReconnectPolicy — exponential backoff + bounce/hint decisions (pure logic, unit-tested)"
```

### Task 2: DBus-watch Connected + device-хелперы

**Files:** Modify: `~/Projects/smk37_config/pad_console.py` (новые функции перед
`ble_connect`; `import threading` + `from gi.repository import GLib` в шапке)

- [ ] Step 1: импорты в шапке (после `import socket, os, sys, time`):

```python
import threading
from gi.repository import GLib
```

- [ ] Step 2: хелперы перед `def ble_connect`:

```python
def device_iface(bus):
    return Interface(bus.get_object(BUS, BASE), "org.bluez.Device1")

def connected_now(bus):
    # Метод Properties — Get (с большой буквы) через org.freedesktop.DBus.Properties.
    # Нижний .get() на dbus.Interface resolves в неизвестный DBus-метод "get"
    # (UnknownMethod) и тихо всегда возвращал бы False.
    try:
        props = Interface(bus.get_object(BUS, BASE), "org.freedesktop.DBus.Properties")
        return bool(props.Get("org.bluez.Device1", "Connected"))
    except Exception:
        return False

def start_connected_watch():
    """DBus-watch Connected клавы (паттерн BlueZ вместо слепого поллинга).
    Отдельный приватный bus-коннект: dbus-python не thread-safe, писать
    (paint/Connect) и слушать в одном коннекте из двух потоков нельзя.
    Если BlueZ рестартует и watch умрёт — daemon-поток, основной цикл
    продолжает работать по таймаутам (graceful degradation)."""
    evt = threading.Event()

    def run():
        try:
            wbus = dbus.SystemBus(private=True)

            def on_props(iface, changed, invalidated, path=None):
                if iface == "org.bluez.Device1" and changed.get("Connected"):
                    evt.set()

            wbus.add_signal_receiver(
                on_props,
                dbus_interface="org.freedesktop.DBus.Properties",
                signal_name="PropertiesChanged",
                path=BASE,
                path_keyword="path")
            GLib.MainLoop().run()
        except Exception as e:
            print("watch thread died (reconnect falls back to timeouts):", e)

    threading.Thread(target=run, daemon=True).start()
    return evt
```

- [ ] Step 3: smoke — скрипт стартует, watch-поток жив

```bash
cd ~/Projects/smk37_config && python3 -c "
import pad_console as pc, time
bus = pc.dbus.SystemBus()
evt = pc.start_connected_watch()
time.sleep(1)
print('watch alive:', any(t.name != 'MainThread' and t.daemon for t in pc.threading.enumerate()))
print('connected_now:', pc.connected_now(bus))
"
```

Expected: `watch alive: True`, `connected_now: True` без трейсбеков (клава
подключена; если False при подключённой клаве — НЕ глотать, разбираться:
правильность Properties.Get критична для bounce/hint).

- [ ] Step 4: коммит

```bash
cd ~/Projects/smk37_config && git add pad_console.py
git commit -m "feat: Device1.PropertiesChanged watch on private bus — instant push instead of blind 3s probe"
```

### Task 3: ble_connect — явный Connect + bounce + hint + backoff

**Files:** Modify: `~/Projects/smk37_config/pad_console.py` — заменить
`ble_connect` (:48-59) и вызов в main (:89)

- [ ] Step 1: добавить bounce перед ble_connect:

```python
def bounce_device(bus):
    """Полумёртвый линк: полный Disconnect+Connect — документированный
    надёжный стопгэп (bluez #2321); ConnectProfile не спасает."""
    dev = device_iface(bus)
    try:
        dev.Disconnect()
    except Exception:
        pass
    time.sleep(0.5)
    try:
        dev.Connect()
    except dbus.exceptions.DBusException as e:
        if ReconnectPolicy.connect_failed(e.get_dbus_name() or ""):
            print("connect failed:", e)
```

- [ ] Step 2: заменить ble_connect целиком:

```python
def ble_connect(bus, connected_evt):
    pol = ReconnectPolicy()
    hinted = False
    while True:
        try:
            dev = device_iface(bus)
            try:
                dev.Connect()  # уже подключена — успешный no-op
            except dbus.exceptions.DBusException as e:
                if pol.connect_failed(e.get_dbus_name() or ""):
                    raise  # провал посчитает общий except — ровно один счёт
                # InProgress: попытка уже идёт — просто ждём как обычно
            # push по PropertiesChanged или таймаут — в обоих случаях пробуем.
            # Известная гонка (benign): evt между wait и clear может прилететь
            # и быть стёрт — потеряется один пуш, следующий цикл отработает
            # по таймауту. Приём события до регистрации receiver тоже может
            # быть пропущен (поток стартует асинхронно) — тот же fallback.
            connected_evt.wait(timeout=pol.wait_delay())
            connected_evt.clear()
            iface = Interface(bus.get_object(BUS, CHAR), "org.bluez.GattCharacteristic1")
            # probe: commit-frame как в smk.py (cp = 05 00 ... 00)
            iface.WriteValue(dbus.Array([dbus.Byte(b) for b in frame(0x22, bytes([0x05, 0, 0, 0, 0, 0, 0, 0]))], signature="y"),
                             {"type": "command"})
            pol.on_success()
            print("BLE ready")
            return iface
        except Exception as e:
            pol.on_probe_fail()
            act = pol.next_action(connected_now(bus))
            if act == "bounce":
                print("link half-dead -> bounce")
                bounce_device(bus)
            elif act == "hint" and not hinted:
                hinted = True
                print("HINT: 3 bounces failed — BlueZ cache may be stale;")
                print("  try: bluetoothctl remove D8:31:0D:19:D1:0C then re-pair")
            time.sleep(0.2)  # не долбить bus; пауза — wait_delay() в evt.wait
```

(Смысловая разница с прежним: слепой sleep(3) → событие/таймаут с backoff;
явный Connect() вместо ожидания чужой инициативы; bounce/hint по политике.
`last = [None] * 16` в main остаётся — сброс кэша цветов при новом iface.)

- [ ] Step 3: в main (:88-89) — старт watch и передача события:

```python
    connected_evt = start_connected_watch()
    while True:  # BLE lifetime loop: один BLE-коннект на много synth-сессий
        iface = ble_connect(bus, connected_evt)
```

- [ ] Step 4: selftest + юнит-тесты зелёные

```bash
cd ~/Projects/smk37_config && python3 pad_console.py --selftest && python3 -m unittest tests.test_reconnect -v
```

Expected: `selftest ok`, `OK (7 tests)`.

- [ ] Step 5: коммит

```bash
cd ~/Projects/smk37_config && git add pad_console.py
git commit -m "feat: reconnect loop — explicit Connect, exponential backoff, half-dead link bounce, stale-cache hint (TECHDEBT #17)"
```

### Task 4: Деплой-совместимость (живой компаньон)

**Files:** нет правок. Синт спавнит компаньона только при
`led_socket_failed_at.is_some()` + backoff 5s (gui/mod.rs:791-830) — новый
компаньон подхватится сам после перезапуска старого.

- [ ] Step 1: убить старый компаньон, убедиться что синт заспавнит новый

```bash
pkill -f pad_console.py; sleep 8; pgrep -af pad_console.py
```

Expected: процесс появился (синт переспавнил) либо отсутствует, если
`led_companion` выключен в конфиге — тогда запуск вручную:

```bash
python3 ~/Projects/smk37_config/pad_console.py &
```

- [ ] Step 2: юзер рисует на пэдах — LED красятся как раньше.

### Task 5: Ручная верификация (экран, из требования)

- [ ] BT клавы OFF → LED замирают, в логе компаньона backoff-цикл, синт работает
- [ ] BT клавы ON → перекраска в течение ~1-2 с (DBus-watch пуш, не 3 с поллинг)
- [ ] быстрый OFF/ON x3 подряд — восстановление каждый раз, без дублей компаньона
- [ ] (если воспроизводим) полумёртвый линк: лог `link half-dead -> bounce` и восстановление
- [ ] цветов/гейта без изменений: прежние паттерны, 60 ms pacing не тронут

## Out of scope

- Rust-сайд (console_rx переживает reconnect_audio — закрыто аудитом #18).
- Автоматический RemoveDevice при stale cache — только лог-хинт (тяжёлое
  средство,Spec: «автоматом не делать»).
- Переписывание на bleak/bluer — raw dbus остаётся.
